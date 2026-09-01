//! Pure helpers for the EFI splash: mode pick + white mark on black.

#![no_std]

const RAW: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/logo.bin"));
const MARK_NUM: u32 = 1;
const MARK_DEN: u32 = 3;

/// Preferred GOP sizes. Native for the current canto panel first.
pub const PREFER: &[(u32, u32)] =
    &[(1920, 1080), (2560, 2880), (2560, 1440), (3840, 2160), (1280, 800)];

#[derive(Clone, Copy)]
pub struct Logo {
    pub w: u32,
    pub h: u32,
    pub alpha: &'static [u8],
}

pub fn logo() -> Option<Logo> {
    if RAW.len() < 8 {
        return None;
    }
    let w = u32::from_le_bytes(RAW[0..4].try_into().ok()?);
    let h = u32::from_le_bytes(RAW[4..8].try_into().ok()?);
    let alpha = &RAW[8..];
    if w == 0 || h == 0 || alpha.len() != (w as usize) * (h as usize) {
        return None;
    }
    Some(Logo { w, h, alpha })
}

/// Pick a GOP mode. Prefer the panel's native size when the firmware lists it.
pub fn pick_mode(modes: &[(u32, u32)], current: (u32, u32)) -> (u32, u32) {
    if modes.is_empty() {
        return current;
    }
    for want in PREFER {
        if modes.iter().any(|m| m == want) {
            return *want;
        }
    }
    modes.iter().copied().max_by_key(|(w, h)| (*w as u64) * (*h as u64)).unwrap_or(current)
}

pub fn mark_size(vis_w: u32, vis_h: u32, logo: &Logo) -> u32 {
    let short = vis_w.min(vis_h);
    (short * MARK_NUM / MARK_DEN).max(1).min(short).min(logo.w.max(1))
}

pub fn sample_alpha(logo: &Logo, x: u32, y: u32, dw: u32, dh: u32) -> u8 {
    let x0 = x * logo.w / dw;
    let x1 = ((x + 1) * logo.w / dw).max(x0 + 1).min(logo.w);
    let y0 = y * logo.h / dh;
    let y1 = ((y + 1) * logo.h / dh).max(y0 + 1).min(logo.h);
    let mut sum = 0u32;
    let mut n = 0u32;
    for sy in y0..y1 {
        let row = (sy * logo.w) as usize;
        for sx in x0..x1 {
            sum += logo.alpha[row + sx as usize] as u32;
            n += 1;
        }
    }
    if n == 0 {
        0
    } else {
        (sum / n) as u8
    }
}

/// Fill `out` (len = dw*dh) with grayscale 0..=255 (white over black).
pub fn raster_mark(logo: &Logo, dw: u32, dh: u32, out: &mut [u8]) {
    for y in 0..dh {
        for x in 0..dw {
            out[(y * dw + x) as usize] = sample_alpha(logo, x, y, dw, dh);
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec;

    #[test]
    fn logo_loads() {
        let l = logo().expect("logo");
        assert!(l.w >= 256 && l.h >= 256);
        assert!(l.alpha[(l.h / 2 * l.w + l.w / 2) as usize] > 200);
        assert_eq!(l.alpha[0], 0);
    }

    #[test]
    fn prefer_1080p_when_listed() {
        let modes = [(1024, 768), (1920, 1080), (3840, 2160)];
        assert_eq!(pick_mode(&modes, (800, 600)), (1920, 1080));
    }

    #[test]
    fn largest_when_no_prefer() {
        let modes = [(800, 600), (1024, 768)];
        assert_eq!(pick_mode(&modes, (800, 600)), (1024, 768));
    }

    #[test]
    fn raster_center_white() {
        let l = logo().unwrap();
        let s = 32;
        let mut buf = vec![0u8; (s * s) as usize];
        raster_mark(&l, s, s, &mut buf);
        assert_eq!(buf[0], 0);
        assert!(buf[(s / 2 * s + s / 2) as usize] > 200);
    }
}
