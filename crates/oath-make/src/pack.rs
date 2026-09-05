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
    // Canto: Intel HDA 8086:1d20 (Cirrus) + Pitcairn HDMI 1002:aab0.
    // QEMU: virtio_snd. USB: headset. Transitive snd-* from modules.dep.
    "kernel/sound/pci/hda/snd-hda-intel.ko.xz",
    "kernel/sound/pci/hda/snd-hda-codec-hdmi.ko.xz",
    "kernel/sound/pci/hda/snd-hda-codec-cirrus.ko.xz",
    "kernel/sound/pci/hda/snd-hda-codec-generic.ko.xz",
    "kernel/sound/usb/snd-usb-audio.ko.xz",
    "kernel/sound/virtio/virtio_snd.ko.xz",
    // T33: NFS client for off-box btrfs send. Deps (sunrpc, lockd, netfs, …)
    // come from modules.dep.
    "kernel/fs/nfs/nfs.ko.xz",
    "kernel/fs/nfs/nfsv4.ko.xz",
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
    "sola-workspaces",
    "solactl",
    "sola-kvm",
    "sola-settings",
    "sola-monitor",
    "sola-kit",
    "sola-preview",
    "sola-paint",
    "sola-mail",
    "sola-arcade",
    "sola-scope",
    "sola-spotify",
    "sola-wrapper",
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
    for n in ["oath", "oath-init", "serial-login", "sudo"] {
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
    if let Some(scp) = &tools.dropbear_scp {
        copy_file(scp, &ir_install.join("bin/scp"))?;
        chmod_exec(&ir_install.join("bin/scp"))?;
    }
    if let Some(sftp) = &tools.sftp_server {
        copy_file(sftp, &ir_install.join("bin/sftp-server"))?;
        chmod_exec(&ir_install.join("bin/sftp-server"))?;
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
    fs::create_dir_all(ir_install.join("lib/oath"))?;
    fs::write(ir_install.join("lib/oath/udhcpc.script"), include_str!("udhcpc.script"))?;
    chmod_exec(&ir_install.join("lib/oath/udhcpc.script"))?;
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
        "bin", "sbin", "lib/oath", "etc", "root", "home", "tmp", "proc", "sys", "dev", "run",
        "oath", "lib",
    ] {
        fs::create_dir_all(stage.join(d))?;
    }
    copy_file(&bin.join("oath-init"), &stage.join("lib/oath/init"))?;
    copy_file(&bin.join("serial-login"), &stage.join("lib/oath/serial-login"))?;
    copy_file(&bin.join("sudo"), &stage.join("lib/oath/sudo"))?;
    fs::write(stage.join("lib/oath/udhcpc.script"), include_str!("udhcpc.script"))?;
    fs::write(stage.join("lib/oath/run-compositor"), include_str!("run-compositor"))?;
    fs::write(stage.join("lib/oath/river-boot"), include_str!("river-boot"))?;
    fs::write(stage.join("lib/oath/display-env.sh"), include_str!("display-env.sh"))?;
    fs::write(stage.join("lib/oath/with-seat-tz"), include_str!("with-seat-tz"))?;
    fs::write(stage.join("lib/oath/backup-send"), include_str!("backup-send"))?;
    fs::write(stage.join("lib/oath/backup-daily"), include_str!("backup-daily"))?;
    chmod_exec(&stage.join("lib/oath/init"))?;
    chmod_exec(&stage.join("lib/oath/serial-login"))?;
    chmod_exec(&stage.join("lib/oath/udhcpc.script"))?;
    chmod_exec(&stage.join("lib/oath/run-compositor"))?;
    chmod_exec(&stage.join("lib/oath/river-boot"))?;
    chmod_exec(&stage.join("lib/oath/backup-send"))?;
    chmod_exec(&stage.join("lib/oath/backup-daily"))?;
    chmod_exec(&stage.join("lib/oath/with-seat-tz"))?;
    fs::set_permissions(stage.join("lib/oath/sudo"), fs::Permissions::from_mode(0o4755))?;
    let _ = fs::remove_file(stage.join("sbin/init"));
    symlink("../lib/oath/init", stage.join("sbin/init"))?;
    let _ = fs::remove_file(stage.join("bin/sudo"));
    symlink("../lib/oath/sudo", stage.join("bin/sudo"))?;
    fs::write(stage.join("etc/passwd"), oath_core::seat::passwd_file())?;
    fs::write(stage.join("etc/group"), oath_core::seat::group_file())?;
    fs::write(stage.join("etc/shadow"), oath_core::seat::shadow_file())?;
    fs::write(stage.join("etc/shells"), oath_core::seat::shells_file())?;
    fs::write(stage.join("etc/nsswitch.conf"), "passwd: files\ngroup: files\nshadow: files\n")?;
    fs::write(stage.join("etc/hosts"), "127.0.0.1 localhost\n::1 localhost\n127.0.1.1 oath\n")?;
    fs::write(
        stage.join("etc/profile"),
        oath_core::seat::profile_body(&{
            let mut m = std::collections::BTreeMap::new();
            m.insert("GROK_DISABLE_AUTOUPDATER".into(), "1".into());
            m.insert("SHELL".into(), "/bin/thoxa".into());
            m.insert("THOXA_ROOT".into(), "/oath/store/pkg/thoxa".into());
            m.insert("CC".into(), "/bin/cc".into());
            m.insert("CXX".into(), "/bin/c++".into());
            m.insert("AR".into(), "/bin/ar".into());
            m.insert(
                "CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER".into(),
                "/bin/musl-cc".into(),
            );
            m.insert("CMAKE_GENERATOR".into(), "Ninja".into());
            m
        }),
    )?;
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
    let Some(dropbear_dbclient) = &tools.dropbear_dbclient else {
        bail!("OATH_DROPBEAR_DBCLIENT / tools dropbear-dbclient required");
    };
    let Some(dropbear_scp) = &tools.dropbear_scp else {
        bail!("OATH_DROPBEAR_SCP / tools dropbear-scp required (pkg:dropbear scp)");
    };
    let Some(openssh_ssh) = &tools.openssh_ssh else {
        bail!("OATH_OPENSSH_SSH / tools openssh-ssh required (guest /bin/ssh)");
    };
    let Some(openssh_ssh_keygen) = &tools.openssh_ssh_keygen else {
        bail!("OATH_OPENSSH_SSH_KEYGEN / tools openssh-ssh-keygen required");
    };
    let Some(sftp_server) = &tools.sftp_server else {
        bail!("OATH_SFTP_SERVER / tools sftp-server required (pkg:dropbear sftp)");
    };
    write_dropbear_store(
        &oath_root,
        dropbear,
        dropbearkey,
        dropbear_dbclient,
        dropbear_scp,
        openssh_ssh,
        openssh_ssh_keygen,
        sftp_server,
    )?;
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
    let grok = grok_elf()?;
    eprintln!("grok={}", grok.display());
    write_bin_store(&oath_root, "grok", &grok)?;
    link_pkg(&oath_root, &guest_bin, "grok", true)?;
    pack_curl(&oath_root, tools)?;
    link_pkg(&oath_root, &guest_bin, "curl", true)?;
    pack_git(root, tools, out, &oath_root)?;
    link_pkg(&oath_root, &guest_bin, "git", true)?;
    pack_pipewire(root, tools, out, &oath_root)?;
    link_pkg(&oath_root, &guest_bin, "pipewire", true)?;
    pack_thoxa(root, out, &oath_root)?;
    link_pkg(&oath_root, &guest_bin, "thoxa", true)?;
    let cc_pack = pack_cc(root, out)?;
    copy_tree(&cc_pack, &oath_root.join("store/pkg/cc"))?;
    chmod_exec(&oath_root.join("store/pkg/cc/libexec/zig/zig"))?;
    chmod_exec(&oath_root.join("store/pkg/cc/libexec/patchelf"))?;
    link_pkg(&oath_root, &guest_bin, "cc", true)?;
    let patchelf = oath_root.join("store/pkg/cc/libexec/patchelf");
    pack_script_pkg(root, out, &oath_root, "pack-pkg-config.sh", "pkg-config", &[])?;
    link_pkg(&oath_root, &guest_bin, "pkg-config", true)?;
    pack_script_pkg(
        root,
        out,
        &oath_root,
        "pack-cmake.sh",
        "cmake",
        &[("PATCHELF", &patchelf)],
    )?;
    link_pkg(&oath_root, &guest_bin, "cmake", true)?;
    pack_script_pkg(
        root,
        out,
        &oath_root,
        "pack-rustc.sh",
        "rustc",
        &[("PATCHELF", &patchelf)],
    )?;
    link_pkg(&oath_root, &guest_bin, "rustc", true)?;
    pack_script_pkg(root, out, &oath_root, "pack-bash.sh", "bash", &[])?;
    link_pkg(&oath_root, &guest_bin, "bash", true)?;

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
        if matches!(
            a,
            "busybox"
                | "hello"
                | "btrfs"
                | "oath"
                | "dropbear"
                | "dropbearkey"
                | "dbclient"
                | "ssh"
                | "ssh-keygen"
                | "scp"
                | "sftp-server"
                | "sudo"
                | "xdg-open"
                | "x-www-browser"
        ) {
            continue;
        }
        let dest = dir.join(a);
        let _ = fs::remove_file(&dest);
        symlink("busybox", dest)?;
    }
    Ok(())
}

