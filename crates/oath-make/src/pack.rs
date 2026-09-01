use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;

use oath_core::{converge_with_link_root, write_json};

use crate::cpio;
use crate::tools::Tools;
use crate::util::{chmod_exec, copy_file, copy_tree, out_dir, run, run_out, sudo};

/// Drivers we need. Transitive deps (led-class, ptp, nvme-auth, af_packet, …)
/// are pulled from `modules.dep` at pack time so insmod order is valid.
const MODULE_ROOTS: &[&str] = &[
    "kernel/drivers/virtio/virtio.ko.xz",
    "kernel/drivers/virtio/virtio_ring.ko.xz",
    "kernel/drivers/virtio/virtio_pci_legacy_dev.ko.xz",
    "kernel/drivers/virtio/virtio_pci_modern_dev.ko.xz",
    "kernel/drivers/virtio/virtio_pci.ko.xz",
    "kernel/drivers/block/virtio_blk.ko.xz",
    "kernel/drivers/virtio/virtio_dma_buf.ko.xz",
    "kernel/drivers/gpu/drm/virtio/virtio-gpu.ko.xz",
    "kernel/drivers/input/evdev.ko.xz",
    "kernel/drivers/virtio/virtio_input.ko.xz",
    "kernel/net/core/failover.ko.xz",
    "kernel/drivers/net/net_failover.ko.xz",
    "kernel/drivers/net/virtio_net.ko.xz",
    "kernel/net/packet/af_packet.ko.xz",
    "kernel/drivers/char/hw_random/rng-core.ko.xz",
    "kernel/drivers/char/hw_random/virtio-rng.ko.xz",
    "kernel/drivers/firmware/qemu_fw_cfg.ko.xz",
    "kernel/crypto/crc32c_generic.ko.xz",
    "kernel/lib/libcrc32c.ko.xz",
    "kernel/crypto/xor.ko.xz",
    "kernel/lib/raid6/raid6_pq.ko.xz",
    "kernel/fs/btrfs/btrfs.ko.xz",
    "kernel/fs/fat/vfat.ko.xz",
    "kernel/fs/nls/nls_cp437.ko.xz",
    "kernel/fs/nls/nls_iso8859-1.ko.xz",
    "kernel/fs/nls/nls_utf8.ko.xz",
    "kernel/drivers/scsi/scsi_common.ko.xz",
    "kernel/drivers/scsi/scsi_mod.ko.xz",
    "kernel/drivers/scsi/sd_mod.ko.xz",
    "kernel/drivers/ata/libata.ko.xz",
    "kernel/drivers/ata/libahci.ko.xz",
    "kernel/drivers/ata/ahci.ko.xz",
    "kernel/drivers/nvme/host/nvme-core.ko.xz",
    "kernel/drivers/nvme/host/nvme.ko.xz",
    "kernel/drivers/net/phy/libphy.ko.xz",
    "kernel/drivers/net/ethernet/broadcom/tg3.ko.xz",
    "kernel/drivers/net/ethernet/intel/e1000e/e1000e.ko.xz",
    "kernel/drivers/net/ethernet/intel/igb/igb.ko.xz",
    "kernel/drivers/net/ethernet/realtek/r8169.ko.xz",
    "kernel/drivers/gpu/drm/amd/amdgpu/amdgpu.ko.xz",
    "kernel/drivers/usb/host/xhci-pci.ko.xz",
    "kernel/drivers/usb/host/ehci-pci.ko.xz",
    "kernel/drivers/hid/hid.ko.xz",
    "kernel/drivers/hid/hid-generic.ko.xz",
    "kernel/drivers/hid/hid-apple.ko.xz",
    "kernel/drivers/hid/usbhid/usbhid.ko.xz",
];

/// Session + kit app ELFs packed into `pkg:sola`.
const SOLA_KIT_ELFS: &[&str] = &[
    "sola-bus",
    "sola-call",
    "sola-river",
    "sola-shell",
    "sola-session",
    "sola-terminal",
    "sola-browser",
];

