//! What `cargo make` builds images from: the native packs pinned in
//! pkgs/lock.json (built by `cargo make bootstrap` and `cargo make pkgs`),
//! the one kernel among them (`pkg:linux`), and the build host's own tools,
//! which no image carries (QEMU and its UEFI firmware: image/host.nix on a
//! Nix host, pkg:qemu on an Oath one).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::util::{prepend_path, run_out, which};

pub struct Tools {
    pub kernel: PathBuf,
    pub modules: PathBuf,
    /// Native pack trees by name (`bash` → `~/.local/share/oath/store/pkg/bash/<hash>`).
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
    let store = crate::pack::host_store_cache(root).join("pkg");
    let mut packs = BTreeMap::new();
    for (name, e) in crate::pkgs::read_lock(root)? {
        let tree = store.join(&name).join(&e.product);
        if tree.is_dir() {
            packs.insert(name, tree);
        }
    }

    // The one kernel is pkg:linux, unless OATH_KERNEL and OATH_MODULES
    // name another build.
    const BUILD_KERNEL: &str = "build the kernel first: cargo make pkgs linux";
    let linux = packs.get("linux");
    let kernel = std::env::var_os("OATH_KERNEL")
        .map(PathBuf::from)
        .or_else(|| linux.map(|p| p.join("boot/vmlinuz")))
        .context(BUILD_KERNEL)?;
    let modules = std::env::var_os("OATH_MODULES")
        .map(PathBuf::from)
        .or_else(|| linux.map(|p| p.join("lib/modules")))
        .context(BUILD_KERNEL)?;
    if !kernel.is_file() || !modules.is_dir() {
        bail!("{BUILD_KERNEL}");
    }

    // A Nix host gets QEMU and OVMF from image/host.nix; an Oath host has
    // pkg:qemu (R32) on PATH.
    let host = if which("nix-build").is_some() {
        eprintln!("loading host tools via nix-build image/host.nix ...");
        let host = PathBuf::from(run_out(
            Command::new("nix-build")
                .args([root.join("image/host.nix").to_str().unwrap(), "--no-out-link"]),
        )?);
        prepend_path(&host.join("bin"));
        Some(host)
    } else {
        None
    };
    // `cargo make esp` needs no QEMU; NixOS has no /bin/true.
    let qemu = which("qemu-system-x86_64")
        .or_else(|| which("true"))
        .unwrap_or_else(|| PathBuf::from("/bin/true"));
    let qemu_img =
        which("qemu-img").or_else(|| which("true")).unwrap_or_else(|| PathBuf::from("/bin/true"));
    // OVMF: host.nix's links, else the edk2 build in QEMU's own share/qemu.
    let share =
        qemu.canonicalize().ok().and_then(|q| q.parent()?.parent().map(|p| p.join("share/qemu")));
    let firmware = |nix: &str, qemu_file: &str| -> Option<PathBuf> {
        let p = match &host {
            Some(h) => h.join(nix),
            None => share.as_ref()?.join(qemu_file),
        };
        p.is_file().then_some(p)
    };
    Ok(Tools {
        kernel,
        modules,
        packs,
        ovmf_code: firmware("OVMF_CODE.fd", "edk2-x86_64-code.fd"),
        ovmf_vars: firmware("OVMF_VARS.fd", "edk2-i386-vars.fd"),
        qemu,
        qemu_img,
    })
}
