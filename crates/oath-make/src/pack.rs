use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;

use oath_core::{converge_with_link_root, write_json};

use crate::cpio;
use crate::tools::Tools;
use crate::util::{chmod_exec, copy_file, copy_tree, run, sudo};

const MODULES: &[&str] = &[
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
    "kernel/drivers/char/hw_random/rng-core.ko.xz",
    "kernel/drivers/char/hw_random/virtio-rng.ko.xz",
    "kernel/drivers/firmware/qemu_fw_cfg.ko.xz",
    "kernel/crypto/crc32c_generic.ko.xz",
    "kernel/lib/libcrc32c.ko.xz",
    "kernel/crypto/xor.ko.xz",
    "kernel/lib/raid6/raid6_pq.ko.xz",
    "kernel/fs/btrfs/btrfs.ko.xz",
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
    for m in MODULES {
        let src = tools.modules.join(&kver).join(m);
        if !src.is_file() {
            eprintln!("warn: missing module {m}");
            continue;
        }
        let dst_name = m.trim_end_matches(".xz");
        let dst = mdst.join(dst_name);
        fs::create_dir_all(dst.parent().unwrap())?;
        let raw = Command::new("xz").args(["-d", "-c"]).arg(&src).output().context("xz")?;
        if !raw.status.success() {
            bail!("xz -d {m} failed");
        }
        fs::write(dst, raw.stdout)?;
    }

    let initrd = out.join("initrd.gz");
    {
        let f = fs::File::create(&initrd)?;
        let mut gz = GzEncoder::new(f, Compression::best());
        cpio::write_tree(&mut gz, &ir)?;
        gz.finish()?;
    }
    eprintln!("initrd {}", initrd.display());

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
    chmod_exec(&stage.join("usr/lib/oath/init"))?;
    chmod_exec(&stage.join("usr/lib/oath/serial-login"))?;
    chmod_exec(&stage.join("usr/lib/oath/udhcpc.script"))?;
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
    link_pkg(&oath_root, &guest_bin, "busybox", false)?;
    link_pkg(&oath_root, &guest_bin, "btrfs", false)?;
    link_pkg(&oath_root, &guest_bin, "oath", false)?;
    link_pkg(&oath_root, &guest_bin, "dropbear", false)?;
    link_pkg(&oath_root, &guest_bin, "glibc", false)?;
    link_pkg(&oath_root, &guest_bin, "river", false)?;

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
    sudo(&["mount", "-o", "loop", raw.to_str().unwrap(), mnt.to_str().unwrap()])?;
    let mount_ok = (|| -> Result<()> {
        sudo(&["btrfs", "subvolume", "create", mnt.join("@").to_str().unwrap()])?;
        sudo(&["mount", "-o", "loop,subvol=@", raw.to_str().unwrap(), rootfs.to_str().unwrap()])?;
        let stage_dot = format!("{}/.", stage.display());
        sudo(&["cp", "-a", &stage_dot, &format!("{}/", rootfs.display())])?;
        sudo(&["chown", "-R", "0:0", rootfs.to_str().unwrap()])?;
        sudo(&["umount", rootfs.to_str().unwrap()])?;
        Ok(())
    })();
    let um = sudo(&["umount", mnt.to_str().unwrap()]);
    mount_ok.and(um)?;
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