pub fn build(root: &Path, out: &Path, tools: &Tools) -> Result<()> {
    fs::create_dir_all(out)?;
    eprintln!("kernel={}", tools.kernel.display());
    eprintln!("modules={}", tools.modules.display());
    eprintln!("busybox={}", tools.busybox.display());

    eprintln!(">> musl binaries");
    run(Command::new("cargo").current_dir(root).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-linux-musl",
        "-p",
        "oath",
        "-p",
        "oath-init",
    ]))?;
    run(Command::new("cargo").current_dir(root).args([
        "build",
        "--release",
        "--target",
        "x86_64-unknown-uefi",
        "-p",
        "oath-efi",
        "--features",
        "uefi-app",
    ]))?;
    let bin = root.join("target/x86_64-unknown-linux-musl/release");
    for n in ["oath", "oath-init", "serial-login"] {
        if !bin.join(n).is_file() {
            bail!("missing {n}");
        }
    }

    eprintln!(">> initramfs");
    let ir = out.join("initramfs");
    let _ = fs::remove_dir_all(&ir);
    for d in ["bin", "dev", "proc", "sys", "newroot", "lib/modules"] {
        fs::create_dir_all(ir.join(d))?;
    }
    copy_file(&bin.join("oath-init"), &ir.join("init"))?;
    chmod_exec(&ir.join("init"))?;
    copy_file(&tools.busybox, &ir.join("bin/busybox"))?;
    chmod_exec(&ir.join("bin/busybox"))?;
    let _ = fs::remove_file(ir.join("bin/sh"));
    symlink("busybox", ir.join("bin/sh"))?;

    let kver = first_dir(&tools.modules).context("no kver under modules")?;
    let mdst = ir.join("lib/modules").join(&kver);
    let dep_path = tools.modules.join(&kver).join("modules.dep");
    let order = if dep_path.is_file() {
        let text = fs::read_to_string(&dep_path).context("modules.dep")?;
        resolve_load_order(&text, MODULE_ROOTS)
    } else {
        MODULE_ROOTS.iter().map(|m| m.trim_end_matches(".xz").to_string()).collect()
    };
    let mut copied = Vec::new();
    for rel in &order {
        let src_xz = tools.modules.join(&kver).join(format!("{rel}.xz"));
        let src_raw = tools.modules.join(&kver).join(rel);
        let src = if src_xz.is_file() { src_xz } else { src_raw };
        if !src.is_file() {
            eprintln!("warn: missing module {rel}");
            continue;
        }
        let dst = mdst.join(rel);
        fs::create_dir_all(dst.parent().unwrap())?;
        if src.extension().is_some_and(|e| e == "xz") {
            let raw = Command::new("xz").args(["-d", "-c"]).arg(&src).output().context("xz")?;
            if !raw.status.success() {
                bail!("xz -d {rel} failed");
            }
            fs::write(&dst, raw.stdout)?;
        } else {
            copy_file(&src, &dst)?;
        }
        copied.push(rel.clone());
    }
    fs::write(mdst.join("load-order"), copied.join("\n") + "\n")?;

    copy_firmware(tools, &ir)?;
    let initrd = write_cpio_gz(&ir, &out.join("initrd.gz"))?;
    eprintln!("initrd {}", initrd.display());

    eprintln!(">> installer initramfs");
    let ir_install = out.join("initramfs-install");
    let _ = fs::remove_dir_all(&ir_install);
    copy_tree(&ir, &ir_install)?;
    if let Some(db) = &tools.dropbear {
        copy_file(db, &ir_install.join("bin/dropbear"))?;
        chmod_exec(&ir_install.join("bin/dropbear"))?;
    }
    if let Some(dk) = &tools.dropbearkey {
        copy_file(dk, &ir_install.join("bin/dropbearkey"))?;
        chmod_exec(&ir_install.join("bin/dropbearkey"))?;
    }
    if let Some(sg) = &tools.sgdisk {
        copy_file(sg, &ir_install.join("bin/sgdisk"))?;
        chmod_exec(&ir_install.join("bin/sgdisk"))?;
    }
    if let Some(fat) = &tools.mkfs_fat {
        copy_file(fat, &ir_install.join("bin/mkfs.fat"))?;
        chmod_exec(&ir_install.join("bin/mkfs.fat"))?;
    }
    if let Some(btrfs) = &tools.btrfs {
        copy_file(btrfs, &ir_install.join("bin/btrfs"))?;
        chmod_exec(&ir_install.join("bin/btrfs"))?;
    }
    if let Some(mk) = &tools.mkfs_btrfs {
        copy_file(mk, &ir_install.join("bin/mkfs.btrfs"))?;
        chmod_exec(&ir_install.join("bin/mkfs.btrfs"))?;
    }
    if let Some(gt) = &tools.gnutar {
        let _ = fs::remove_file(ir_install.join("bin/tar"));
        copy_file(gt, &ir_install.join("bin/tar"))?;
        chmod_exec(&ir_install.join("bin/tar"))?;
    } else {
        let _ = fs::remove_file(ir_install.join("bin/tar"));
        symlink("busybox", ir_install.join("bin/tar"))?;
    }
    // busybox applets used by the host install script over SSH
    fs::create_dir_all(ir_install.join("opt/oath-install"))?;
    copy_file(&out.join("initrd.gz"), &ir_install.join("opt/oath-install/initrd.gz"))?;
    copy_file(&tools.kernel, &ir_install.join("opt/oath-install/vmlinuz"))?;
    if let Some(boot) = &tools.systemd_boot {
        copy_file(boot, &ir_install.join("opt/oath-install/systemd-bootx64.efi"))?;
    }
    let splash = root.join("target/x86_64-unknown-uefi/release/oath-efi.efi");
    if splash.is_file() {
        copy_file(&splash, &ir_install.join("opt/oath-install/BOOTX64.EFI"))?;
    } else if let Some(boot) = &tools.systemd_boot {
        copy_file(boot, &ir_install.join("opt/oath-install/BOOTX64.EFI"))?;
    }
    fs::create_dir_all(ir_install.join("usr/lib/oath"))?;
    fs::write(ir_install.join("usr/lib/oath/udhcpc.script"), include_str!("udhcpc.script"))?;
    chmod_exec(&ir_install.join("usr/lib/oath/udhcpc.script"))?;
    for a in [
        "ip",
        "udhcpc",
        "mount",
        "umount",
        "mkdir",
        "mdev",
        "mkfs.vfat",
        "blockdev",
        "reboot",
        "sync",
        "sleep",
        "cp",
        "cat",
        "sh",
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
        "bin",
        "sbin",
        "usr/lib/oath",
        "etc",
        "root",
        "tmp",
        "proc",
        "sys",
        "dev",
        "run",
        "oath",
        "lib",
    ] {
        fs::create_dir_all(stage.join(d))?;
    }
    copy_file(&bin.join("oath-init"), &stage.join("usr/lib/oath/init"))?;
    copy_file(&bin.join("serial-login"), &stage.join("usr/lib/oath/serial-login"))?;
    fs::write(stage.join("usr/lib/oath/udhcpc.script"), include_str!("udhcpc.script"))?;
    fs::write(stage.join("usr/lib/oath/run-compositor"), include_str!("run-compositor"))?;
    fs::write(stage.join("usr/lib/oath/river-boot"), include_str!("river-boot"))?;
    chmod_exec(&stage.join("usr/lib/oath/init"))?;
    chmod_exec(&stage.join("usr/lib/oath/serial-login"))?;
    chmod_exec(&stage.join("usr/lib/oath/udhcpc.script"))?;
    chmod_exec(&stage.join("usr/lib/oath/run-compositor"))?;
    chmod_exec(&stage.join("usr/lib/oath/river-boot"))?;
    let _ = fs::remove_file(stage.join("sbin/init"));
    symlink("../usr/lib/oath/init", stage.join("sbin/init"))?;
    fs::write(stage.join("etc/passwd"), "root:x:0:0:root:/root:/bin/sh\n")?;
    fs::write(stage.join("etc/group"), "root:x:0:\n")?;
    fs::write(stage.join("etc/shadow"), "root:*:1:0:99999:7:::\n")?;
    fs::write(stage.join("etc/nsswitch.conf"), "passwd: files\ngroup: files\nshadow: files\n")?;
    fs::write(stage.join("etc/hosts"), "127.0.0.1 localhost\n::1 localhost\n127.0.1.1 oath\n")?;
    fs::create_dir_all(stage.join("var/run"))?;
    fs::create_dir_all(stage.join("root/.ssh"))?;
    run(Command::new(bin.join("oath")).args([
        "--root",
        stage.join("oath").to_str().unwrap(),
        "seed",
    ]))?;

    let oath_root = stage.join("oath");
    let guest_bin = stage.join("bin");
    write_busybox_store(&oath_root, &tools.busybox)?;
    let Some(btrfs) = &tools.btrfs else {
        bail!("OATH_BTRFS / tools btrfs required (pkg:btrfs)");
    };
    write_bin_store(&oath_root, "btrfs", btrfs)?;
    write_bin_store(&oath_root, "oath", &bin.join("oath"))?;
    let Some(dropbear) = &tools.dropbear else {
        bail!("OATH_DROPBEAR / tools dropbear required (pkg:dropbear)");
    };
    let Some(dropbearkey) = &tools.dropbearkey else {
        bail!("OATH_DROPBEARKEY / tools dropbearkey required");
    };
    write_dropbear_store(&oath_root, dropbear, dropbearkey)?;
    write_bin_store(&oath_root, "hello", &root.join("apps/hello/bin/hello"))?;
    let Some(glibc) = &tools.glibc else {
        bail!("OATH_GLIBC / tools glibc required (pkg:glibc)");
    };
    let Some(river) = &tools.river else {
        bail!("OATH_RIVER / tools river required (pkg:river)");
    };
    copy_tree(glibc, &oath_root.join("store/pkg/glibc"))?;
    copy_tree(river, &oath_root.join("store/pkg/river"))?;
    chmod_exec(&oath_root.join("store/pkg/river/bin/river"))?;
    if oath_root.join("store/pkg/river/bin/seatd").is_file() {
        chmod_exec(&oath_root.join("store/pkg/river/bin/seatd"))?;
    }
    if oath_root.join("store/pkg/river/libexec/river").is_file() {
        chmod_exec(&oath_root.join("store/pkg/river/libexec/river"))?;
    }
    let sola = pack_sola(root, tools, out)?;
    copy_tree(&sola, &oath_root.join("store/pkg/sola"))?;
    for b in SOLA_KIT_ELFS.iter().copied().chain(std::iter::once("tmux")) {
        chmod_exec(&oath_root.join("store/pkg/sola/bin").join(b))?;
        chmod_exec(&oath_root.join("store/pkg/sola/libexec").join(b))?;
    }
    link_pkg(&oath_root, &guest_bin, "busybox", false)?;
    link_pkg(&oath_root, &guest_bin, "btrfs", false)?;
    link_pkg(&oath_root, &guest_bin, "oath", false)?;
    link_pkg(&oath_root, &guest_bin, "dropbear", false)?;
    link_pkg(&oath_root, &guest_bin, "glibc", false)?;
    link_pkg(&oath_root, &guest_bin, "river", false)?;
    link_pkg(&oath_root, &guest_bin, "sola", true)?;

    eprintln!(">> rootfs (btrfs subvol @) — loop-mount needs root");
    let raw = out.join("root.raw");
    let qcow = out.join("oath.qcow2");
    let _ = fs::remove_file(&raw);
    let _ = fs::remove_file(&qcow);
    run(Command::new(&tools.qemu_img).args(["create", "-f", "raw", raw.to_str().unwrap(), "2G"]))?;
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
    eprintln!("next: cargo make probe");
    Ok(())
}