fn write_dropbear_store(
    oath_root: &Path,
    dropbear: &Path,
    dropbearkey: &Path,
    dbclient: &Path,
    scp: &Path,
    ssh: &Path,
    ssh_keygen: &Path,
    sftp_server: &Path,
) -> Result<()> {
    let dir = oath_root.join("store/pkg/dropbear/bin");
    fs::create_dir_all(&dir)?;
    copy_file(dropbear, &dir.join("dropbear"))?;
    copy_file(dropbearkey, &dir.join("dropbearkey"))?;
    copy_file(dbclient, &dir.join("dbclient"))?;
    copy_file(ssh, &dir.join("ssh"))?;
    copy_file(ssh_keygen, &dir.join("ssh-keygen"))?;
    copy_file(scp, &dir.join("scp"))?;
    copy_file(sftp_server, &dir.join("sftp-server"))?;
    chmod_exec(&dir.join("dropbear"))?;
    chmod_exec(&dir.join("dropbearkey"))?;
    chmod_exec(&dir.join("dbclient"))?;
    chmod_exec(&dir.join("ssh"))?;
    chmod_exec(&dir.join("ssh-keygen"))?;
    chmod_exec(&dir.join("scp"))?;
    chmod_exec(&dir.join("sftp-server"))?;
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

fn pack_curl(oath_root: &Path, tools: &Tools) -> Result<()> {
    let curl = tools.curl.as_ref().context("OATH_CURL / tools curl required (pkg:curl)")?;
    let cacert = tools.cacert.as_ref().context("OATH_CACERT / tools ca-bundle required")?;
    let dest = oath_root.join("store/pkg/curl");
    fs::create_dir_all(dest.join("bin"))?;
    fs::create_dir_all(dest.join("libexec"))?;
    fs::create_dir_all(dest.join("ssl"))?;
    copy_file(curl, &dest.join("libexec/curl"))?;
    chmod_exec(&dest.join("libexec/curl"))?;
    copy_file(cacert, &dest.join("ssl/cert.pem"))?;
    fs::write(
        dest.join("bin/curl"),
        "#!/bin/sh\nexport CURL_CA_BUNDLE=\"${CURL_CA_BUNDLE:-/oath/store/pkg/curl/ssl/cert.pem}\"\nexport SSL_CERT_FILE=\"${SSL_CERT_FILE:-/oath/store/pkg/curl/ssl/cert.pem}\"\nexec /oath/store/pkg/curl/libexec/curl \"$@\"\n",
    )?;
    chmod_exec(&dest.join("bin/curl"))?;
    Ok(())
}

fn pack_pipewire(root: &Path, tools: &Tools, out: &Path, oath_root: &Path) -> Result<()> {
    let pw = dir_or_nix(tools.pipewire.as_ref(), "pipewire")?;
    let wp = dir_or_nix(tools.wireplumber.as_ref(), "wireplumber")?;
    let alsa = dir_or_nix(tools.alsa_lib.as_ref(), "alsa-lib")?;
    let pulse = dir_or_nix(tools.libpulse.as_ref(), "libpulseaudio")?;
    let pw_out = out.join("pipewire-pack");
    let _ = fs::remove_dir_all(&pw_out);
    eprintln!(">> relocate pipewire");
    run(Command::new("bash")
        .arg(root.join("image/relocate-pipewire.sh"))
        .arg(&pw_out)
        .env("PIPEWIRE", &pw)
        .env("WIREPLUMBER", &wp)
        .env("ALSA_LIB", &alsa)
        .env("LIBPULSE", &pulse))?;
    copy_tree(&pw_out, &oath_root.join("store/pkg/pipewire"))?;
    for b in [
        "pipewire",
        "pipewire-pulse",
        "wireplumber",
        "wpctl",
        "pw-dump",
        "pw-cat",
        "pw-cli",
        "pw-play",
        "pw-record",
    ] {
        chmod_exec(&oath_root.join("store/pkg/pipewire/bin").join(b))?;
        let libexec = oath_root.join("store/pkg/pipewire/libexec").join(b);
        if libexec.is_file() && !libexec.symlink_metadata()?.file_type().is_symlink() {
            chmod_exec(&libexec)?;
        }
    }
    Ok(())
}

fn dir_or_nix(have: Option<&PathBuf>, attr: &str) -> Result<PathBuf> {
    if let Some(p) = have {
        if p.is_dir() {
            return Ok(p.clone());
        }
    }
    let key = format!("OATH_{}", attr.to_ascii_uppercase().replace('-', "_"));
    if let Some(p) = std::env::var_os(&key) {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Ok(p);
        }
    }
    eprintln!("nix-build <nixpkgs> -A {attr}");
    let out = run_out(Command::new("nix-build").args(["--no-out-link", "<nixpkgs>", "-A", attr]))?;
    let p = PathBuf::from(out.trim());
    if !p.is_dir() && !p.is_file() {
        bail!("{attr} nix-build is not a path: {}", p.display());
    }
    // alsa-lib / libpulseaudio / pipewire are store dirs.
    if p.is_file() {
        return p
            .parent()
            .map(|d| d.to_path_buf())
            .with_context(|| format!("{attr} is a file with no parent"));
    }
    Ok(p)
}

