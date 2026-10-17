use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;

use oath_core::{converge_with_link_root, promote_store, write_json};

use crate::cpio;
use crate::output::{self, Output};
use crate::tools::Tools;
use crate::util::{chmod_exec, copy_file, copy_tree, run, sudo};

/// Native tools the installer runs over SSH (static binaries): pack and
/// file under its `bin/`.
pub(crate) const INSTALLER_TOOLS: &[(&str, &str)] = &[
    ("dropbear", "dropbear"),
    ("dropbear", "dropbearkey"),
    ("btrfs", "btrfs"),
    ("btrfs", "mkfs.btrfs"),
    ("util-linux", "sfdisk"),
    ("util-linux", "wipefs"),
    ("dosfstools", "mkfs.fat"),
];

/// Pack the image for one seed profile (`base`, `dev`) into a build output
/// (`build/out/<id>/`, see output.rs), or reuse the output of an unchanged
/// tree. The profile's present packages are packed; anything else it
/// names stays absent. `out` is the build directory.
pub fn build(root: &Path, out: &Path, tools: &Tools, profile: &str) -> Result<Output> {
    let wanted =
        oath_core::Profile::load(profile).map_err(|e| anyhow::anyhow!("{e}"))?.present_pkgs();
    for name in &wanted {
        if name != "oath" && !tools.packs.contains_key(name) {
            bail!("profile {profile} wants pkg:{name}, which this build cannot pack yet");
        }
    }
    // The Oath binaries: pkg:oath, this tree built by pkg:rust.
    let oath = crate::oath_pkg::build(root)?;
    let manifest = crate::manifest::make(root, tools, profile)?;
    let id = output::id(&manifest);
    if let Some(o) = output::reuse(out, &id)? {
        eprintln!("output {} (unchanged tree)", o.dir.display());
        return Ok(o);
    }
    let dir = output::begin(out, &id)?;
    eprintln!("profile={profile} output={id}");
    eprintln!("kernel={}", tools.kernel.display());
    eprintln!("modules={}", tools.modules.display());
    eprintln!("packs={}", tools.packs.keys().cloned().collect::<Vec<_>>().join(" "));

    let lib = oath.join("lib/oath");
    write_initrd(&dir, tools, &lib.join("init"))?;

    eprintln!(">> installer initramfs");
    let ir_install = dir.join("initramfs-install");
    let _ = fs::remove_dir_all(&ir_install);
    copy_tree(&dir.join("initramfs"), &ir_install)?;
    for &(pack, file) in INSTALLER_TOOLS {
        let dest = ir_install.join("bin").join(file);
        let _ = fs::remove_file(&dest);
        copy_file(&tools.bin(pack, file)?, &dest)?;
        chmod_exec(&dest)?;
    }
    // The installer writes the new catalog with `oath seed --overlay`.
    copy_file(&oath.join("bin/oath"), &ir_install.join("bin/oath"))?;
    chmod_exec(&ir_install.join("bin/oath"))?;
    let _ = fs::remove_file(ir_install.join("bin/tar"));
    symlink("busybox", ir_install.join("bin/tar"))?;
    fs::create_dir_all(ir_install.join("opt/oath-install"))?;
    copy_file(&dir.join("initrd.gz"), &ir_install.join("opt/oath-install/initrd.gz"))?;
    copy_file(&tools.kernel, &ir_install.join("opt/oath-install/vmlinuz"))?;
    // oath-efi is the boot loader (menu, BLS entries, kernel + initrd).
    copy_file(&efi(&oath), &ir_install.join("opt/oath-install/BOOTX64.EFI"))?;
    fs::create_dir_all(ir_install.join("lib/oath"))?;
    fs::write(ir_install.join("lib/oath/udhcpc.script"), include_str!("udhcpc.script"))?;
    chmod_exec(&ir_install.join("lib/oath/udhcpc.script"))?;
    // busybox applets the install steps run over SSH (`dd` writes files).
    for a in [
        "ip", "udhcpc", "mount", "umount", "mkdir", "mdev", "blockdev", "reboot", "sync", "sleep",
        "cp", "cat", "sh", "dd", "rm",
    ] {
        let _ = fs::remove_file(ir_install.join("bin").join(a));
        symlink("busybox", ir_install.join("bin").join(a))?;
    }
    fs::create_dir_all(ir_install.join("root/.ssh"))?;
    let initrd_install = write_cpio_gz(&ir_install, &dir.join("initrd-install.gz"))?;
    eprintln!("initrd-install {}", initrd_install.display());

    eprintln!(">> stage rootfs");
    let stage = dir.join("stage");
    let _ = fs::remove_dir_all(&stage);
    for d in [
        "bin", "sbin", "lib/oath", "etc", "root", "home", "tmp", "proc", "sys", "dev", "run",
        "oath", "lib",
    ] {
        fs::create_dir_all(stage.join(d))?;
    }
    for n in ["init", "serial-login", "sudo"] {
        copy_file(&lib.join(n), &stage.join("lib/oath").join(n))?;
    }
    fs::write(stage.join("lib/oath/udhcpc.script"), include_str!("udhcpc.script"))?;
    fs::write(stage.join("lib/oath/ntp-event"), include_str!("ntp-event"))?;
    fs::write(stage.join("lib/oath/backup-send"), include_str!("backup-send"))?;
    fs::write(stage.join("lib/oath/backup-daily"), include_str!("backup-daily"))?;
    chmod_exec(&stage.join("lib/oath/init"))?;
    chmod_exec(&stage.join("lib/oath/serial-login"))?;
    chmod_exec(&stage.join("lib/oath/udhcpc.script"))?;
    chmod_exec(&stage.join("lib/oath/ntp-event"))?;
    chmod_exec(&stage.join("lib/oath/backup-send"))?;
    chmod_exec(&stage.join("lib/oath/backup-daily"))?;
    fs::set_permissions(stage.join("lib/oath/sudo"), fs::Permissions::from_mode(0o4755))?;
    let _ = fs::remove_file(stage.join("sbin/init"));
    symlink("../lib/oath/init", stage.join("sbin/init"))?;
    let _ = fs::remove_file(stage.join("lib/oath/modprobe"));
    symlink("init", stage.join("lib/oath/modprobe"))?;
    let _ = fs::remove_file(stage.join("lib/oath/net-lease"));
    symlink("init", stage.join("lib/oath/net-lease"))?;
    let _ = fs::remove_file(stage.join("bin/sudo"));
    symlink("../lib/oath/sudo", stage.join("bin/sudo"))?;
    fs::write(stage.join("etc/passwd"), oath_core::seat::passwd_file())?;
    fs::write(stage.join("etc/group"), oath_core::seat::group_file())?;
    fs::write(stage.join("etc/shadow"), oath_core::seat::shadow_file())?;
    fs::write(stage.join("etc/shells"), oath_core::seat::shells_file())?;
    fs::write(
        stage.join("etc/nsswitch.conf"),
        "passwd: files\ngroup: files\nshadow: files\nhosts: files dns\n",
    )?;
    fs::write(stage.join("etc/hosts"), "127.0.0.1 localhost\n::1 localhost\n127.0.1.1 oath\n")?;
    fs::create_dir_all(stage.join("etc/ssh"))?;
    fs::write(
        stage.join("etc/ssh/ssh_config"),
        "# guest OpenSSH client. Record unknown hosts; refuse changed keys.\n\
         Host *\n\
         \tStrictHostKeyChecking accept-new\n",
    )?;
    fs::create_dir_all(stage.join("var/run"))?;
    fs::create_dir_all(stage.join("root/.ssh"))?;
    fs::create_dir_all(stage.join("home/.ssh"))?;
    run(Command::new(oath.join("bin/oath")).args([
        "--root",
        stage.join("oath").to_str().unwrap(),
        "seed",
        "--profile",
        profile,
    ]))?;

    let oath_root = stage.join("oath");
    let guest_bin = stage.join("bin");
    // The profile's packages as they were built: native packs
    // (pkgs/lock.json), and pkg:oath from this tree.
    for name in &wanted {
        let tree = if name == "oath" { oath.as_path() } else { tools.pack(name)? };
        copy_tree(tree, &oath_root.join("store/pkg").join(name))?;
        link_pkg(&oath_root, &guest_bin, name)?;
    }

    let cache = host_store_cache(root);
    eprintln!(">> hash-in-path store ({})", cache.display());
    let pins = promote_store(&oath_root, Some(&cache)).map_err(|e| anyhow::anyhow!("{e}"))?;
    for (n, h) in &pins {
        eprintln!("   pkg:{n} {h}");
    }
    relink_present(&oath_root, &guest_bin)?;
    oath_core::sync_need_hashes(&oath_root).map_err(|e| anyhow::anyhow!("{e}"))?;
    settle_pkg_applied(&oath_root)?;

    eprintln!(">> rootfs (btrfs subvol @) — loop-mount needs root");
    let raw = dir.join("root.raw");
    let qcow = dir.join("oath.qcow2");
    let _ = fs::remove_file(&raw);
    let _ = fs::remove_file(&qcow);
    run(Command::new(&tools.qemu_img).args(["create", "-f", "raw", raw.to_str().unwrap(), "4G"]))?;
    run(Command::new("mkfs.btrfs").args(["-q", "-L", "oath", raw.to_str().unwrap()]))?;
    let mnt = dir.join("mnt");
    let rootfs = dir.join("rootfs");
    fs::create_dir_all(&mnt)?;
    fs::create_dir_all(&rootfs)?;
    // nix-shell puts busybox `mount` first; that binary does not
    // understand `subvol=@`. Use the host util-linux wrappers.
    let mount = host_mount();
    let umount = host_umount();
    sudo(&[mount.as_str(), "-o", "loop", raw.to_str().unwrap(), mnt.to_str().unwrap()])?;
    let mount_ok = (|| -> Result<()> {
        sudo(&["btrfs", "subvolume", "create", mnt.join("@").to_str().unwrap()])?;
        sudo(&[umount.as_str(), mnt.to_str().unwrap()])?;
        sudo(&[
            mount.as_str(),
            "-o",
            "loop,subvol=@",
            raw.to_str().unwrap(),
            rootfs.to_str().unwrap(),
        ])?;
        let stage_dot = format!("{}/.", stage.display());
        sudo(&["cp", "-a", &stage_dot, &format!("{}/", rootfs.display())])?;
        sudo(&["chown", "-R", "0:0", rootfs.to_str().unwrap()])?;
        sudo(&[
            "chown",
            "-R",
            &format!("{}:{}", oath_core::seat::UID, oath_core::seat::GID),
            rootfs.join("home").to_str().unwrap(),
        ])?;
        sudo(&["chmod", "4755", rootfs.join("lib/oath/sudo").to_str().unwrap()])?;
        sudo(&[umount.as_str(), rootfs.to_str().unwrap()])?;
        Ok(())
    })();
    let _ = sudo(&[umount.as_str(), mnt.to_str().unwrap()]);
    mount_ok?;
    run(Command::new(&tools.qemu_img).args([
        "convert",
        "-f",
        "raw",
        "-O",
        "qcow2",
        raw.to_str().unwrap(),
        qcow.to_str().unwrap(),
    ]))?;
    let _ = fs::remove_file(&raw);
    let uid = format!("{}", nix_uid());
    let gid = format!("{}", nix_gid());
    let _ = sudo(&["chown", "-R", &format!("{uid}:{gid}"), dir.to_str().unwrap()]);
    let bz = dir.join("bzImage");
    let _ = fs::remove_file(&bz);
    copy_file(&tools.kernel, &bz)?;
    copy_file(&efi(&oath), &dir.join("BOOTX64.EFI"))?;
    crate::util::write_pretty(&dir.join("manifest.json"), &manifest)?;
    // Packing scratch: the boot initrd's tree and the mount points.
    for scratch in ["initramfs", "mnt", "rootfs"] {
        let _ = fs::remove_dir_all(dir.join(scratch));
    }
    let o = output::finish(out, &id, &dir)?;
    eprintln!("output {}", o.dir.display());
    eprintln!("next: cargo make check, then cargo make install");
    Ok(o)
}

