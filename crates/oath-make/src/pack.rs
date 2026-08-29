use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;

use crate::cpio;
use crate::tools::Tools;
use crate::util::{chmod_exec, copy_file, run, sudo};

const MODULES: &[&str] = &[
    "kernel/drivers/virtio/virtio.ko.xz",
    "kernel/drivers/virtio/virtio_ring.ko.xz",
    "kernel/drivers/virtio/virtio_pci_legacy_dev.ko.xz",
    "kernel/drivers/virtio/virtio_pci_modern_dev.ko.xz",
    "kernel/drivers/virtio/virtio_pci.ko.xz",
    "kernel/drivers/block/virtio_blk.ko.xz",
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
    copy_file(&tools.busybox, &stage.join("bin/busybox"))?;
    chmod_exec(&stage.join("bin/busybox"))?;
    let list = crate::util::run_out(Command::new(stage.join("bin/busybox")).arg("--list"))?;
    for a in list.split_whitespace() {
        let _ = fs::remove_file(stage.join("bin").join(a));
        symlink("busybox", stage.join("bin").join(a))?;
    }
    // pkg:hello owns /bin/hello. Drop a busybox applet of that name if present.
    let _ = fs::remove_file(stage.join("bin/hello"));
    if let Some(btrfs) = &tools.btrfs {
        copy_file(btrfs, &stage.join("bin/btrfs"))?;
        chmod_exec(&stage.join("bin/btrfs"))?;
    }
    copy_file(&bin.join("oath"), &stage.join("bin/oath"))?;
    copy_file(&bin.join("oath-init"), &stage.join("usr/lib/oath/init"))?;
    copy_file(&bin.join("serial-login"), &stage.join("usr/lib/oath/serial-login"))?;
    chmod_exec(&stage.join("bin/oath"))?;
    chmod_exec(&stage.join("usr/lib/oath/init"))?;
    chmod_exec(&stage.join("usr/lib/oath/serial-login"))?;
    let _ = fs::remove_file(stage.join("sbin/init"));
    symlink("../usr/lib/oath/init", stage.join("sbin/init"))?;
    fs::write(stage.join("etc/passwd"), "root:x:0:0:root:/root:/bin/sh\n")?;
    fs::write(stage.join("etc/group"), "root:x:0:\n")?;
    run(Command::new(bin.join("oath")).args([
        "--root",
        stage.join("oath").to_str().unwrap(),
        "seed",
    ]))?;
    let hello = stage.join("oath/store/pkg/hello/bin/hello");
    fs::create_dir_all(hello.parent().unwrap())?;
    fs::write(&hello, "#!/bin/sh\nprintf 'hello\\n'\n")?;
    chmod_exec(&hello)?;

    eprintln!(">> rootfs (btrfs subvol @) — loop-mount needs root");
    let raw = out.join("root.raw");
    let qcow = out.join("oath.qcow2");
    let _ = fs::remove_file(&raw);
    let _ = fs::remove_file(&qcow);
    run(Command::new(&tools.qemu_img).args([
        "create",
        "-f",
        "raw",
        raw.to_str().unwrap(),
        "512M",
    ]))?;
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
