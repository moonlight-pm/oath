//! Connected-GPU facts for package apply.
//!
//! Gamescope's Wayland nest needs Vulkan WSI **DRM format modifiers**.
//! RADV on AMD GFX6–8 advertises none; virtio-gpu neither. Session
//! Steam on host Xwayland does not need this.

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};

thread_local! {
    static OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
}

/// PCI vendor: AMD.
pub const VENDOR_AMD: u16 = 0x1002;
/// PCI vendor: NVIDIA.
pub const VENDOR_NVIDIA: u16 = 0x10de;
/// PCI vendor: Intel.
pub const VENDOR_INTEL: u16 = 0x8086;
/// PCI vendor: virtio (QEMU).
pub const VENDOR_VIRTIO: u16 = 0x1af4;
/// Canto Pitcairn.
#[cfg(test)]
const DEVICE_PITCAIRN: u16 = 0x6810;

/// Run `f` with a forced probe result. Serializes against other
/// override tests (the live `/sys` probe is used when unset).
pub fn with_drm_modifiers_override<R>(value: bool, f: impl FnOnce() -> R) -> R {
    OVERRIDE.with(|c| {
        let prev = c.get();
        c.set(Some(value));
        let out = f();
        c.set(prev);
        out
    })
}

/// True if every *connected* display GPU can export dmabufs with
/// format modifiers (the gamescope WSI requirement).
pub fn drm_modifiers_available() -> bool {
    if let Some(v) = OVERRIDE.with(|c| c.get()) {
        return v;
    }
    let ids = connected_pci_ids();
    if ids.is_empty() {
        return false;
    }
    ids.iter().all(|(v, d)| pci_has_drm_modifiers(*v, *d))
}

/// Whether this PCI id is known to have Vulkan WSI DRM modifiers.
pub fn pci_has_drm_modifiers(vendor: u16, device: u16) -> bool {
    match vendor {
        VENDOR_VIRTIO => false,
        VENDOR_NVIDIA | VENDOR_INTEL => true,
        VENDOR_AMD => !amd_gfx6_through_8(device),
        _ => true,
    }
}

/// AMD Southern Islands / Sea Islands / GFX8. RADV reports no
/// modifiers on these (T37 canto nest).
fn amd_gfx6_through_8(dev: u16) -> bool {
    matches!(
        dev,
        0x1304..=0x131D // Kaveri (GFX7)
            | 0x6600..=0x667F // Oland / Bonaire / Hainan
            | 0x6780..=0x683F // Tahiti / Pitcairn / Verde / Hawaii / Polaris 10/11
            | 0x6900..=0x694F // Topaz / Tonga / Vega M
            | 0x6980..=0x699F // Polaris 12
            | 0x7300..=0x730F // Fiji
            | 0x9830..=0x983F // Kabini
            | 0x9850..=0x985F // Mullins
            | 0x9870..=0x9877 // Carrizo
            | 0x98E4 // Stoney
    )
}

fn connected_pci_ids() -> Vec<(u16, u16)> {
    let drm = Path::new("/sys/class/drm");
    let Ok(entries) = fs::read_dir(drm) else {
        return Vec::new();
    };
    let mut cards: Vec<PathBuf> = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if !is_card_name(&name) {
            continue;
        }
        cards.push(e.path());
    }
    cards.sort();
    let mut connected = Vec::new();
    for card in &cards {
        let name = card.file_name().unwrap().to_string_lossy();
        if !card_has_connected_connector(card, &name) {
            continue;
        }
        if let Some(id) = pci_id_for_card(card) {
            connected.push(id);
        }
    }
    if !connected.is_empty() {
        return connected;
    }
    // Headless / KMS not up: any card still counts (fail closed on SI).
    cards.iter().filter_map(|c| pci_id_for_card(c)).collect()
}

fn is_card_name(name: &str) -> bool {
    let rest = match name.strip_prefix("card") {
        Some(r) => r,
        None => return false,
    };
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
}

fn card_has_connected_connector(card: &Path, name: &str) -> bool {
    let Ok(rd) = fs::read_dir(card) else {
        return false;
    };
    let prefix = format!("{name}-");
    for e in rd.flatten() {
        let n = e.file_name();
        let n = n.to_string_lossy();
        if !n.starts_with(&prefix) {
            continue;
        }
        let status = fs::read_to_string(e.path().join("status")).unwrap_or_default();
        if status.trim() == "connected" {
            return true;
        }
    }
    false
}

fn pci_id_for_card(card: &Path) -> Option<(u16, u16)> {
    let dev = card.join("device");
    let vendor = read_hex(&dev.join("vendor"))?;
    let device = read_hex(&dev.join("device"))?;
    Some((vendor, device))
}

fn read_hex(path: &Path) -> Option<u16> {
    let s = fs::read_to_string(path).ok()?;
    let s = s.trim().trim_start_matches("0x");
    u16::from_str_radix(s, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pitcairn_has_no_modifiers() {
        assert!(!pci_has_drm_modifiers(VENDOR_AMD, DEVICE_PITCAIRN));
        assert!(!pci_has_drm_modifiers(VENDOR_AMD, 0x6800));
        assert!(!pci_has_drm_modifiers(VENDOR_AMD, 0x6798));
    }

    #[test]
    fn polaris_fiji_have_no_modifiers() {
        assert!(!pci_has_drm_modifiers(VENDOR_AMD, 0x67DF));
        assert!(!pci_has_drm_modifiers(VENDOR_AMD, 0x7300));
    }

    #[test]
    fn navi_and_nvidia_have_modifiers() {
        assert!(pci_has_drm_modifiers(VENDOR_AMD, 0x73FF));
        assert!(pci_has_drm_modifiers(VENDOR_NVIDIA, 0x2203));
        assert!(pci_has_drm_modifiers(VENDOR_INTEL, 0x9A49));
    }

    #[test]
    fn virtio_has_no_modifiers() {
        assert!(!pci_has_drm_modifiers(VENDOR_VIRTIO, 0x1050));
    }

    #[test]
    fn override_wins() {
        with_drm_modifiers_override(false, || {
            assert!(!drm_modifiers_available());
        });
        with_drm_modifiers_override(true, || {
            assert!(drm_modifiers_available());
        });
    }
}