fn write_busybox_store(oath_root: &Path, busybox: &Path) -> Result<()> {
    let dir = oath_root.join("store/pkg/busybox/bin");
    fs::create_dir_all(&dir)?;
    copy_file(busybox, &dir.join("busybox"))?;
    chmod_exec(&dir.join("busybox"))?;
    let list = crate::util::run_out(Command::new(busybox).arg("--list"))?;
    for a in list.split_whitespace() {
        if matches!(a, "busybox" | "hello" | "btrfs" | "oath" | "dropbear" | "dropbearkey") {
            continue;
        }
        let dest = dir.join(a);
        let _ = fs::remove_file(&dest);
        symlink("busybox", dest)?;
    }
    Ok(())
}

fn write_dropbear_store(oath_root: &Path, dropbear: &Path, dropbearkey: &Path) -> Result<()> {
    let dir = oath_root.join("store/pkg/dropbear/bin");
    fs::create_dir_all(&dir)?;
    copy_file(dropbear, &dir.join("dropbear"))?;
    copy_file(dropbearkey, &dir.join("dropbearkey"))?;
    chmod_exec(&dir.join("dropbear"))?;
    chmod_exec(&dir.join("dropbearkey"))?;
    Ok(())
}

fn pack_sola(root: &Path, tools: &Tools, out: &Path) -> Result<PathBuf> {
    let bins = sola_release_bins(root)?;
    let Some(rt) = &tools.sola_rt else {
        bail!("OATH_SOLA_RT / tools sola-rt required (pkg:sola)");
    };
    let sola_out = out.join("sola-pack");
    let _ = fs::remove_dir_all(&sola_out);
    eprintln!(">> relocate sola");
    let script = root.join("image/relocate-sola.sh");
    let mut cmd = Command::new("bash");
    cmd.arg(&script)
        .arg(&sola_out)
        .env("SOLA_BINS", &bins)
        .env("SOLA_SRC", root.join("forks/sola"));
    let share = PathBuf::from("/opt/sola/share");
    if share.is_dir() {
        cmd.env("SOLA_SHARE", &share);
    }
    for (key, name) in [
        ("WAYLAND", "wayland"),
        ("XKBCOMMON", "xkbcommon"),
        ("LIBFFI", "libffi"),
        ("LIBGLVND", "libglvnd"),
        ("VULKAN_LOADER", "vulkan-loader"),
        ("FONTCONFIG", "fontconfig"),
        ("FREETYPE", "freetype"),
        ("INTER", "inter"),
        ("TMUX_BIN", "tmux"),
        ("NCURSES", "ncurses"),
        ("LOCALES", "locales"),
        ("JETBRAINS_MONO", "jetbrains-mono"),
        ("IOSEVKA_TERM_SLAB", "iosevka-term-slab"),
        ("CACERT", "cacert"),
        ("LIBX11", "libx11"),
    ] {
        let p = rt.join(name);
        if p.exists() {
            cmd.env(key, p);
        }
    }
    if let Some(sf) = sola_sf_fonts(root) {
        cmd.env("SOLA_SF_FONTS", sf);
    }
    if let Some(cef) = sola_cef_dir(root) {
        cmd.env("CEF_DIR", cef);
    }
    run(&mut cmd)?;
    Ok(sola_out)
}

