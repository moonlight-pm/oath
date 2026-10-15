//! The one kernel: vanilla kernel.org source, pinned by sha256 and
//! built by its plan (`pkgs/linux`, `pkg:linux`). Pack refuses a bzImage
//! whose release does not match the module tree it ships with.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

/// Module every image needs; its vermagic is checked against the bzImage.
const VERMAGIC_PROBE: &str = "kernel/fs/btrfs/btrfs.ko";

/// Release (`7.3.0-rc1`) from an x86 bzImage. The boot protocol header
/// has `HdrS` at 0x202 and, at 0x20E, the offset (from 0x200) of a
/// NUL-terminated version string whose first word is the release.
pub fn bzimage_release(image: &[u8]) -> Option<String> {
    if image.len() < 0x210 || &image[0x202..0x206] != b"HdrS" {
        return None;
    }
    let off = u16::from_le_bytes([image[0x20E], image[0x20F]]) as usize + 0x200;
    let rest = image.get(off..)?;
    let end = rest.iter().position(|&b| b == 0)?;
    let text = std::str::from_utf8(&rest[..end]).ok()?;
    text.split_whitespace().next().map(str::to_string)
}

/// First word of the `vermagic=` modinfo string in a module.
pub fn module_vermagic(module: &[u8]) -> Option<String> {
    let key = b"vermagic=";
    let at = module.windows(key.len()).position(|w| w == key)? + key.len();
    let rest = &module[at..];
    let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
    let text = std::str::from_utf8(&rest[..end]).ok()?;
    text.split_whitespace().next().map(str::to_string)
}

/// Check `kernel` against `modules/<release>` and return the release.
pub fn check(kernel: &Path, modules: &Path) -> Result<String> {
    let image = fs::read(kernel).with_context(|| format!("read {}", kernel.display()))?;
    let Some(release) = bzimage_release(&image) else {
        bail!("{} is not an x86 bzImage", kernel.display());
    };
    let kdir = modules.join(&release);
    if !kdir.is_dir() {
        bail!(
            "kernel {} is {release} but {} has no {release}/ — rebuild with `cargo make pkgs linux`",
            kernel.display(),
            modules.display()
        );
    }
    let Some(probe) = crate::pack::find_module(&kdir, VERMAGIC_PROBE) else {
        bail!("{} has no {VERMAGIC_PROBE}", kdir.display());
    };
    let bytes = crate::pack::decompress_module(&probe)?;
    match module_vermagic(&bytes) {
        Some(v) if v == release => Ok(release),
        Some(v) => bail!("kernel is {release} but {} has vermagic {v}", probe.display()),
        None => bail!("{} has no vermagic", probe.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_bzimage(version: &str) -> Vec<u8> {
        let mut b = vec![0u8; 0x400];
        b[0x202..0x206].copy_from_slice(b"HdrS");
        let off: u16 = 0x100;
        b[0x20E..0x210].copy_from_slice(&off.to_le_bytes());
        let at = 0x200 + off as usize;
        b[at..at + version.len()].copy_from_slice(version.as_bytes());
        b
    }

    #[test]
    fn reads_release_from_header() {
        let b = fake_bzimage("7.3.0-rc1 (oath@novus) #1 SMP PREEMPT_DYNAMIC");
        assert_eq!(bzimage_release(&b).as_deref(), Some("7.3.0-rc1"));
        assert_eq!(bzimage_release(&[0u8; 16]), None);
    }

    #[test]
    fn reads_vermagic() {
        let m = b"\x7fELF....license=GPL\0vermagic=7.1.2 SMP preempt mod_unload \0name=btrfs\0";
        assert_eq!(module_vermagic(m).as_deref(), Some("7.1.2"));
        assert_eq!(module_vermagic(b"no magic here"), None);
    }

    #[test]
    fn mismatched_tree_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let k = d.path().join("vmlinuz");
        fs::write(&k, fake_bzimage("7.1.2 (nixbld@localhost) #1-NixOS")).unwrap();
        let mods = d.path().join("modules");
        fs::create_dir_all(mods.join("7.3.0-rc1/kernel/fs/btrfs")).unwrap();
        let err = check(&k, &mods).unwrap_err().to_string();
        assert!(err.contains("7.1.2"), "{err}");
        fs::create_dir_all(mods.join("7.1.2/kernel/fs/btrfs")).unwrap();
        fs::write(mods.join("7.1.2/kernel/fs/btrfs/btrfs.ko"), b"vermagic=7.3.0-rc1 SMP\0")
            .unwrap();
        let err = check(&k, &mods).unwrap_err().to_string();
        assert!(err.contains("vermagic 7.3.0-rc1"), "{err}");
        fs::write(mods.join("7.1.2/kernel/fs/btrfs/btrfs.ko"), b"vermagic=7.1.2 SMP\0").unwrap();
        assert_eq!(check(&k, &mods).unwrap(), "7.1.2");
    }
}
