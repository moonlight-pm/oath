//! Pure helpers for the EFI splash: mode pick + white mark on black.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

const RAW: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/logo.bin"));
const MARK_NUM: u32 = 1;
const MARK_DEN: u32 = 3;

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

/// Pick a GOP mode. Keep firmware's current mode when it already looks like
/// a real panel (native). Do not hardcode a distro panel size.
pub fn pick_mode(modes: &[(u32, u32)], current: (u32, u32)) -> (u32, u32) {
    if modes.is_empty() {
        return current;
    }
    let listed = modes.iter().any(|m| *m == current);
    if listed && current.0 >= 800 && current.1 >= 600 {
        return current;
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

pub fn parse_loader_conf(text: &str) -> (String, u32) {
    let mut default = String::from("oath.conf");
    let mut timeout = 0u32;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("default ") {
            default = String::from(rest.trim());
        } else if let Some(rest) = line.strip_prefix("timeout ") {
            timeout = rest.trim().parse().unwrap_or(0);
        }
    }
    (default, timeout)
}

pub fn parse_oath_boots(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        out.push(String::from(line));
    }
    out
}

pub struct Bls {
    pub title: String,
    pub linux: String,
    pub initrd: String,
    pub options: String,
}

pub fn parse_bls(text: &str) -> Bls {
    let mut e = Bls {
        title: String::from("Oath"),
        linux: String::from("\\vmlinuz"),
        initrd: String::from("\\initrd.gz"),
        options: String::new(),
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("title ") {
            e.title = String::from(rest.trim());
        } else if let Some(rest) = line.strip_prefix("linux ") {
            e.linux = efi_slash(rest.trim());
        } else if let Some(rest) = line.strip_prefix("initrd ") {
            e.initrd = efi_slash(rest.trim());
        } else if let Some(rest) = line.strip_prefix("options ") {
            e.options = String::from(rest.trim());
        }
    }
    e
}

pub fn efi_slash(p: &str) -> String {
    let p = p.trim();
    let mut s = String::from("\\");
    let mut first = true;
    for part in p.split(['/', '\\']).filter(|x| !x.is_empty()) {
        if !first {
            s.push('\\');
        }
        first = false;
        s.push_str(part);
    }
    s
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuKey {
    Up,
    Down,
    Enter,
    Esc,
}

/// `None` = cancel (Esc). `Some((idx, done))` — done means Enter.
pub fn menu_step(idx: usize, n: usize, key: MenuKey) -> Option<(usize, bool)> {
    if n == 0 {
        return None;
    }
    let last = n - 1;
    match key {
        MenuKey::Up => Some((if idx == 0 { last } else { idx - 1 }, false)),
        MenuKey::Down => Some((if idx >= last { 0 } else { idx + 1 }, false)),
        MenuKey::Enter => Some((idx.min(last), true)),
        MenuKey::Esc => None,
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
    fn keeps_firmware_native() {
        let modes = [(1024, 768), (1920, 1080), (3840, 2160)];
        assert_eq!(pick_mode(&modes, (1920, 1080)), (1920, 1080));
    }

    #[test]
    fn tiny_firmware_mode_uses_largest() {
        let modes = [(640, 480), (1920, 1080), (3840, 2160)];
        assert_eq!(pick_mode(&modes, (640, 480)), (3840, 2160));
    }

    #[test]
    fn keeps_listed_firmware_mode() {
        let modes = [(800, 600), (1024, 768)];
        assert_eq!(pick_mode(&modes, (800, 600)), (800, 600));
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

    #[test]
    fn loader_and_bls() {
        let (d, t) = parse_loader_conf("default oath.conf\ntimeout 5\n");
        assert_eq!(d, "oath.conf");
        assert_eq!(t, 5);
        let boots = parse_oath_boots("# c\noath.conf\noath-1.conf\n");
        assert_eq!(boots.len(), 2);
        let b = parse_bls("title Oath boot 1\nlinux /oath/boot/1/vmlinuz\ninitrd /oath/boot/1/initrd.gz\noptions oath.subvol=@boot-1\n");
        assert_eq!(b.linux, "\\oath\\boot\\1\\vmlinuz");
        assert!(b.options.contains("@boot-1"));
    }

    #[test]
    fn menu_wraps() {
        assert_eq!(menu_step(0, 3, MenuKey::Up), Some((2, false)));
        assert_eq!(menu_step(2, 3, MenuKey::Down), Some((0, false)));
        assert_eq!(menu_step(1, 3, MenuKey::Enter), Some((1, true)));
        assert_eq!(menu_step(0, 3, MenuKey::Esc), None);
    }
}
