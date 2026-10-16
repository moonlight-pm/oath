use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;

use oath_core::{converge_with_link_root, promote_store, write_json};

use crate::cpio;
use crate::tools::Tools;
use crate::util::{chmod_exec, copy_file, copy_tree, run, sudo};

/// Kernel + initrd only. Used by `cargo make esp` on metal (no qcow, no
/// qemu). Returns pkg:oath's tree, which holds `oath-efi`.
pub fn boot_image(root: &Path, out: &Path, tools: &Tools) -> Result<PathBuf> {
    fs::create_dir_all(out)?;
    eprintln!("kernel={}", tools.kernel.display());
    eprintln!("modules={}", tools.modules.display());
    let oath = crate::oath_pkg::build(root)?;
    write_initrd(root, out, tools, &oath.join("lib/oath/init"))?;
    Ok(oath)
}

/// Pack the image for one seed profile (`base`, `dev`). The profile's
/// present packages are packed; anything else it names stays absent.
pub fn build(root: &Path, out: &Path, tools: &Tools, profile: &str) -> Result<()> {
    let wanted =
        oath_core::Profile::load(profile).map_err(|e| anyhow::anyhow!("{e}"))?.present_pkgs();
    for name in &wanted {
        if name != "oath" && !tools.packs.contains_key(name) {
            bail!("profile {profile} wants pkg:{name}, which this build cannot pack yet");
        }
    }
    fs::create_dir_all(out)?;
    eprintln!("profile={profile}");
    eprintln!("kernel={}", tools.kernel.display());
    eprintln!("modules={}", tools.modules.display());
    eprintln!("packs={}", tools.packs.keys().cloned().collect::<Vec<_>>().join(" "));

    // The Oath binaries: pkg:oath, this tree built by pkg:rust.
    let oath = crate::oath_pkg::build(root)?;
    let lib = oath.join("lib/oath");
    write_initrd(root, out, tools, &lib.join("init"))?;

    eprintln!(">> installer initramfs");
    let ir_install = out.join("initramfs-install");
    let _ = fs::remove_dir_all(&ir_install);
    copy_tree(&out.join("initramfs"), &ir_install)?;
    // Native tools the installer runs over SSH (static binaries).
    for (pack, file) in [
        ("dropbear", "dropbear"),
        ("dropbear", "dropbearkey"),
        ("btrfs", "btrfs"),
        ("btrfs", "mkfs.btrfs"),
        ("util-linux", "sfdisk"),
        ("util-linux", "wipefs"),
        ("dosfstools", "mkfs.fat"),
    ] {
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
    copy_file(&out.join("initrd.gz"), &ir_install.join("opt/oath-install/initrd.gz"))?;
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
    let initrd_install = write_cpio_gz(&ir_install, &out.join("initrd-install.gz"))?;
    eprintln!("initrd-install {}", initrd_install.display());

    eprintln!(">> stage rootfs");
    let stage = out.join("stage");
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
    let fixed = oath_core::Profile::load(profile).map_err(|e| anyhow::anyhow!("{e}"))?;
    for name in &wanted {
        let tree = if name == "oath" { oath.as_path() } else { tools.pack(name)? };
        copy_tree(tree, &oath_root.join("store/pkg").join(name))?;
        link_pkg(&oath_root, &guest_bin, name, !fixed.is_fixed(name))?;
    }

    let cache = host_store_cache(root);
    eprintln!(">> hash-in-path store ({})", cache.display());
    let pins = promote_store(&oath_root, Some(&cache)).map_err(|e| anyhow::anyhow!("{e}"))?;
    for (n, h) in &pins {
        eprintln!("   pkg:{n} {h}");
    }
    relink_present(&oath_root, &guest_bin)?;
    oath_core::sync_need_hashes(&oath_root).map_err(|e| anyhow::anyhow!("{e}"))?;
    settle_pkg_actuals(&oath_root)?;

    eprintln!(">> rootfs (btrfs subvol @) — loop-mount needs root");
    let raw = out.join("root.raw");
    let qcow = out.join("oath.qcow2");
    let _ = fs::remove_file(&raw);
    let _ = fs::remove_file(&qcow);
    run(Command::new(&tools.qemu_img).args(["create", "-f", "raw", raw.to_str().unwrap(), "4G"]))?;
    run(Command::new("mkfs.btrfs").args(["-q", "-L", "oath", raw.to_str().unwrap()]))?;
    let mnt = out.join("mnt");
    let rootfs = out.join("rootfs");
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
    let _ = sudo(&["chown", "-R", &format!("{uid}:{gid}"), out.to_str().unwrap()]);
    let bz = out.join("bzImage");
    let _ = fs::remove_file(&bz);
    copy_file(&tools.kernel, &bz)?;
    eprintln!("image {}", qcow.display());
    eprintln!("kernel {}", bz.display());
    eprintln!("next: cargo make run");
    Ok(())
}

/// `oath-efi` inside pkg:oath's tree.
pub(crate) fn efi(oath: &Path) -> PathBuf {
    oath.join("boot/BOOTX64.EFI")
}

fn write_initrd(root: &Path, out: &Path, tools: &Tools, init: &Path) -> Result<()> {
    let _ = root;
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

pub fn bake_install_keys(out: &Path, keys: &str) -> Result<PathBuf> {
    let tree = out.join("initramfs-install");
    if !tree.join("init").is_file() {
        bail!("missing installer initramfs (run cargo make build)");
    }
    let baked = out.join("initramfs-install-keys");
    let _ = fs::remove_dir_all(&baked);
    copy_tree(&tree, &baked)?;
    fs::create_dir_all(baked.join("root/.ssh"))?;
    fs::set_permissions(baked.join("root/.ssh"), fs::Permissions::from_mode(0o700))?;
    fs::write(baked.join("root/.ssh/authorized_keys"), keys)?;
    fs::set_permissions(
        baked.join("root/.ssh/authorized_keys"),
        fs::Permissions::from_mode(0o600),
    )?;
    write_cpio_gz(&baked, &out.join("initrd-install.gz"))
}

/// Transitive closure + topological load order from `modules.dep`.
/// Output paths have `.xz` stripped (pack stores uncompressed `.ko`).

fn link_pkg(oath_root: &Path, bin_dir: &Path, name: &str, removable: bool) -> Result<()> {
    let hash = desired_hash(oath_root, name);
    let mut actual =
        converge_with_link_root(oath_root, bin_dir, Path::new("/oath"), name, true, &hash)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    actual.removable = removable;
    write_json(&oath_root.join("objects/pkg").join(name).join("actual.json"), &actual)
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
        let removable = oath_root
            .join("objects/pkg")
            .join(&name)
            .join("actual.json")
            .exists()
            .then(|| {
                oath_core::read_json::<serde_json::Value>(
                    &oath_root.join("objects/pkg").join(&name).join("actual.json"),
                )
                .ok()
                .and_then(|a| a.get("removable").and_then(|x| x.as_bool()))
                .unwrap_or(true)
            })
            .unwrap_or(true);
        link_pkg(oath_root, bin_dir, &name, removable)?;
    }
    Ok(())
}

/// Give every `pkg` actual the prose and pin its desired doc has, the way
/// apply would, so a fresh image starts in sync instead of drifting.
fn settle_pkg_actuals(oath_root: &Path) -> Result<()> {
    let dir = oath_root.join("objects/pkg");
    for e in fs::read_dir(&dir)?.flatten() {
        let d = e.path();
        let desired: serde_json::Value = oath_core::read_json(&d.join("desired.json"))
            .map_err(|e| anyhow::anyhow!("{}: {e}", d.display()))?;
        let mut actual: serde_json::Value = oath_core::read_json(&d.join("actual.json"))
            .map_err(|e| anyhow::anyhow!("{}: {e}", d.display()))?;
        for k in ["url", "needs", "description", "home"] {
            if let Some(v) = desired.get(k) {
                actual[k] = v.clone();
            }
        }
        let pin = desired.get("hash").and_then(|h| h.as_str()).unwrap_or("");
        if !pin.is_empty() && actual.get("hash").and_then(|h| h.as_str()).unwrap_or("").is_empty() {
            actual["hash"] = serde_json::json!(pin);
        }
        write_json(&d.join("actual.json"), &actual).map_err(|e| anyhow::anyhow!("{e}"))?;
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
    let tree = cache.join("pkg").join(name).join(&hash);
    if !tree.is_dir() {
        oath_core::copy_pack_tree(from, &tree).map_err(|e| anyhow::anyhow!("{e}"))?;
    }
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

/// Copy hashed pack tarballs into an origin tree (`pkg/<name>/<hash>.tar`).
pub fn publish_store(repo: &Path, dest: &Path) -> Result<()> {
    let cache = host_store_cache(repo).join("pkg");
    if !cache.is_dir() {
        bail!(
            "no host store cache at {} — cargo make store --name <n> --from <dir> --tar",
            cache.display()
        );
    }
    let dest_pkg = dest.join("pkg");
    fs::create_dir_all(&dest_pkg)?;
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut names: Vec<String> = fs::read_dir(&cache)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        let slot = cache.join(&name);
        let mut hashes: Vec<String> = fs::read_dir(&slot)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| oath_core::is_realization_id(n))
            .collect();
        hashes.sort();
        for hash in hashes {
            let tree = slot.join(&hash);
            let tar = slot.join(format!("{hash}.tar"));
            if !tar.is_file() {
                write_pack_tar(&tree, &tar)?;
            }
            let out_dir = dest_pkg.join(&name);
            fs::create_dir_all(&out_dir)?;
            let out = out_dir.join(format!("{hash}.tar"));
            fs::copy(&tar, &out)
                .with_context(|| format!("copy {} -> {}", tar.display(), out.display()))?;
            println!("pkg:{name} {hash}");
            rows.push((name.clone(), hash));
        }
    }
    let cache_adv = host_store_cache(repo).join("adv");
    if cache_adv.is_dir() {
        let dest_adv = dest.join("adv");
        fs::create_dir_all(&dest_adv)?;
        let mut names: Vec<String> = fs::read_dir(&cache_adv)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names {
            let slot = cache_adv.join(&name);
            let out_dir = dest_adv.join(&name);
            fs::create_dir_all(&out_dir)?;
            let mut files: Vec<String> = fs::read_dir(&slot)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".json"))
                .collect();
            files.sort();
            for file in files {
                fs::copy(slot.join(&file), out_dir.join(&file)).with_context(|| {
                    format!(
                        "copy {} -> {}",
                        slot.join(&file).display(),
                        out_dir.join(&file).display()
                    )
                })?;
                println!("adv:{name} {file}");
            }
        }
    }
    let mut index = String::from(
        "# Oath pack origin\n\n\
         Not a package manager. Trees are `oath-tree-v1` (SHA-256). \
         Fetch `{origin}/pkg/<name>/<hash>.tar` and pin `desired.hash`. \
         Advisories are `{origin}/adv/<name>/<hash>.json` (not part of the pack hash).\n\n\
         | name | hash |\n\
         |------|------|\n",
    );
    for (name, hash) in &rows {
        index.push_str(&format!("| `{name}` | `{hash}` |\n"));
    }
    fs::write(dest.join("INDEX.md"), index)?;
    println!("{} ({} packs)", dest.display(), rows.len());
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
