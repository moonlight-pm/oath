//! What sits under the mark while Oath boots: a bar and a line or two
//! saying what the machine is doing. oath-efi draws it while it reads the
//! kernel; PID 1 draws the same place in the firmware's framebuffer
//! (`oath.fb=` on the command line) until a display driver takes the
//! screen. A bar that stops moving means the machine hung; the text says
//! where.

use alloc::string::String;
use alloc::vec::Vec;

use crate::mark_side;

const FONT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/font.bin"));
pub const GLYPH_W: u32 = 5;
pub const GLYPH_H: u32 = 9;
pub const CELL_W: u32 = GLYPH_W + 1;
pub const LINE_H: u32 = GLYPH_H + 3;
/// Lines of text under the bar; more is cut with an ellipsis.
pub const LINES: usize = 2;
/// The widest a line gets, in characters.
const COLS: u32 = 96;

/// The glyph's nine rows, its leftmost pixel in bit 4. Anything outside
/// ASCII but the ellipsis is a question mark.
pub fn glyph(c: char) -> &'static [u8] {
    let i = match c {
        ' '..='~' => c as usize - 32,
        '\u{2026}' => 95,
        _ => '?' as usize - 32,
    };
    &FONT[i * GLYPH_H as usize..(i + 1) * GLYPH_H as usize]
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub mark: Rect,
    pub bar: Rect,
    pub text: Rect,
    /// Pixels per font pixel.
    pub scale: u32,
}

