//! Firmware-framebuffer hold vs KMS takeover.
//!
//! Not every machine has EFI GOP or a driver for its card. While the
//! firmware's framebuffer shows the boot mark, defer KMS drivers that
//! would kick it until just before the compositor. virtio-gpu is the QEMU
//! display — never defer it. When none takes the screen, efidrm does.

use std::fs;
use std::path::Path;

/// Where sysfb leaves the firmware's framebuffer for efidrm, until a
/// card's driver takes it (R89).
pub const FIRMWARE_FB: &str = "/sys/bus/platform/devices/efi-framebuffer.0";

/// A DRM device is there: a driver has the screen.
pub fn has_drm_card() -> bool {
    fs::read_dir("/sys/class/drm").into_iter().flatten().flatten().any(|e| {
        let n = e.file_name();
        let n = n.to_string_lossy();
        n.starts_with("card") && !n.contains('-')
    })
}

/// Whether efidrm should take the firmware's framebuffer: no driver has
/// the screen, and the firmware left one.
pub fn wants_firmware_fb() -> bool {
    !has_drm_card() && Path::new(FIRMWARE_FB).exists()
}

/// Module paths (under `kernel/drivers/...`) that replace the firmware fb,
/// and NVIDIA's modules, which a package brings (R34).
pub fn takes_over_firmware_fb(rel: &str) -> bool {
    let r = rel.replace('\\', "/");
    r.contains("/amd/amdgpu/")
        || r.contains("/gpu/drm/i915/")
        || r.contains("/gpu/drm/xe/")
        || r.contains("/gpu/drm/nouveau/")
        || r.contains("/gpu/drm/radeon/")
        || r.rsplit('/').next().is_some_and(|f| f.starts_with("nvidia"))
}

/// A module that drives a display: any DRM or fbdev driver, or NVIDIA's.
/// The boot splash lets go of the firmware's framebuffer before one loads.
pub fn drives_display(rel: &str) -> bool {
    let r = rel.replace('\\', "/");
    takes_over_firmware_fb(&r) || r.contains("/gpu/drm/") || r.contains("/video/fbdev/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defers_discrete_kms_not_virtio() {
        assert!(takes_over_firmware_fb("kernel/drivers/gpu/drm/amd/amdgpu/amdgpu.ko"));
        assert!(takes_over_firmware_fb("kernel/drivers/gpu/drm/i915/i915.ko"));
        assert!(takes_over_firmware_fb("kernel/drivers/gpu/drm/nouveau/nouveau.ko"));
        assert!(takes_over_firmware_fb(
            "/oath/store/pkg/nvidia/sha256-0/lib/modules/7/nvidia-drm.ko"
        ));
        assert!(!takes_over_firmware_fb("kernel/drivers/net/ethernet/nvidia/forcedeth.ko"));
        assert!(!takes_over_firmware_fb("kernel/drivers/gpu/drm/virtio/virtio-gpu.ko"));
        assert!(!takes_over_firmware_fb("kernel/drivers/net/ethernet/broadcom/tg3.ko"));
        assert!(!takes_over_firmware_fb("kernel/fs/btrfs/btrfs.ko"));
    }

    #[test]
    fn any_display_driver_ends_the_splash() {
        assert!(drives_display("kernel/drivers/gpu/drm/virtio/virtio-gpu.ko"));
        assert!(drives_display("kernel/drivers/gpu/drm/drm.ko"));
        assert!(drives_display("kernel/drivers/video/fbdev/core/fb.ko"));
        assert!(drives_display("/oath/store/pkg/nvidia/sha256-0/lib/modules/7/nvidia.ko"));
        // The fallback ends the splash too, and never waits as a card's does.
        assert!(drives_display("kernel/drivers/gpu/drm/sysfb/efidrm.ko"));
        assert!(!takes_over_firmware_fb("kernel/drivers/gpu/drm/sysfb/efidrm.ko"));
        assert!(!drives_display("kernel/drivers/usb/host/xhci-hcd.ko"));
    }
}