/// `oath-efi` inside pkg:oath's tree.
pub(crate) fn efi(oath: &Path) -> PathBuf {
    oath.join("boot/BOOTX64.EFI")
}

fn write_initrd(out: &Path, tools: &Tools, init: &Path) -> Result<()> {
    eprintln!(">> initramfs");
    let ir = out.join("initramfs");
    let _ = fs::remove_dir_all(&ir);
    for d in ["bin", "dev", "proc", "sys", "newroot", "lib/modules"] {
        fs::create_dir_all(ir.join(d))?;
    }
    copy_file(init, &ir.join("init"))?;
    chmod_exec(&ir.join("init"))?;
    copy_file(&tools.bin("busybox", "busybox")?, &ir.join("bin/busybox"))?;
    chmod_exec(&ir.join("bin/busybox"))?;
    let _ = fs::remove_file(ir.join("bin/sh"));
    symlink("busybox", ir.join("bin/sh"))?;

    let kver = crate::kernel::check(&tools.kernel, &tools.modules)?;
    eprintln!("kernel release {kver} (bzImage, modules and vermagic agree)");
    let kdir = tools.modules.join(&kver);
    let n = copy_modules(&kdir, &ir.join("lib/modules").join(&kver))?;
    eprintln!("{n} modules with their index (PID 1 loads by modalias)");
    // The kernel's modprobe helper is PID 1's own binary.
    fs::create_dir_all(ir.join("lib/oath"))?;
    symlink("/init", ir.join("lib/oath/modprobe"))?;

    copy_firmware(tools, &ir)?;
    let initrd = write_cpio_gz(&ir, &out.join("initrd.gz"))?;
    eprintln!("initrd {}", initrd.display());
    Ok(())
}

