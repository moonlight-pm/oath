use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;

use oath_core::{converge_with_link_root, promote_store, write_json};
use serde_json::json;

use crate::cpio;
use crate::output::{self, Output};
use crate::tools::Tools;
use crate::util::{chmod_exec, copy_file, copy_tree, run, run_out, sudo};

/// The boot initrd's one program besides PID 1 (static): the shell PID 1
/// falls back to.
pub(crate) const INITRD_TOOLS: &[(&str, &str)] = &[("bash", "bash")];

/// What the installer ramdisk runs, over SSH and on its console: pack and
/// file under its `bin/` (or a path in the pack: sshd's helpers are in
/// `libexec/`). Each comes with the libraries it loads.
pub(crate) const INSTALLER_TOOLS: &[(&str, &str)] = &[
    ("openssh", "sshd"),
    ("openssh", "libexec/sshd-session"),
    ("openssh", "libexec/sshd-auth"),
    ("openssh", "ssh-keygen"),
    ("btrfs", "btrfs"),
    ("btrfs", "mkfs.btrfs"),
    ("util-linux", "sfdisk"),
    ("util-linux", "wipefs"),
    ("util-linux", "blockdev"),
    ("util-linux", "mount"),
    ("util-linux", "umount"),
    ("util-linux", "dmesg"),
    ("dosfstools", "mkfs.fat"),
    ("iproute2", "ip"),
    ("dhcpcd", "dhcpcd"),
    ("tar", "tar"),
    ("gawk", "gawk"),
    ("coreutils", "cat"),
    ("coreutils", "chmod"),
    ("coreutils", "chown"),
    ("coreutils", "cp"),
    ("coreutils", "dd"),
    ("coreutils", "ls"),
    ("coreutils", "mkdir"),
    ("coreutils", "rm"),
    ("coreutils", "sleep"),
    ("coreutils", "sync"),
];

/// Puts programs into a ramdisk tree with what they load (R17): every
/// library they name, found in the packs' `lib/`, and glibc's loader, all
/// in its `/lib`.
pub(crate) struct Ramdisk<'a> {
    root: &'a Path,
    tools: &'a Tools,
    done: BTreeSet<String>,
    /// The packs whose files went in.
    pub packs: BTreeSet<String>,
}

impl<'a> Ramdisk<'a> {
    pub fn new(root: &'a Path, tools: &'a Tools) -> Self {
        Self { root, tools, done: BTreeSet::new(), packs: BTreeSet::new() }
    }

    /// `bin/<file>` of `pack` (or the pack's `<dir>/<file>`), as `bin/<file>`.
    pub fn program(&mut self, pack: &str, file: &str) -> Result<()> {
        let (src, name) = match file.rsplit_once('/') {
            Some((_, name)) => (self.tools.pack(pack)?.join(file), name),
            None => (self.tools.bin(pack, file)?, file),
        };
        if !src.is_file() {
            bail!("pkg:{pack} has no {file}");
        }
        let dest = self.root.join("bin").join(name);
        fs::create_dir_all(dest.parent().context("bin")?)?;
        let _ = fs::remove_file(&dest);
        copy_file(&src, &dest)?;
        chmod_exec(&dest)?;
        self.packs.insert(pack.to_string());
        self.libs_of(&dest)
    }

    fn libs_of(&mut self, file: &Path) -> Result<()> {
        let n = crate::elf::needs(&fs::read(file)?)
            .with_context(|| format!("read {}", file.display()))?;
        if let Some(i) = n.interp {
            if i != "/lib/ld-linux-x86-64.so.2" {
                bail!("{} names the loader {i}, not /lib's (R17)", file.display());
            }
            self.lib("ld-linux-x86-64.so.2")?;
        }
        for l in n.needed {
            self.lib(&l)?;
        }
        Ok(())
    }

    fn lib(&mut self, name: &str) -> Result<()> {
        if !self.done.insert(name.to_string()) {
            return Ok(());
        }
        let (pack, src) = self
            .tools
            .packs
            .iter()
            .map(|(p, tree)| (p, tree.join("lib").join(name)))
            .find(|(_, f)| f.exists())
            .with_context(|| format!("no pack has lib/{name}"))?;
        let dest = self.root.join("lib").join(name);
        fs::create_dir_all(dest.parent().context("lib")?)?;
        let _ = fs::remove_file(&dest);
        // The file itself, not the pack's soname link.
        fs::write(&dest, fs::read(&src)?)?;
        chmod_exec(&dest)?;
        self.packs.insert(pack.clone());
        self.libs_of(&dest)
    }
}