fn pack_git(root: &Path, tools: &Tools, out: &Path, oath_root: &Path) -> Result<()> {
    let git = tools.git.as_ref().context("OATH_GIT / tools git required (pkg:git)")?;
    let cacert = tools.cacert.as_ref().context("OATH_CACERT / tools ca-bundle required")?;
    let git_out = out.join("git-pack");
    let _ = fs::remove_dir_all(&git_out);
    eprintln!(">> relocate git");
    run(Command::new("bash")
        .arg(root.join("image/relocate-git.sh"))
        .arg(&git_out)
        .env("GIT", git))?;
    copy_file(cacert, &git_out.join("ssl/cert.pem"))?;
    copy_tree(&git_out, &oath_root.join("store/pkg/git"))?;
    chmod_exec(&oath_root.join("store/pkg/git/bin/git"))?;
    Ok(())
}

fn thoxa_src(root: &Path) -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("OATH_THOXA") {
        let p = PathBuf::from(p);
        if p.join("crates/compiler/Cargo.toml").is_file() {
            return Ok(p);
        }
        bail!("OATH_THOXA missing crates/compiler: {}", p.display());
    }
    let sibling = root.join("../Thoxa");
    if sibling.join("crates/compiler/Cargo.toml").is_file() {
        return fs::canonicalize(&sibling)
            .with_context(|| format!("canonicalize {}", sibling.display()));
    }
    bail!("Thoxa tree not found (set OATH_THOXA or put Thoxa next to Oath)")
}