/// Host `cargo make install-cef` cache. Not in git.
fn sola_cef_dir(root: &Path) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SOLA_CEF_DIR").or_else(|| std::env::var_os("CEF_DIR")) {
        let p = PathBuf::from(p);
        if p.join("Release").join("libcef.so").is_file() || p.join("libcef.so").is_file() {
            return Some(p);
        }
    }
    let ver = fs::read_to_string(root.join("forks/sola/cef-version")).ok()?;
    let ver = ver.trim();
    if ver.is_empty() {
        return None;
    }
    let home = std::env::var_os("HOME")?;
    let p = PathBuf::from(home).join(".cache/sola").join(format!("cef-{ver}"));
    (p.join("Release").join("libcef.so").is_file() || p.join("libcef.so").is_file()).then_some(p)
}

/// Licensed SF Pro Text faces. Not in git; pack from the operator stash.
fn sola_sf_fonts(root: &Path) -> Option<PathBuf> {
    let mut cands = Vec::new();
    if let Some(p) = std::env::var_os("SOLA_SF_FONTS") {
        cands.push(PathBuf::from(p));
    }
    if let Some(home) = std::env::var_os("HOME") {
        cands.push(PathBuf::from(home).join(".local/share/fonts/sola-sf"));
    }
    cands.push(root.join("forks/sola/.local/fonts/SF"));
    cands.into_iter().find(|p| {
        p.is_dir()
            && fs::read_dir(p).ok().is_some_and(|rd| {
                rd.filter_map(|e| e.ok())
                    .any(|e| e.file_name().to_string_lossy().starts_with("SF-Pro-Text-"))
            })
    })
}