/// Where the mark, the bar and the text go on a `w` by `h` screen. The
/// mark is centred; the bar and the text sit under it.
pub fn layout(w: u32, h: u32) -> Layout {
    let side = mark_side(w, h);
    let mark = Rect { x: w.saturating_sub(side) / 2, y: h.saturating_sub(side) / 2, w: side, h: side };
    let scale = ((h + 180) / 480).max(1);
    let bar_w = (side / 2).max(16).min(w);
    let bar_h = scale.max(2);
    let bar = Rect {
        x: (w - bar_w) / 2,
        y: (mark.y + side + side / 6).min(h.saturating_sub(bar_h)),
        w: bar_w,
        h: bar_h,
    };
    let margin = (2 * CELL_W * scale).min(w / 4);
    let tw = (w - 2 * margin).min(COLS * CELL_W * scale);
    let ty = (bar.y + bar_h + 8 * scale).min(h);
    let text =
        Rect { x: (w - tw) / 2, y: ty, w: tw, h: (LINES as u32 * LINE_H * scale).min(h - ty) };
    Layout { mark, bar, text, scale }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub const BLACK: Rgb = Rgb(0, 0, 0);
const TEXT: Rgb = Rgb(0xa8, 0xa8, 0xa8);
const LIT: Rgb = Rgb(0xe8, 0xe8, 0xe8);
const TRACK: Rgb = Rgb(0x2c, 0x2c, 0x2c);
const WARN: Rgb = Rgb(0xf0, 0xa0, 0x30);

/// Normal, or something went wrong (amber).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Normal,
    Warn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bar {
    /// No bar: the machine is done booting, or never shows one.
    Hidden,
    /// A known share of the work, in thousandths.
    Fill(u32),
    /// Work of unknown length; each frame moves the lit segment on.
    Sweep(u32),
}

/// Frames in one sweep of the lit segment across the bar.
pub const SWEEP_FRAMES: u32 = 48;

/// The bar's pixels, `w` by `h`, row by row.
pub fn paint_bar(bar: Bar, tone: Tone, w: u32, h: u32) -> Vec<Rgb> {
    let lit = if tone == Tone::Warn { WARN } else { LIT };
    let mut out = alloc::vec![if bar == Bar::Hidden { BLACK } else { TRACK }; (w * h) as usize];
    let (x0, x1) = match bar {
        Bar::Hidden => (0, 0),
        Bar::Fill(per_mille) => (0, (w as u64 * per_mille.min(1000) as u64 / 1000) as u32),
        Bar::Sweep(frame) => {
            // A quarter of the bar enters on the left and leaves on the right.
            let seg = (w / 4).max(1);
            let pos = (frame % SWEEP_FRAMES) as u64 * (w + seg) as u64 / SWEEP_FRAMES as u64;
            let start = pos as i64 - seg as i64;
            (start.max(0) as u32, ((start + seg as i64).max(0) as u32).min(w))
        }
    };
    for y in 0..h {
        for x in x0..x1 {
            out[(y * w + x) as usize] = lit;
        }
    }
    out
}

/// `text`, wrapped to `LINES` lines of the area's width and centred, as
/// the area's pixels, `area.w` by `area.h`, row by row.
pub fn paint_text(text: &str, tone: Tone, area: Rect, scale: u32) -> Vec<Rgb> {
    let ink = if tone == Tone::Warn { WARN } else { TEXT };
    let mut out = alloc::vec![BLACK; (area.w * area.h) as usize];
    let cols = (area.w / (CELL_W * scale)).max(1) as usize;
    for (n, line) in wrap(text, cols).iter().enumerate() {
        let len = line.chars().count() as u32;
        let width = (len * CELL_W).saturating_sub(1) * scale;
        let x0 = area.w.saturating_sub(width) / 2;
        let y0 = n as u32 * LINE_H * scale;
        for (i, c) in line.chars().enumerate() {
            let gx = x0 + i as u32 * CELL_W * scale;
            for (r, bits) in glyph(c).iter().enumerate() {
                for col in 0..GLYPH_W {
                    if bits >> (GLYPH_W - 1 - col) & 1 == 0 {
                        continue;
                    }
                    for dy in 0..scale {
                        let y = y0 + r as u32 * scale + dy;
                        if y >= area.h {
                            continue;
                        }
                        for dx in 0..scale {
                            let x = gx + col * scale + dx;
                            if x < area.w {
                                out[(y * area.w + x) as usize] = ink;
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

/// Words onto at most `LINES` lines of `cols` characters; a word longer
/// than a line is broken, and what does not fit ends in an ellipsis.
pub fn wrap(text: &str, cols: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cut = false;
    for word in text.split_whitespace() {
        let mut word: Vec<char> = word.chars().collect();
        loop {
            let used = cur.chars().count();
            let room = if used == 0 { cols } else { cols.saturating_sub(used + 1) };
            if word.len() <= room {
                if used > 0 {
                    cur.push(' ');
                }
                cur.extend(word.iter());
                break;
            }
            if used == 0 {
                // Longer than a whole line: break it.
                cur.extend(word.drain(..cols));
            }
            lines.push(core::mem::take(&mut cur));
            if lines.len() == LINES {
                cut = true;
                break;
            }
        }
        if cut {
            break;
        }
    }
    if !cut && !cur.is_empty() {
        lines.push(cur);
    }
    if cut {
        if let Some(last) = lines.last_mut() {
            let mut chars: Vec<char> = last.chars().collect();
            chars.truncate(cols.saturating_sub(1));
            chars.push('\u{2026}');
            *last = chars.into_iter().collect();
        }
    }
    lines
}

/// A pixel as a 32-bit framebuffer word: the firmware's `Rgb` order
/// (red in the lowest byte) or `Bgr` (blue there).
pub fn word(p: Rgb, bgr: bool) -> u32 {
    let (lo, hi) = if bgr { (p.2, p.0) } else { (p.0, p.2) };
    lo as u32 | (p.1 as u32) << 8 | (hi as u32) << 16
}

/// The framebuffer oath-efi hands PID 1: `<base>,<w>,<h>,<stride>,<rgb|bgr>`,
/// the base in hex, the stride in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fb {
    pub base: u64,
    pub w: u32,
    pub h: u32,
    pub stride: u32,
    pub bgr: bool,
}

impl Fb {
    pub fn parse(s: &str) -> Option<Fb> {
        let mut it = s.split(',');
        let base = u64::from_str_radix(it.next()?.trim_start_matches("0x"), 16).ok()?;
        let w = it.next()?.parse().ok()?;
        let h = it.next()?.parse().ok()?;
        let stride: u32 = it.next()?.parse().ok()?;
        let bgr = match it.next()? {
            "bgr" => true,
            "rgb" => false,
            _ => return None,
        };
        if it.next().is_some() || base == 0 || w == 0 || h == 0 || stride < w {
            return None;
        }
        Some(Fb { base, w, h, stride, bgr })
    }

    pub fn arg(&self) -> String {
        alloc::format!(
            "oath.fb=0x{:x},{},{},{},{}",
            self.base,
            self.w,
            self.h,
            self.stride,
            if self.bgr { "bgr" } else { "rgb" }
        )
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn glyphs_from_the_sheet() {
        assert_eq!(FONT.len(), 96 * GLYPH_H as usize);
        assert!(glyph(' ').iter().all(|r| *r == 0));
        assert_eq!(&glyph('A')[..7], &[0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001]);
        // g descends; anything unknown is a question mark.
        assert_ne!(glyph('g')[8], 0);
        assert_eq!(glyph('\u{e9}'), glyph('?'));
        assert_eq!(glyph('\u{2026}')[6], 0b10101);
    }

    #[test]
    fn layout_stays_on_screen() {
        for (w, h) in [(640, 480), (1280, 800), (1920, 1080), (3840, 2160), (1080, 1920)] {
            let l = layout(w, h);
            for r in [l.mark, l.bar, l.text] {
                assert!(r.x + r.w <= w && r.y + r.h <= h, "{w}x{h}: {r:?}");
            }
            assert!(l.bar.y >= l.mark.y + l.mark.h);
            assert!(l.text.y >= l.bar.y + l.bar.h);
            assert!(l.text.h >= LINE_H * l.scale, "{w}x{h}: one line at least");
        }
        assert_eq!(layout(1920, 1080).scale, 2);
        assert_eq!(layout(3840, 2160).scale, 4);
    }

    #[test]
    fn wraps_and_cuts() {
        assert_eq!(wrap("Loading drivers", 40), ["Loading drivers"]);
        assert_eq!(wrap("one two three", 7), ["one two", "three"]);
        assert_eq!(wrap("aaaaaaaaaa", 4), ["aaaa", "aaa\u{2026}"]);
        assert_eq!(wrap("one two three four five", 9), ["one two", "three\u{2026}"]);
        assert!(wrap("", 10).is_empty());
    }

    #[test]
    fn bar_fill_and_sweep() {
        let full = paint_bar(Bar::Fill(1000), Tone::Normal, 40, 2);
        assert!(full.iter().all(|p| *p == LIT));
        let half = paint_bar(Bar::Fill(500), Tone::Warn, 40, 2);
        assert_eq!((half[19], half[20]), (WARN, TRACK));
        assert!(paint_bar(Bar::Hidden, Tone::Normal, 40, 2).iter().all(|p| *p == BLACK));
        // The segment enters from the left edge and moves right.
        let lit = |f| paint_bar(Bar::Sweep(f), Tone::Normal, 40, 1).iter().position(|p| *p == LIT);
        assert_eq!(lit(0), None);
        assert!(lit(SWEEP_FRAMES / 2).unwrap() > lit(SWEEP_FRAMES / 4).unwrap());
    }

    #[test]
    fn text_is_centred_ink() {
        let area = Rect { x: 0, y: 0, w: 60, h: 2 * LINE_H };
        let px = paint_text("I", Tone::Normal, area, 1);
        // 'I' is .###. : its top row is three pixels wide, centred.
        let row: std::vec::Vec<bool> = px[..60].iter().map(|p| *p == TEXT).collect();
        let first = row.iter().position(|b| *b).unwrap();
        assert_eq!(row.iter().filter(|b| **b).count(), 3);
        assert_eq!(first, (60 - 5) / 2 + 1);
    }

    #[test]
    fn framebuffer_arg_round_trips() {
        let fb = Fb { base: 0x4000_0000, w: 1920, h: 1080, stride: 1920, bgr: true };
        let arg = fb.arg();
        assert_eq!(arg, "oath.fb=0x40000000,1920,1080,1920,bgr");
        assert_eq!(Fb::parse(arg.strip_prefix("oath.fb=").unwrap()), Some(fb));
        assert_eq!(Fb::parse("0x0,1,1,1,bgr"), None);
        assert_eq!(Fb::parse("0x1000,1920,1080,1900,rgb"), None);
        assert_eq!(word(Rgb(1, 2, 3), true), 0x010203);
        assert_eq!(word(Rgb(1, 2, 3), false), 0x030201);
    }
}