fn pack_cc(root: &Path, out: &Path) -> Result<PathBuf> {
    let dest = out.join("cc-pack");
    eprintln!(">> pack cc (zig)");
    run(Command::new("sh")
        .arg(root.join("image/pack-cc.sh"))
        .arg(&dest)
        .env("OATH_FETCH", out.join("fetch")))?;
    Ok(dest)
}

fn pack_script_pkg(
    root: &Path,
    out: &Path,
    oath_root: &Path,
    script: &str,
    name: &str,
    extra: &[(&str, &PathBuf)],
) -> Result<()> {
    let dest = out.join(format!("{name}-pack"));
    eprintln!(">> pack {name}");
    let mut cmd = Command::new("sh");
    cmd.arg(root.join("image").join(script))
        .arg(&dest)
        .env("OATH_FETCH", out.join("fetch"));
    for (k, v) in extra {
        cmd.env(*k, v);
    }
    run(&mut cmd)?;
    copy_tree(&dest, &oath_root.join("store/pkg").join(name))?;
    Ok(())
}

fn pack_thoxa(root: &Path, out: &Path, oath_root: &Path) -> Result<()> {
    let src = thoxa_src(root)?;
    eprintln!(">> cargo build thoxa ({})", src.display());
    run(Command::new("cargo").current_dir(&src).args(["build", "--release", "-p", "thoxa"]))?;
    let bin = src.join("target/rust/release/thoxa");
    if !bin.is_file() {
        bail!("missing thoxa ELF at {}", bin.display());
    }
    // Session steps link this archive; build it if cargo didn't.
    if !src.join("target/c/libthoxa_rt.a").is_file() {
        eprintln!(">> cargo make runtime");
        run(Command::new("cargo").current_dir(&src).args(["make", "runtime"]))?;
    }
    let thoxa_out = out.join("thoxa-pack");
    let _ = fs::remove_dir_all(&thoxa_out);
    eprintln!(">> relocate thoxa");
    run(Command::new("bash")
        .arg(root.join("image/relocate-thoxa.sh"))
        .arg(&thoxa_out)
        .env("THOXA_SRC", &src)
        .env("THOXA_BIN", &bin))?;
    copy_tree(&thoxa_out, &oath_root.join("store/pkg/thoxa"))?;
    chmod_exec(&oath_root.join("store/pkg/thoxa/bin/thoxa"))?;
    chmod_exec(&oath_root.join("store/pkg/thoxa/libexec/thoxa"))?;
    Ok(())
}

/// Borrowed static-pie Grok ELF (T30). Not in nix; not in `pkg:sola`.
fn grok_elf() -> Result<PathBuf> {
    let mut cands = Vec::new();
    if let Some(p) = std::env::var_os("OATH_GROK") {
        cands.push(PathBuf::from(p));
    }
    if let Ok(out) = Command::new("sh").args(["-c", "command -v grok"]).output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                cands.push(PathBuf::from(s));
            }
        }
    }
    if let Some(h) = std::env::var_os("HOME") {
        cands.push(PathBuf::from(h).join(".grok/bin/grok"));
    }
    for p in cands {
        if p.is_file() {
            return fs::canonicalize(&p)
                .with_context(|| format!("canonicalize grok ELF {}", p.display()));
        }
    }
    bail!("OATH_GROK: no grok ELF (set OATH_GROK or put grok on PATH)")
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