/// Strip `.ko.{zst,xz,gz}` down to `.ko` (Ubuntu mainline, Nix, Debian).
pub(crate) fn ko_path(p: &str) -> String {
    let mut p = p.trim().to_string();
    loop {
        let next = p
            .strip_suffix(".zst")
            .or_else(|| p.strip_suffix(".xz"))
            .or_else(|| p.strip_suffix(".gz"));
        match next {
            Some(s) => p = s.to_string(),
            None => break,
        }
    }
    p
}

fn zstd_bin() -> PathBuf {
    crate::util::which("zstd")
        .or_else(|| {
            let p = PathBuf::from("/tmp/zstd");
            p.is_file().then_some(p)
        })
        .unwrap_or_else(|| PathBuf::from("zstd"))
}

pub(crate) fn find_module(kdir: &Path, rel: &str) -> Option<PathBuf> {
    let rel = ko_path(rel);
    for cand in [rel.clone(), format!("{rel}.xz"), format!("{rel}.zst"), format!("{rel}.gz")] {
        let p = kdir.join(&cand);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

pub(crate) fn decompress_module(src: &Path) -> Result<Vec<u8>> {
    let name = src.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name.ends_with(".zst") {
        let out = Command::new(zstd_bin()).args(["-d", "-c"]).arg(src).output().context("zstd")?;
        if !out.status.success() {
            bail!("zstd -d {} failed", src.display());
        }
        return Ok(out.stdout);
    }
    if name.ends_with(".xz") {
        let out = Command::new("xz").args(["-d", "-c"]).arg(src).output().context("xz")?;
        if !out.status.success() {
            bail!("xz -d {} failed", src.display());
        }
        return Ok(out.stdout);
    }
    if name.ends_with(".gz") {
        let out = Command::new("gzip").args(["-d", "-c"]).arg(src).output().context("gzip")?;
        if !out.status.success() {
            bail!("gzip -d {} failed", src.display());
        }
        return Ok(out.stdout);
    }
    fs::read(src).with_context(|| format!("read {}", src.display()))
}

/// Every module the kernel config builds (`modules.order`), uncompressed,
/// with depmod's index. `modules.dep` names the uncompressed paths.
fn copy_modules(kdir: &Path, dst: &Path) -> Result<usize> {
    let order = fs::read_to_string(kdir.join("modules.order")).context("modules.order")?;
    let mut n = 0;
    for rel in order.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let src = find_module(kdir, rel)
            .with_context(|| format!("{rel} is in modules.order but missing"))?;
        let to = dst.join(ko_path(rel));
        fs::create_dir_all(to.parent().context("module path")?)?;
        fs::write(&to, decompress_module(&src)?)?;
        n += 1;
    }
    let dep = fs::read_to_string(kdir.join("modules.dep")).context("modules.dep")?;
    fs::write(dst.join("modules.dep"), plain_dep(&dep))?;
    for f in ["modules.alias", "modules.softdep", "modules.builtin"] {
        copy_file(&kdir.join(f), &dst.join(f)).with_context(|| f.to_string())?;
    }
    Ok(n)
}

/// `modules.dep` with `.ko.xz` / `.ko.zst` paths cut to `.ko`.
fn plain_dep(dep: &str) -> String {
    dep.lines()
        .map(|l| match l.split_once(':') {
            Some((m, deps)) => {
                let deps: String =
                    deps.split_whitespace().map(|d| format!(" {}", ko_path(d))).collect();
                format!("{}:{deps}\n", ko_path(m))
            }
            None => format!("{l}\n"),
        })
        .collect()
}

fn write_cpio_gz(tree: &Path, dest: &Path) -> Result<PathBuf> {
    let f = fs::File::create(dest)?;
    let mut gz = GzEncoder::new(f, Compression::best());
    cpio::write_tree(&mut gz, tree)?;
    gz.finish()?;
    Ok(dest.to_path_buf())
}

fn copy_firmware(tools: &Tools, ir: &Path) -> Result<()> {
    if let Some(fw) = tools.firmware() {
        copy_tree(&fw, &ir.join("lib/firmware"))?;
    }
    Ok(())
}

/// The output's installer initrd with `keys` as root's authorized keys,
/// written to `run/initrd-install.gz` (the output stays as it is).
pub fn bake_install_keys(o: &Output, run: &Path, keys: &str) -> Result<PathBuf> {
    let tree = o.file("initramfs-install");
    if !tree.join("init").is_file() {
        bail!("output {} has no installer initramfs", o.id);
    }
    let baked = run.join("initramfs-install-keys");
    let _ = fs::remove_dir_all(&baked);
    copy_tree(&tree, &baked)?;
    fs::create_dir_all(baked.join("root/.ssh"))?;
    fs::set_permissions(baked.join("root/.ssh"), fs::Permissions::from_mode(0o700))?;
    fs::write(baked.join("root/.ssh/authorized_keys"), keys)?;
    fs::set_permissions(
        baked.join("root/.ssh/authorized_keys"),
        fs::Permissions::from_mode(0o600),
    )?;
    let gz = write_cpio_gz(&baked, &run.join("initrd-install.gz"))?;
    let _ = fs::remove_dir_all(&baked);
    Ok(gz)
}

/// Transitive closure + topological load order from `modules.dep`.
/// Output paths have `.xz` stripped (pack stores uncompressed `.ko`).

/// Link `pkg:<name>` into the stage's `/bin`. No status is written: the
/// machine observes it at boot (R14).
fn link_pkg(oath_root: &Path, bin_dir: &Path, name: &str) -> Result<()> {
    let hash = desired_hash(oath_root, name);
    converge_with_link_root(oath_root, bin_dir, Path::new("/oath"), name, true, &hash)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

fn desired_hash(oath_root: &Path, name: &str) -> String {
    let p = oath_root.join("objects/pkg").join(name).join("desired.json");
    let Ok(v) = oath_core::read_json::<serde_json::Value>(&p) else {
        return String::new();
    };
    v.get("hash").and_then(|h| h.as_str()).unwrap_or("").to_string()
}

fn relink_present(oath_root: &Path, bin_dir: &Path) -> Result<()> {
    let dir = oath_root.join("objects/pkg");
    if !dir.is_dir() {
        return Ok(());
    }
    let mut names: Vec<String> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        let desired = oath_root.join("objects/pkg").join(&name).join("desired.json");
        let Ok(v) = oath_core::read_json::<serde_json::Value>(&desired) else {
            continue;
        };
        if v.get("present").and_then(|p| p.as_bool()) != Some(true) {
            continue;
        }
        link_pkg(oath_root, bin_dir, &name)?;
    }
    Ok(())
}

/// Keep every `pkg`'s desired doc (with its pin) as applied, the way apply
/// would: the image links what its seed pins, so it starts in sync.
fn settle_pkg_applied(oath_root: &Path) -> Result<()> {
    let dir = oath_root.join("objects/pkg");
    for e in fs::read_dir(&dir)?.flatten() {
        let d = e.path();
        let desired: serde_json::Value = oath_core::read_json(&d.join("desired.json"))
            .map_err(|e| anyhow::anyhow!("{}: {e}", d.display()))?;
        write_json(&d.join("applied.json"), &desired).map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    Ok(())
}

pub fn host_store_cache(repo: &Path) -> PathBuf {
    std::env::var("OATH_STORE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo.join(".cache/oath/store"))
}

/// Install a pack directory into the host cache and print the hash.
pub fn store_pack(repo: &Path, name: &str, from: &Path, tar: bool) -> Result<()> {
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        bail!("bad pkg name {name}");
    }
    let cache = host_store_cache(repo);
    let hash = oath_core::hash_tree(from).map_err(|e| anyhow::anyhow!("{e}"))?;
    let tree = oath_core::put_tree(&cache.join("pkg").join(name), &hash, from)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    println!("pkg:{name} {hash}");
    println!("{}", tree.display());
    if tar {
        let tarball =
            write_pack_tar(&tree, &cache.join("pkg").join(name).join(format!("{hash}.tar")))?;
        println!("{}", tarball.display());
    }
    Ok(())
}

fn write_pack_tar(tree: &Path, tarball: &Path) -> Result<PathBuf> {
    if let Some(p) = tarball.parent() {
        fs::create_dir_all(p)?;
    }
    run(Command::new("tar").args([
        "-C",
        tree.to_str().unwrap(),
        "-cf",
        tarball.to_str().unwrap(),
        ".",
    ]))?;
    Ok(tarball.to_path_buf())
}

/// `cargo make publish`: copy into an origin tree (`pkg/<name>/<hash>.tar`,
/// `adv/<name>/<hash>.json`, `INDEX.md`) every package a seed profile
/// names, at the hash this tree records for it (`pkgs/lock.json`, or
/// pkg:oath's last build), each with a licence on record, and beside it
/// what it was built from (R13): the plan (`plan/<name>/<hash>.plan`) and
/// the source pack it pins (`pkg/<name>-src/<hash>.tar`). Nothing else in
/// the cache is published: not build tools or other hashes. Refuses before
/// copying anything if a package is not built, has no licence or plan, or
/// needs a package no profile names.
pub fn publish_store(repo: &Path, dest: &Path) -> Result<()> {
    let mut named = BTreeSet::new();
    for (profile, _) in oath_core::PROFILES {
        let p = oath_core::Profile::load(profile).map_err(|e| anyhow::anyhow!("{e}"))?;
        named.extend(p.pkgs());
    }
    let lock = crate::pkgs::read_lock(repo)?;
    let all = crate::pkgs::load(repo)?;
    let cache = host_store_cache(repo);
    let mut rows: Vec<Published> = Vec::new();
    for name in &named {
        let (hash, license, plan_path, plan_hash) = if name == "oath" {
            let built = crate::oath_pkg::built(repo)?;
            let tree = cache.join("pkg/oath").join(&built.product);
            let license = crate::oath_pkg::license(repo, &tree)?;
            (built.product, license, repo.join("build/pkgs/oath.plan"), built.plan)
        } else {
            let e = lock.get(name).with_context(|| {
                format!("pkg:{name} is not in pkgs/lock.json: cargo make pkgs {name}")
            })?;
            let plan_path = crate::pkgs::pkgs_dir(repo).join(name).join(format!("{name}.plan"));
            (e.product.clone(), e.license.clone(), plan_path, e.plan.clone())
        };
        if license.trim().is_empty() {
            bail!("pkg:{name} has no licence on record; publish refuses it");
        }
        if !cache.join("pkg").join(name).join(&hash).is_dir() {
            bail!("pkg:{name} {hash} is not in {}", cache.display());
        }
        for need in all.get(name).map(|p| p.run.as_slice()).unwrap_or_default() {
            if !named.contains(need) {
                bail!("pkg:{name} needs pkg:{need}, which no seed profile names");
            }
        }
        let plan = fs::read(&plan_path).unwrap_or_default();
        if plan_hash.is_empty() || oath_core::file_hash(&plan) != plan_hash {
            bail!(
                "pkg:{name}: {} is not the plan that built {hash}; publish refuses a pack \
                 without its plan (cargo make pkgs {name})",
                plan_path.display()
            );
        }
        let parsed = oath_core::parse_plan(&String::from_utf8_lossy(&plan))
            .map_err(|e| anyhow::anyhow!("{}: {e}", plan_path.display()))?;
        let src_id = format!("pkg:{name}-src");
        let src = parsed
            .build_needs
            .iter()
            .find(|n| n.id == src_id)
            .map(|n| n.hash.clone())
            .with_context(|| format!("{}: no {src_id} in build_needs", plan_path.display()))?;
        if !cache.join("pkg").join(format!("{name}-src")).join(&src).is_dir() {
            bail!("{src_id} {src} is not in {}", cache.display());
        }
        rows.push(Published { name: name.clone(), hash, license, plan, plan_hash, src });
    }

    for r in &rows {
        let (name, hash, license) = (&r.name, &r.hash, &r.license);
        copy_pack_tar(&cache, dest, name, hash)?;
        println!("pkg:{name} {hash} {license}");
        copy_pack_tar(&cache, dest, &format!("{name}-src"), &r.src)?;
        println!("pkg:{name}-src {}", r.src);
        let out = dest.join("plan").join(name).join(format!("{}.plan", r.plan_hash));
        fs::create_dir_all(out.parent().context("parent")?)?;
        fs::write(&out, &r.plan).with_context(|| format!("write {}", out.display()))?;
        println!("plan:{name} {}", r.plan_hash);
        let adv = cache.join("adv").join(name).join(format!("{hash}.json"));
        if adv.is_file() {
            let out = dest.join("adv").join(name).join(format!("{hash}.json"));
            fs::create_dir_all(out.parent().context("parent")?)?;
            fs::copy(&adv, &out)
                .with_context(|| format!("copy {} -> {}", adv.display(), out.display()))?;
            println!("adv:{name} {hash}.json");
        }
    }
    let mut skipped: Vec<String> = fs::read_dir(cache.join("pkg"))?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !named.contains(n) && !rows.iter().any(|r| *n == format!("{}-src", r.name)))
        .collect();
    skipped.sort();
    if !skipped.is_empty() {
        println!("not named by a seed profile, not published: {}", skipped.join(" "));
    }

    let mut index = String::from(
        "# Oath pack origin\n\n\
         Not a package manager. Trees are `oath-tree-v1` (SHA-256). \
         Fetch `{origin}/pkg/<name>/<hash>.tar` and pin `desired.hash`. \
         Advisories are `{origin}/adv/<name>/<hash>.json` (not part of the pack hash). \
         Each pack was built by `{origin}/plan/<name>/<plan>.plan` from the source pack \
         `{origin}/pkg/<name>-src/<source>.tar` that the plan pins.\n\n\
         | name | hash | licence | plan | source |\n\
         |------|------|---------|------|--------|\n",
    );
    for r in &rows {
        index.push_str(&format!(
            "| `{}` | `{}` | {} | `{}` | `{}` |\n",
            r.name, r.hash, r.license, r.plan_hash, r.src
        ));
    }
    fs::write(dest.join("INDEX.md"), index)?;
    println!("{} ({} packs)", dest.display(), rows.len());
    Ok(())
}

/// One package on an origin, with the plan and source pack it was built from.
struct Published {
    name: String,
    hash: String,
    license: String,
    plan: Vec<u8>,
    plan_hash: String,
    src: String,
}

/// Copy `pkg/<name>/<hash>.tar` into the origin, writing the tar from the
/// cached tree first if it is not there yet.
fn copy_pack_tar(cache: &Path, dest: &Path, name: &str, hash: &str) -> Result<()> {
    let tree = cache.join("pkg").join(name).join(hash);
    let tar = cache.join("pkg").join(name).join(format!("{hash}.tar"));
    if !tar.is_file() {
        write_pack_tar(&tree, &tar)?;
    }
    let out = dest.join("pkg").join(name).join(format!("{hash}.tar"));
    fs::create_dir_all(out.parent().context("parent")?)?;
    fs::copy(&tar, &out).with_context(|| format!("copy {} -> {}", tar.display(), out.display()))?;
    Ok(())
}

fn host_mount() -> String {
    for p in ["/run/wrappers/bin/mount", "/usr/bin/mount"] {
        if Path::new(p).is_file() {
            return p.to_string();
        }
    }
    "mount".into()
}

fn host_umount() -> String {
    for p in ["/run/wrappers/bin/umount", "/usr/bin/umount"] {
        if Path::new(p).is_file() {
            return p.to_string();
        }
    }
    "umount".into()
}

fn nix_uid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    unsafe { getuid() }
}
fn nix_gid() -> u32 {
    extern "C" {
        fn getgid() -> u32;
    }
    unsafe { getgid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_dep_cuts_compressed_paths() {
        let dep = "kernel/a.ko.xz: kernel/b.ko.xz kernel/c.ko\nkernel/b.ko.xz:\n";
        assert_eq!(plain_dep(dep), "kernel/a.ko: kernel/b.ko kernel/c.ko\nkernel/b.ko:\n");
    }

    #[test]
    fn ko_path_strips_zst() {
        assert_eq!(ko_path("kernel/foo.ko.zst"), "kernel/foo.ko");
        assert_eq!(ko_path("kernel/foo.ko.xz"), "kernel/foo.ko");
        assert_eq!(ko_path("kernel/foo.ko"), "kernel/foo.ko");
    }
}