/// The packs a ramdisk with these programs carries, for the manifest.
pub(crate) fn ramdisk_packs(tools: &Tools, programs: &[(&str, &str)]) -> Result<BTreeSet<String>> {
    let scratch = tempfile::tempdir()?;
    let mut r = Ramdisk::new(scratch.path(), tools);
    for &(pack, file) in programs {
        r.program(pack, file)?;
    }
    Ok(r.packs)
}

/// Pack the image for one seed profile (`base`, `dev`) into a build output
/// (`build/out/<id>/`, see output.rs), or reuse the output of an unchanged
/// tree. The profile's present packages are packed; anything else it
/// names stays absent. `out` is the build directory.
pub fn build(root: &Path, out: &Path, tools: &Tools, profile: &str) -> Result<Output> {
    tools.require_qemu()?;
    let p = oath_core::Profile::load(profile).map_err(|e| anyhow::anyhow!("{e}"))?;
    let wanted = p.present_pkgs();
    let lock = crate::pkgs::read_lock(root)?;
    let all = crate::pkgs::load(root)?;
    // Packs built elsewhere (R28): the ones the image ships are fetched
    // into the host store.
    let pins = crate::pinned::of_profile(root, profile)?;
    let shipped_pins = pins.clone().into_iter().filter(|(n, _)| wanted.contains(n)).collect();
    let pinned = crate::pinned::fetch(&host_store_cache(root), &shipped_pins, &p.origins())?;
    let mut run_needs = BTreeSet::new();
    for (name, pin) in &pins {
        for (need, _) in &pin.needs {
            if wanted.contains(name) && !wanted.contains(need) {
                bail!("profile {profile}: pkg:{name} runs with pkg:{need}, which it does not ship");
            }
        }
    }
    for name in wanted.iter().filter(|n| !pins.contains_key(*n)) {
        if crate::pkgs::is_recipe(&all, name) {
            bail!(
                "profile {profile} wants pkg:{name} present, but it is a recipe (R23): no image \
                 carries it; name it with present false and the machine builds it"
            );
        }
        if name != "oath" && !tools.packs.contains_key(name) {
            bail!("profile {profile} wants pkg:{name}, which this build cannot pack yet");
        }
        for need in crate::pkgs::locked_run(root, &all, &lock, name) {
            if !wanted.contains(&need) {
                bail!("profile {profile}: pkg:{name} runs with pkg:{need}, which it does not ship");
            }
            run_needs.insert((name.clone(), need));
        }
    }
    // The Oath binaries: pkg:oath, this tree built by pkg:rust.
    let oath = crate::oath_pkg::build(root)?;
    let manifest = crate::manifest::make(root, tools, profile, &pins)?;
    crate::pinned::warn_stale(&manifest);
    let id = output::id(&manifest);
    if let Some(o) = output::reuse(out, &id, Some(&manifest))? {
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
    let mut ramdisk = Ramdisk::new(&ir_install, tools);
    for &(pack, file) in INSTALLER_TOOLS {
        ramdisk.program(pack, file)?;
    }
    symlink("gawk", ir_install.join("bin/awk"))?;
    // `reboot -f` is PID 1's own binary.
    symlink("../init", ir_install.join("bin/reboot"))?;
    // The installer writes the new catalog with `oath seed --overlay`.
    copy_file(&oath.join("bin/oath"), &ir_install.join("bin/oath"))?;
    chmod_exec(&ir_install.join("bin/oath"))?;
    fs::create_dir_all(ir_install.join("opt/oath-install"))?;
    copy_file(&dir.join("initrd.gz"), &ir_install.join("opt/oath-install/initrd.gz"))?;
    copy_file(&tools.kernel, &ir_install.join("opt/oath-install/vmlinuz"))?;
    // oath-efi is the boot loader (menu, BLS entries, kernel + initrd).
    copy_file(&efi(&oath), &ir_install.join("opt/oath-install/BOOTX64.EFI"))?;
    fs::create_dir_all(ir_install.join("lib/oath"))?;
    fs::write(ir_install.join(&oath_core::DHCP_HOOK[1..]), include_str!("dhcpcd.hook"))?;
    chmod_exec(&ir_install.join(&oath_core::DHCP_HOOK[1..]))?;
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
    for n in ["init", "serial-login", "screen-login", "sudo", "login"] {
        copy_file(&lib.join(n), &stage.join("lib/oath").join(n))?;
    }
    fs::write(stage.join(&oath_core::DHCP_HOOK[1..]), include_str!("dhcpcd.hook"))?;
    fs::write(stage.join("lib/oath/backup-send"), include_str!("backup-send"))?;
    fs::write(stage.join("lib/oath/backup-daily"), include_str!("backup-daily"))?;
    chmod_exec(&stage.join("lib/oath/init"))?;
    chmod_exec(&stage.join("lib/oath/serial-login"))?;
    chmod_exec(&stage.join("lib/oath/screen-login"))?;
    chmod_exec(&stage.join("lib/oath/login"))?;
    chmod_exec(&stage.join(&oath_core::DHCP_HOOK[1..]))?;
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
    // R18: modprobe, reboot, halt and poweroff are PID 1's binary.
    for cmd in ["modprobe", "reboot", "halt", "poweroff"] {
        let _ = fs::remove_file(stage.join("bin").join(cmd));
        symlink("../lib/oath/init", stage.join("bin").join(cmd))?;
    }
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
    fs::create_dir_all(stage.join("var/lib/dhcpcd"))?;
    // void's home (R36): empty, the chroot of sshd's and dhcpcd's
    // unprivileged processes.
    fs::create_dir_all(stage.join(&oath_core::seat::VOID_HOME[1..]))?;
    fs::set_permissions(
        stage.join(&oath_core::seat::VOID_HOME[1..]),
        fs::Permissions::from_mode(0o755),
    )?;
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
        let tree = match pinned.get(name) {
            _ if name == "oath" => oath.as_path(),
            Some(t) => t.as_path(),
            None => tools.pack(name)?,
        };
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
    pin_run_needs(&oath_root, &run_needs)?;
    oath_core::sync_need_hashes(&oath_root).map_err(|e| anyhow::anyhow!("{e}"))?;
    let named = oath_core::Profile::load(profile).map_err(|e| anyhow::anyhow!("{e}"))?.pkgs();
    file_recipes(root, &oath_root, &all, &lock, &named)?;
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
    // A `mount` early on PATH may not understand `subvol=@`. Use the
    // host's util-linux wrappers.
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
    let mut ramdisk = Ramdisk::new(&ir, tools);
    for &(pack, file) in INITRD_TOOLS {
        ramdisk.program(pack, file)?;
    }
    symlink("bash", ir.join("bin/sh"))?;

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

/// Each pack's runtime needs (the libraries it loads, from the lock) as
/// its `needs`, so apply refuses removing one while a present pack loads
/// it. `sync_need_hashes` pins them.
fn pin_run_needs(oath_root: &Path, needs: &BTreeSet<(String, String)>) -> Result<()> {
    for (name, need) in needs {
        let p = oath_root.join("objects/pkg").join(name).join("desired.json");
        let mut v: serde_json::Value =
            oath_core::read_json(&p).map_err(|e| anyhow::anyhow!("{}: {e}", p.display()))?;
        let list = v
            .as_object_mut()
            .with_context(|| format!("{}: not an object", p.display()))?
            .entry("needs")
            .or_insert_with(|| serde_json::json!([]));
        let id = format!("pkg:{need}");
        let arr = list.as_array_mut().with_context(|| format!("{}: needs", p.display()))?;
        if !arr.iter().any(|n| n.get("id").and_then(|i| i.as_str()) == Some(id.as_str())) {
            arr.push(serde_json::json!(id));
        }
        write_json(&p, &v).map_err(|e| anyhow::anyhow!("{e}"))?;
    }
    Ok(())
}

/// R23: a recipe the profile names goes in as its plan, not its pack. The
/// plan is filed in the image's store and pinned (`plan:<name>`);
/// `pkg:<name>-src` gets the vendor's URL, the file's place and its pin,
/// and `pkg:<name>` the product's pin and needs, both absent until the
/// machine runs `oath build plan:<name>`. The plan runs with packs the
/// image ships, at the hashes it ships them, and builds with those or
/// with locked packs, which a machine fetches from its origins (R35).
fn file_recipes(
    root: &Path,
    oath_root: &Path,
    all: &BTreeMap<String, crate::pkgs::PkgToml>,
    lock: &crate::pkgs::Lock,
    named: &[String],
) -> Result<()> {
    for name in named.iter().filter(|n| crate::pkgs::is_recipe(all, n)) {
        let e = lock
            .get(name)
            .with_context(|| format!("pkg:{name} is not built yet (cargo make pkgs {name})"))?;
        let plan_path = crate::pkgs::pkgs_dir(root).join(name).join(format!("{name}.plan"));
        let text = fs::read(&plan_path).with_context(|| plan_path.display().to_string())?;
        if oath_core::file_hash(&text) != e.plan {
            bail!(
                "pkg:{name}: {} is not the plan that built {} (cargo make pkgs {name})",
                plan_path.display(),
                e.product
            );
        }
        let plan = oath_core::parse_plan(&String::from_utf8_lossy(&text))
            .map_err(|err| anyhow::anyhow!("{}: {err}", plan_path.display()))?;
        let src_id = format!("pkg:{name}-src");
        let mut src = None;
        let build: Vec<&str> = plan.build_needs.iter().map(|n| n.id.as_str()).collect();
        for n in plan.build_needs.iter().chain(&plan.run_needs) {
            if n.id == src_id {
                src = Some(n.hash.clone());
                continue;
            }
            let pkg = n.id.strip_prefix("pkg:").unwrap_or(&n.id);
            let have = pkg_pin(oath_root, pkg);
            let fetched = have.is_none()
                && build.contains(&n.id.as_str())
                && lock.get(pkg).is_some_and(|l| l.product == n.hash);
            if have.as_deref() != Some(n.hash.as_str()) && !fetched {
                bail!(
                    "pkg:{name}: its plan needs {} at {}, and the image has {}: \
                     cargo make pkgs {name}",
                    n.id,
                    n.hash,
                    have.unwrap_or_else(|| "none".into())
                );
            }
        }
        let src = src.with_context(|| format!("{}: no {src_id}", plan_path.display()))?;
        if !oath_root.join("objects/plan").join(name).is_dir() {
            bail!("pkg:{name} is a recipe: the profile names it, so it names plan:{name} too");
        }
        oath_core::file_plan(oath_root, &text).map_err(|err| anyhow::anyhow!("{err}"))?;
        let s = &all[name].source[0];
        patch_pkg(
            oath_root,
            &format!("{name}-src"),
            json!({ "url": s.url, "file": format!("src/{}", s.file()), "hash": src }),
        )?;
        let needs: Vec<_> =
            plan.run_needs.iter().map(|n| json!({ "id": n.id, "hash": n.hash })).collect();
        patch_pkg(oath_root, name, json!({ "hash": e.product, "needs": needs }))?;
        eprintln!("   recipe pkg:{name} {} (plan {}, built on the machine)", e.product, e.plan);
    }
    oath_core::Catalog::open(oath_root)
        .and_then(|c| c.write_index())
        .map_err(|err| anyhow::anyhow!("{err}"))
}

/// The pin the image's catalog gives `pkg:<name>`, if it names it.
fn pkg_pin(oath_root: &Path, name: &str) -> Option<String> {
    let v: serde_json::Value =
        oath_core::read_json(&oath_root.join("objects/pkg").join(name).join("desired.json"))
            .ok()?;
    v.get("hash").and_then(|h| h.as_str()).filter(|h| !h.is_empty()).map(str::to_string)
}

/// Merge `fields` into a seeded, absent `pkg:<name>`'s desired doc.
fn patch_pkg(oath_root: &Path, name: &str, fields: serde_json::Value) -> Result<()> {
    let p = oath_root.join("objects/pkg").join(name).join("desired.json");
    let mut v: serde_json::Value = oath_core::read_json(&p)
        .map_err(|err| anyhow::anyhow!("pkg:{name} (a recipe's; the profile names it): {err}"))?;
    if v.get("present") != Some(&json!(false)) {
        bail!("pkg:{name} belongs to a recipe (R23), so the profile names it with present false");
    }
    for (k, val) in fields.as_object().context("fields")? {
        v[k] = val.clone();
    }
    write_json(&p, &v).map_err(|err| anyhow::anyhow!("{err}"))
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

/// The host store: `$OATH_STORE`, else `$XDG_DATA_HOME/oath/store`
/// (`~/.local/share/oath/store`). It lives outside the checkout, so a
/// `git clean`, a fresh clone or a worktree shares it; its parent is the
/// build catalog.
pub fn host_store_cache(repo: &Path) -> PathBuf {
    if let Some(p) = std::env::var_os("OATH_STORE").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    let data = std::env::var_os("XDG_DATA_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    match data {
        Some(d) => d.join("oath/store"),
        None => repo.join(".cache/oath/store"),
    }
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

/// `cargo make publish`: every package a seed profile names and every
/// pack `pkgs/lock.json` records (build packs too, R25), at the hash this
/// tree records for it (the lock, or pkg:oath's last build), each with a
/// licence on record, and beside it what it was built from (R13): the
/// plan (`plan/<name>/<hash>.plan`) and the source pack it pins
/// (`pkg/<name>-src/<hash>.tar`); then advisories (`adv/<name>/<hash>.json`)
/// and `INDEX.md`. Every pack a published plan builds with goes out at
/// the hash it pins, or is on the origin at that hash already (an earlier
/// publish put it there with its own plan: a pack its build inputs moved
/// past, CEF's, waits for its next build), so an origin alone rebuilds
/// any of them. Nothing else in the cache does: not test packs or other
/// hashes. Refuses before writing anything if a package is not built, has
/// no licence or plan, or needs a package that is not published. With
/// `dest`, writes that directory (an origin is any such tree); without,
/// uploads to the default origin what it does not have yet.
pub fn publish_store(
    repo: &Path,
    out: &Path,
    dest: Option<&Path>,
    output: Option<&str>,
) -> Result<()> {
    // R38: the newest checked output goes out after its packs; R47: to
    // the origin only from a clean master that is origin's, before any
    // upload.
    let picked = crate::output::pick(out, output, true);
    if dest.is_none() {
        crate::release::require_master(repo, picked.as_ref().ok())?;
    }
    let origin = match dest {
        Some(_) => None,
        None => Some(crate::origin::Origin::from_env()?),
    };
    let have = match &origin {
        Some(o) => o.keys()?,
        None => BTreeSet::new(),
    };
    let there = |key: &str| have.contains(key) || dest.is_some_and(|d| d.join(key).is_file());
    let p = collect_publication(repo, there)?;
    match dest {
        Some(d) => write_publication(repo, &p, d)?,
        None => upload_publication(repo, &p)?,
    }
    if !p.skipped.is_empty() {
        println!("not locked or named by a seed profile, not published: {}", p.skipped.join(" "));
    }
    match picked {
        Ok(o) => crate::release::publish(&o, dest, &out.join("release").join(&o.id)),
        Err(e) if output.is_none() => {
            println!("no output published: {e:#}");
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// What a publish puts on an origin: each file under its key, in order
/// (`INDEX.md` last, so it never lists what is not there yet).
struct Publication {
    files: Vec<(String, Body)>,
    packs: usize,
    skipped: Vec<String>,
}

enum Body {
    /// A pack tree in the cache, as a tar.
    Tar {
        name: String,
        hash: String,
    },
    File(PathBuf),
    Bytes(Vec<u8>),
}

fn collect_publication(repo: &Path, there: impl Fn(&str) -> bool) -> Result<Publication> {
    let mut named = BTreeSet::new();
    for (profile, _) in oath_core::PROFILES {
        let p = oath_core::Profile::load(profile).map_err(|e| anyhow::anyhow!("{e}"))?;
        named.extend(p.pkgs());
    }
    let lock = crate::pkgs::read_lock(repo)?;
    named.extend(lock.keys().cloned());
    // A pack pinned from elsewhere (R28) is its owner's to publish.
    let pinned = crate::pinned::all(repo)?;
    named.retain(|n| !pinned.contains(n));
    let all = crate::pkgs::load(repo)?;
    // A recipe (R23) is published as its plan alone: no pack, no source.
    let recipes: BTreeSet<String> =
        named.iter().filter(|n| crate::pkgs::is_recipe(&all, n)).cloned().collect();
    named.retain(|n| {
        !recipes.contains(n) && !n.strip_suffix("-src").is_some_and(|r| recipes.contains(r))
    });
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
                bail!("pkg:{name} needs pkg:{need}, which is not published");
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
        for n in parsed.build_needs.iter().filter(|n| n.id != src_id) {
            let need = n.id.strip_prefix("pkg:").unwrap_or(&n.id);
            let published = if need == "oath" { None } else { lock.get(need) };
            let moved = published.is_some_and(|e| e.product != n.hash);
            if !named.contains(need) || (moved && !there(&format!("pkg/{need}/{}.tar", n.hash))) {
                bail!(
                    "pkg:{name}'s plan builds with {} {}, which is not published at that hash \
                     (cargo make pkgs {name})",
                    n.id,
                    n.hash
                );
            }
        }
        rows.push(Published { name: name.clone(), hash, license, plan, plan_hash, src });
    }

    let mut files = Vec::new();
    for r in &rows {
        let name = &r.name;
        files.push((
            format!("pkg/{name}/{}.tar", r.hash),
            Body::Tar { name: name.clone(), hash: r.hash.clone() },
        ));
        files.push((
            format!("pkg/{name}-src/{}.tar", r.src),
            Body::Tar { name: format!("{name}-src"), hash: r.src.clone() },
        ));
        files.push((format!("plan/{name}/{}.plan", r.plan_hash), Body::Bytes(r.plan.clone())));
        let adv = cache.join("adv").join(name).join(format!("{}.json", r.hash));
        if adv.is_file() {
            files.push((format!("adv/{name}/{}.json", r.hash), Body::File(adv)));
        }
    }
    let mut plans = Vec::new();
    for name in &recipes {
        let e = lock.get(name).with_context(|| format!("pkg:{name}: cargo make pkgs {name}"))?;
        let plan_path = crate::pkgs::pkgs_dir(repo).join(name).join(format!("{name}.plan"));
        let plan = fs::read(&plan_path).with_context(|| plan_path.display().to_string())?;
        if oath_core::file_hash(&plan) != e.plan {
            bail!("pkg:{name}: {} is not its locked plan", plan_path.display());
        }
        files.push((format!("plan/{name}/{}.plan", e.plan), Body::Bytes(plan)));
        plans.push((name.clone(), e.product.clone(), e.license.clone(), e.plan.clone()));
    }
    let mut skipped: Vec<String> = fs::read_dir(cache.join("pkg"))?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .filter(|n| !named.contains(n) && !rows.iter().any(|r| *n == format!("{}-src", r.name)))
        .filter(|n| !pinned.contains(n))
        .filter(|n| !recipes.contains(n.strip_suffix("-src").unwrap_or(n)))
        .collect();
    skipped.sort();

    let mut index = String::from(
        "# Oath pack origin\n\n\
         Not a package manager. Trees are `oath-tree-v1` (SHA-256). \
         Fetch `{origin}/pkg/<name>/<hash>.tar` and pin `desired.hash`; any origin may serve \
         any pack, and the hash decides. \
         Advisories are `{origin}/adv/<name>/<hash>.json` (not part of the pack hash). \
         Each pack was built by `{origin}/plan/<name>/<plan>.plan` from the source pack \
         `{origin}/pkg/<name>-src/<source>.tar` that the plan pins; every pack a plan builds \
         with is here too.\n\n\
         | name | hash | licence | plan | source |\n\
         |------|------|---------|------|--------|\n",
    );
    for r in &rows {
        index.push_str(&format!(
            "| `{}` | `{}` | {} | `{}` | `{}` |\n",
            r.name, r.hash, r.license, r.plan_hash, r.src
        ));
    }
    if !plans.is_empty() {
        index.push_str(
            "\nRecipes (R23): their licences allow no redistribution, so only the plan is \
             here. A machine fetches the vendor's file itself and `oath build`s the pack.\n\n\
             | name | builds | licence | plan |\n\
             |------|--------|---------|------|\n",
        );
        for (name, product, license, plan) in &plans {
            index.push_str(&format!("| `{name}` | `{product}` | {license} | `{plan}` |\n"));
        }
    }
    files.push(("INDEX.md".into(), Body::Bytes(index.into_bytes())));
    Ok(Publication { files, packs: rows.len(), skipped })
}

/// Write the publication into `dest`, an origin tree on disk.
fn write_publication(repo: &Path, p: &Publication, dest: &Path) -> Result<()> {
    let cache = host_store_cache(repo);
    for (key, body) in &p.files {
        let out = dest.join(key);
        fs::create_dir_all(out.parent().context("parent")?)?;
        match body {
            Body::Tar { name, hash } => {
                let tar = pack_tar(&cache, name, hash)?;
                fs::copy(&tar, &out)
            }
            Body::File(f) => fs::copy(f, &out),
            Body::Bytes(b) => fs::write(&out, b).map(|_| 0),
        }
        .with_context(|| format!("write {}", out.display()))?;
        println!("{key}");
    }
    println!("{} ({} packs)", dest.display(), p.packs);
    Ok(())
}

/// Upload to the default origin every file it lacks (`INDEX.md` and
/// advisories every time), then read `INDEX.md` back from where machines
/// fetch it: the first of the `base` profile's `host:local.origins`.
fn upload_publication(repo: &Path, p: &Publication) -> Result<()> {
    let origin = crate::origin::Origin::from_env()?;
    let cache = host_store_cache(repo);
    let tmp = cache.parent().context("store cache has no parent")?.join("publish");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp)?;
    let have = origin.keys()?;
    let (mut put, mut kept, mut bytes) = (0usize, 0usize, 0u64);
    for (key, body) in &p.files {
        if crate::origin::is_immutable(key) && have.contains(key) {
            kept += 1;
            continue;
        }
        // A tar the cache does not keep is written for the upload and removed.
        let (file, temp) = match body {
            Body::Tar { name, hash } => {
                let cached = cache.join("pkg").join(name).join(format!("{hash}.tar"));
                if cached.is_file() {
                    (cached, false)
                } else {
                    let t = tmp.join(format!("{name}-{hash}.tar"));
                    write_pack_tar(&cache.join("pkg").join(name).join(hash), &t)?;
                    (t, true)
                }
            }
            Body::File(f) => (f.clone(), false),
            Body::Bytes(b) => {
                let t = tmp.join(key.replace('/', "_"));
                fs::write(&t, b)?;
                (t, true)
            }
        };
        let size = fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        origin.put(key, &file)?;
        if temp {
            let _ = fs::remove_file(&file);
        }
        put += 1;
        bytes += size;
        println!("put {key} ({:.1} MB)", size as f64 / 1e6);
    }
    let _ = fs::remove_dir_all(&tmp);
    println!(
        "{put} files put ({:.2} GB), {kept} already there ({} packs)",
        bytes as f64 / 1e9,
        p.packs
    );
    let base = oath_core::Profile::load("base").map_err(|e| anyhow::anyhow!("{e}"))?;
    let public = base
        .origins()
        .first()
        .context("the base profile names no origin (host:local.origins)")?
        .trim_end_matches('/')
        .to_string();
    let Some((_, Body::Bytes(index))) = p.files.last() else { bail!("no INDEX.md") };
    let url = format!("{public}/INDEX.md?v={}", oath_core::file_hash(index));
    let got = run_out(Command::new("curl").args(["-fsSL", &url]))?;
    if got.trim() != String::from_utf8_lossy(index).trim() {
        bail!("{url} does not serve the INDEX.md just put");
    }
    println!("{public}/INDEX.md serves it");
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

/// `pkg/<name>/<hash>.tar` beside the cached tree, written first if it
/// is not there yet.
fn pack_tar(cache: &Path, name: &str, hash: &str) -> Result<PathBuf> {
    let tar = cache.join("pkg").join(name).join(format!("{hash}.tar"));
    if !tar.is_file() {
        write_pack_tar(&cache.join("pkg").join(name).join(hash), &tar)?;
    }
    Ok(tar)
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
