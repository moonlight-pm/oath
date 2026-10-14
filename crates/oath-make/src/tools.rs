//! What `cargo make` builds images from: the one kernel (image/build-linux.sh),
//! the native packs pinned in pkgs/lock.json (built by `cargo make bootstrap`
//! and `cargo make pkgs`), and build-host tools that never ship (QEMU and
//! its UEFI firmware, from image/host.nix).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::util::{prepend_path, run_out, which};

pub struct Tools {
    pub kernel: PathBuf,
    pub modules: PathBuf,
    /// Native pack trees by name (`busybox` → `.cache/oath/store/pkg/busybox/<hash>`).
    pub packs: BTreeMap<String, PathBuf>,
    pub ovmf_code: Option<PathBuf>,
    pub ovmf_vars: Option<PathBuf>,
    pub qemu: PathBuf,
    pub qemu_img: PathBuf,
}

impl Tools {
    /// A native pack's tree, or how to build it.
    pub fn pack(&self, name: &str) -> Result<&Path> {
        self.packs.get(name).map(PathBuf::as_path).with_context(|| {
            format!("no native pkg:{name} — run `cargo make bootstrap` then `cargo make pkgs`")
        })
    }

    /// `bin/<file>` inside a native pack.
    pub fn bin(&self, pack: &str, file: &str) -> Result<PathBuf> {
        let p = self.pack(pack)?.join("bin").join(file);
        if !p.is_file() {
            bail!("pkg:{pack} has no bin/{file}");
        }
        Ok(p)
    }

    /// Firmware for the initrd (`pkg:firmware`'s `lib/firmware`).
    pub fn firmware(&self) -> Option<PathBuf> {
        let p = self.packs.get("firmware")?.join("lib/firmware");
        p.is_dir().then_some(p)
    }
}

pub fn load(root: &Path) -> Result<Tools> {
    const BUILD_KERNEL: &str =
        "build the kernel first: nix-shell image/linux-build.nix --run 'sh image/build-linux.sh'";
    // The one kernel is our own build (image/build-linux.sh).
    let own = root.join("build/linux");
    let kernel = std::env::var_os("OATH_KERNEL")
        .map(PathBuf::from)
        .or_else(|| Some(own.join("vmlinuz")).filter(|p| p.is_file()))
        .context(BUILD_KERNEL)?;
    let modules = std::env::var_os("OATH_MODULES")
        .map(PathBuf::from)
        .or_else(|| Some(own.join("modules")).filter(|p| p.is_dir()))
        .context(BUILD_KERNEL)?;
    if !kernel.is_file() || !modules.is_dir() {
        bail!("{BUILD_KERNEL}");
    }

    let store = crate::pack::host_store_cache(root).join("pkg");
    let mut packs = BTreeMap::new();
    for (name, e) in crate::pkgs::read_lock(root)? {
        let tree = store.join(&name).join(&e.product);
        if tree.is_dir() {
            packs.insert(name, tree);
        }
    }

    eprintln!("loading host tools via nix-build image/host.nix ...");
    let host = PathBuf::from(run_out(
        Command::new("nix-build")
            .args([root.join("image/host.nix").to_str().unwrap(), "--no-out-link"]),
    )?);
    prepend_path(&host.join("bin"));
    let opt = |name: &str| -> Option<PathBuf> {
        let p = host.join(name);
        p.is_file().then_some(p)
    };
    // `cargo make boot` / `esp` need no QEMU; NixOS has no /bin/true.
    let qemu = which("qemu-system-x86_64")
        .or_else(|| which("true"))
        .unwrap_or_else(|| PathBuf::from("/bin/true"));
    let qemu_img =
        which("qemu-img").or_else(|| which("true")).unwrap_or_else(|| PathBuf::from("/bin/true"));
    Ok(Tools {
        kernel,
        modules,
        packs,
        ovmf_code: opt("OVMF_CODE.fd"),
        ovmf_vars: opt("OVMF_VARS.fd"),
        qemu,
        qemu_img,
    })
}