fn sola_release_bins(root: &Path) -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("OATH_SOLA_BINS") {
        let p = PathBuf::from(p);
        if p.join("sola-bus").is_file() {
            return Ok(p);
        }
        bail!("OATH_SOLA_BINS missing sola-bus: {}", p.display());
    }
    let src = root.join("forks/sola");
    if !src.join("Cargo.toml").is_file() {
        bail!("forks/sola missing (git submodule?)");
    }
    let target = out_dir(root).join("sola-target");
    fs::create_dir_all(&target)?;
    let wt = ensure_sola_worktree(&src)?;
    eprintln!(">> cargo build sola session ({})", wt.display());
    let mut args = vec!["build".to_string(), "--release".to_string()];
    for n in SOLA_KIT_ELFS {
        args.push("-p".to_string());
        args.push((*n).to_string());
    }
    run(Command::new("cargo").current_dir(&wt).env("CARGO_TARGET_DIR", &target).args(&args))?;
    let bins = target.join("release");
    for n in SOLA_KIT_ELFS {
        if !bins.join(n).is_file() {
            bail!("missing sola {n} in {}", bins.display());
        }
    }
    Ok(bins)
}

fn ensure_sola_worktree(src: &Path) -> Result<PathBuf> {
    let wt = std::env::temp_dir().join("oath-sola-build");
    let head = run_out(Command::new("git").current_dir(src).args(["rev-parse", "HEAD"]))?;
    if wt.join("Cargo.toml").is_file() {
        run(Command::new("git").current_dir(&wt).args(["checkout", "--detach", "--quiet", &head]))?;
    } else {
        run(Command::new("git").current_dir(src).args([
            "worktree",
            "add",
            "--detach",
            wt.to_str().unwrap(),
            &head,
        ]))?;
    }
    Ok(wt)
}

