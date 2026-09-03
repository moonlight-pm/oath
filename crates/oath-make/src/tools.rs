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
    pub mkfs_btrfs: Option<PathBuf>,
    pub dropbear: Option<PathBuf>,
    pub dropbearkey: Option<PathBuf>,
    pub dropbear_scp: Option<PathBuf>,
    pub sftp_server: Option<PathBuf>,
    pub sgdisk: Option<PathBuf>,
    pub mkfs_fat: Option<PathBuf>,
    pub kexec: Option<PathBuf>,
    pub gnutar: Option<PathBuf>,
    pub systemd_boot: Option<PathBuf>,
    pub ovmf_code: Option<PathBuf>,
    pub ovmf_vars: Option<PathBuf>,
    pub firmware: Option<PathBuf>,
    pub glibc: Option<PathBuf>,
    pub river: Option<PathBuf>,
    pub sola_rt: Option<PathBuf>,
    pub git: Option<PathBuf>,
    pub curl: Option<PathBuf>,
    pub cacert: Option<PathBuf>,
    pub pipewire: Option<PathBuf>,
    pub wireplumber: Option<PathBuf>,
    pub alsa_lib: Option<PathBuf>,
    pub libpulse: Option<PathBuf>,
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
    let mut dropbear_scp = std::env::var_os("OATH_DROPBEAR_SCP").map(PathBuf::from);
    let mut sftp_server = std::env::var_os("OATH_SFTP_SERVER").map(PathBuf::from);
    let mut glibc = std::env::var_os("OATH_GLIBC").map(PathBuf::from);
    let mut river = std::env::var_os("OATH_RIVER").map(PathBuf::from);
    let mut sola_rt = std::env::var_os("OATH_SOLA_RT").map(PathBuf::from);
    let mut git = std::env::var_os("OATH_GIT").map(PathBuf::from);
    let mut curl = std::env::var_os("OATH_CURL").map(PathBuf::from);
    let mut cacert = std::env::var_os("OATH_CACERT").map(PathBuf::from);

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
        dropbear_scp = dropbear_scp.or_else(|| {
            let p = tools.join("dropbear-scp");
            p.is_file().then_some(p)
        });
        sftp_server = sftp_server.or_else(|| {
            let p = tools.join("sftp-server");
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
        git = git.or_else(|| {
            let p = tools.join("git");
            p.is_dir().then_some(p)
        });
        curl = curl.or_else(|| {
            let p = tools.join("curl");
            p.is_file().then_some(p)
        });
        cacert = cacert.or_else(|| {
            let p = tools.join("ca-bundle.crt");
            p.is_file().then_some(p)
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
    let dropbear_scp = dropbear_scp.filter(|p| p.is_file()).or_else(|| {
        dropbear.as_ref().and_then(|d| {
            let p = d.parent()?.join("dropbear-scp");
            p.is_file().then_some(p)
        })
    });
    let sftp_server = sftp_server.filter(|p| p.is_file()).or_else(|| {
        dropbear.as_ref().and_then(|d| {
            let p = d.parent()?.join("sftp-server");
            p.is_file().then_some(p)
        })
    });
    let glibc = glibc.filter(|p| p.is_dir());
    let river = river.filter(|p| p.is_dir());
    let sola_rt = sola_rt.filter(|p| p.is_dir());
    let git = git.filter(|p| p.is_dir());
    let curl = curl.filter(|p| p.is_file());
    let cacert = cacert.filter(|p| p.is_file());
    let tools_dir = kernel.parent().map(|p| p.to_path_buf());
    let opt_file = |name: &str| -> Option<PathBuf> {
        let p = tools_dir.as_ref()?.join(name);
        p.is_file().then_some(p)
    };
    let opt_dir = |name: &str| -> Option<PathBuf> {
        let p = tools_dir.as_ref()?.join(name);
        p.is_dir().then_some(p)
    };
    let _ = fs::metadata(&qemu)?;
    Ok(Tools {
        kernel,
        modules,
        busybox,
        btrfs,
        mkfs_btrfs: opt_file("mkfs.btrfs"),
        dropbear,
        dropbearkey,
        dropbear_scp,
        sftp_server,
        sgdisk: opt_file("sgdisk"),
        mkfs_fat: opt_file("mkfs.fat"),
        kexec: opt_file("kexec"),
        gnutar: opt_file("gnutar"),
        systemd_boot: opt_file("systemd-bootx64.efi"),
        ovmf_code: opt_file("OVMF_CODE.fd"),
        ovmf_vars: opt_file("OVMF_VARS.fd"),
        firmware: opt_dir("firmware"),
        glibc,
        river,
        sola_rt,
        git,
        curl,
        cacert,
        pipewire: opt_dir("pipewire").or_else(|| {
            std::env::var_os("OATH_PIPEWIRE").map(PathBuf::from).filter(|p| p.is_dir())
        }),
        wireplumber: opt_dir("wireplumber").or_else(|| {
            std::env::var_os("OATH_WIREPLUMBER").map(PathBuf::from).filter(|p| p.is_dir())
        }),
        alsa_lib: opt_dir("alsa-lib").or_else(|| {
            std::env::var_os("OATH_ALSA_LIB").map(PathBuf::from).filter(|p| p.is_dir())
        }),
        libpulse: opt_dir("libpulseaudio").or_else(|| {
            std::env::var_os("OATH_LIBPULSEAUDIO").map(PathBuf::from).filter(|p| p.is_dir())
        }),
        qemu,
        qemu_img,
    })
}
