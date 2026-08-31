use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::util::{prepend_path, run_out, which};

pub struct Tools {
    pub kernel: PathBuf,
    pub modules: PathBuf,
    pub busybox: PathBuf,
    pub btrfs: Option<PathBuf>,
    pub dropbear: Option<PathBuf>,
    pub dropbearkey: Option<PathBuf>,
    pub glibc: Option<PathBuf>,
    pub river: Option<PathBuf>,
    pub sola_rt: Option<PathBuf>,
    pub qemu: PathBuf,
    pub qemu_img: PathBuf,
}

pub fn load(root: &Path) -> Result<Tools> {
    let mut kernel = std::env::var_os("OATH_KERNEL").map(PathBuf::from);
    let mut modules = std::env::var_os("OATH_MODULES").map(PathBuf::from);
    let mut busybox = std::env::var_os("OATH_BUSYBOX").map(PathBuf::from);
    let mut btrfs = std::env::var_os("OATH_BTRFS").map(PathBuf::from);
    let mut dropbear = std::env::var_os("OATH_DROPBEAR").map(PathBuf::from);
    let mut dropbearkey = std::env::var_os("OATH_DROPBEARKEY").map(PathBuf::from);
    let mut glibc = std::env::var_os("OATH_GLIBC").map(PathBuf::from);
    let mut river = std::env::var_os("OATH_RIVER").map(PathBuf::from);
    let mut sola_rt = std::env::var_os("OATH_SOLA_RT").map(PathBuf::from);

    if kernel.is_none() || modules.is_none() || busybox.is_none() {
        eprintln!("loading tools via nix-build image/tools.nix ...");
        let tools = PathBuf::from(run_out(
            Command::new("nix-build")
                .args([root.join("image/tools.nix").to_str().unwrap(), "--no-out-link"]),
        )?);
        prepend_path(&tools.join("bin"));
        kernel = kernel.or_else(|| Some(tools.join("bzImage")));
        modules = modules.or_else(|| Some(tools.join("modules")));
        busybox = busybox.or_else(|| Some(tools.join("busybox")));
        btrfs = btrfs.or_else(|| {
            let p = tools.join("btrfs");
            p.is_file().then_some(p)
        });
        dropbear = dropbear.or_else(|| {
            let p = tools.join("dropbear");
            p.is_file().then_some(p)
        });
        dropbearkey = dropbearkey.or_else(|| {
            let p = tools.join("dropbearkey");
            p.is_file().then_some(p)
        });
        glibc = glibc.or_else(|| {
            let p = tools.join("glibc");
            p.is_dir().then_some(p)
        });
        river = river.or_else(|| {
            let p = tools.join("river");
            p.is_dir().then_some(p)
        });
        sola_rt = sola_rt.or_else(|| {
            let p = tools.join("sola-rt");
            p.is_dir().then_some(p)
        });
        let musl = tools.join("musl-cc");
        if std::env::var_os("CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER").is_none()
            && musl.is_file()
        {
            std::env::set_var("CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER", musl);
        }
    }

    let qemu = which("qemu-system-x86_64")
        .or_else(|| std::env::var_os("QEMU").map(PathBuf::from))
        .context("qemu-system-x86_64 not on PATH (try: nix-shell)")?;
    let qemu_img = which("qemu-img").context("qemu-img not on PATH")?;

    let kernel = kernel.context("OATH_KERNEL")?;
    let modules = modules.context("OATH_MODULES")?;
    let busybox = busybox.context("OATH_BUSYBOX")?;
    if !kernel.is_file() {
        bail!("kernel not a file: {}", kernel.display());
    }
    if !modules.is_dir() {
        bail!("modules not a dir: {}", modules.display());
    }
    if !busybox.is_file() {
        bail!("busybox not a file: {}", busybox.display());
    }
    let btrfs = btrfs.filter(|p| p.is_file());
    let dropbear = dropbear.filter(|p| p.is_file());
    let dropbearkey = dropbearkey.filter(|p| p.is_file());
    let glibc = glibc.filter(|p| p.is_dir());
    let river = river.filter(|p| p.is_dir());
    let sola_rt = sola_rt.filter(|p| p.is_dir());
    let _ = fs::metadata(&qemu)?;
    Ok(Tools {
        kernel,
        modules,
        busybox,
        btrfs,
        dropbear,
        dropbearkey,
        glibc,
        river,
        sola_rt,
        qemu,
        qemu_img,
    })
}