fn write_cpio_gz(tree: &Path, dest: &Path) -> Result<PathBuf> {
    let f = fs::File::create(dest)?;
    let mut gz = GzEncoder::new(f, Compression::best());
    cpio::write_tree(&mut gz, tree)?;
    gz.finish()?;
    Ok(dest.to_path_buf())
}

fn copy_firmware(tools: &Tools, ir: &Path) -> Result<()> {
    if let Some(fw) = &tools.firmware {
        if fw.is_dir() {
            copy_tree(fw, &ir.join("lib/firmware"))?;
        }
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
pub(crate) fn resolve_load_order(dep_text: &str, roots: &[&str]) -> Vec<String> {
    use std::collections::{HashMap, HashSet, VecDeque};

    let mut graph: HashMap<String, Vec<String>> = HashMap::new();
    for line in dep_text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        graph.insert(key.to_string(), rest.split_whitespace().map(|s| s.to_string()).collect());
    }

    let mut rank: HashMap<String, usize> = HashMap::new();
    let mut need: HashSet<String> = HashSet::new();
    let mut q = VecDeque::new();
    for r in roots {
        q.push_back((*r).to_string());
    }
    let mut next_rank = 0usize;
    while let Some(n) = q.pop_front() {
        if !need.insert(n.clone()) {
            continue;
        }
        rank.entry(n.clone()).or_insert_with(|| {
            let r = next_rank;
            next_rank += 1;
            r
        });
        if let Some(deps) = graph.get(&n) {
            for d in deps {
                if !need.contains(d) {
                    q.push_back(d.clone());
                }
            }
        }
    }

    let mut indeg: HashMap<String, usize> = need.iter().map(|n| (n.clone(), 0)).collect();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for n in &need {
        if let Some(deps) = graph.get(n) {
            for d in deps {
                if need.contains(d) {
                    adj.entry(d.clone()).or_default().push(n.clone());
                    *indeg.get_mut(n).unwrap() += 1;
                }
            }
        }
    }

    let mut zero: Vec<String> =
        indeg.iter().filter(|(_, d)| **d == 0).map(|(k, _)| k.clone()).collect();
    zero.sort_by_key(|n| rank.get(n).copied().unwrap_or(usize::MAX));
    let mut order = Vec::new();
    while let Some(n) = {
        if zero.is_empty() {
            None
        } else {
            Some(zero.remove(0))
        }
    } {
        order.push(n.clone());
        if let Some(children) = adj.get(&n) {
            let mut next = children.clone();
            next.sort_by_key(|m| rank.get(m).copied().unwrap_or(usize::MAX));
            for m in next {
                if let Some(e) = indeg.get_mut(&m) {
                    *e = e.saturating_sub(1);
                    if *e == 0 {
                        zero.push(m);
                        zero.sort_by_key(|x| rank.get(x).copied().unwrap_or(usize::MAX));
                    }
                }
            }
        }
    }
    for n in &need {
        if !order.iter().any(|x| x == n) {
            order.push(n.clone());
        }
    }
    order.into_iter().map(|p| p.trim_end_matches(".xz").to_string()).collect()
}

fn write_bin_store(oath_root: &Path, name: &str, src: &Path) -> Result<()> {
    let dest = oath_root.join("store/pkg").join(name).join("bin").join(name);
    fs::create_dir_all(dest.parent().unwrap())?;
    copy_file(src, &dest)?;
    chmod_exec(&dest)?;
    Ok(())
}

fn link_pkg(oath_root: &Path, bin_dir: &Path, name: &str, removable: bool) -> Result<()> {
    let mut actual = converge_with_link_root(oath_root, bin_dir, Path::new("/oath"), name, true)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    actual.removable = removable;
    write_json(&oath_root.join("objects/pkg").join(name).join("actual.json"), &actual)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}

fn first_dir(modules: &Path) -> Option<String> {
    let mut names: Vec<String> = fs::read_dir(modules)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names.into_iter().next()
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
    fn resolve_load_order_deps_before_user() {
        let dep = "\
a.ko.xz: b.ko.xz c.ko.xz
b.ko.xz: c.ko.xz
c.ko.xz:
d.ko.xz: a.ko.xz
";
        let order = resolve_load_order(dep, &["a.ko.xz", "d.ko.xz"]);
        let pos = |n: &str| order.iter().position(|x| x == n).unwrap();
        assert!(pos("c.ko") < pos("b.ko"));
        assert!(pos("b.ko") < pos("a.ko"));
        assert!(pos("a.ko") < pos("d.ko"));
        assert_eq!(order.len(), 4);
    }

    #[test]
    fn resolve_load_order_af_packet_and_libphy() {
        let dep = "\
kernel/drivers/net/phy/libphy.ko.xz: kernel/drivers/leds/led-class.ko.xz
kernel/drivers/leds/led-class.ko.xz:
kernel/net/packet/af_packet.ko.xz:
kernel/drivers/net/virtio_net.ko.xz:
";
        let order = resolve_load_order(
            dep,
            &[
                "kernel/drivers/net/virtio_net.ko.xz",
                "kernel/net/packet/af_packet.ko.xz",
                "kernel/drivers/net/phy/libphy.ko.xz",
            ],
        );
        let pos = |n: &str| order.iter().position(|x| x == n).unwrap();
        assert!(pos("kernel/drivers/leds/led-class.ko") < pos("kernel/drivers/net/phy/libphy.ko"));
        assert!(order.iter().any(|x| x == "kernel/net/packet/af_packet.ko"));
    }
}
