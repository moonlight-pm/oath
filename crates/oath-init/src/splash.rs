//! White mark on black on the EFI/simpledrm framebuffer.
//! Never paint `amdgpudrmfb` — that wedged Pitcairn. Logs stay on serial.

use std::fs::{self, OpenOptions};
use std::io::Cursor;
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::log;

const LOGO_PNG: &[u8] = include_bytes!("../../../brand/logo-white.png");
/// Mark is about a third of the short edge — same weight as the source square.
const MARK_NUM: u32 = 1;
const MARK_DEN: u32 = 3;
const FBIOGET_VSCREENINFO: libc::Ioctl = 0x4600 as libc::Ioctl;
const FBIOGET_FSCREENINFO: libc::Ioctl = 0x4602 as libc::Ioctl;
const KDSETMODE: libc::Ioctl = 0x4B3A as libc::Ioctl;
const KD_GRAPHICS: libc::c_int = 1;

static LOGO: OnceLock<Logo> = OnceLock::new();

struct Logo {
    w: u32,
    h: u32,
    alpha: Vec<u8>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FbBitfield {
    offset: u32,
    length: u32,
    msb_right: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FbVarScreeninfo {
    xres: u32,
    yres: u32,
    xres_virtual: u32,
    yres_virtual: u32,
    xoffset: u32,
    yoffset: u32,
    bits_per_pixel: u32,
    grayscale: u32,
    red: FbBitfield,
    green: FbBitfield,
    blue: FbBitfield,
    transp: FbBitfield,
    nonstd: u32,
    activate: u32,
    height: u32,
    width: u32,
    accel_flags: u32,
    pixclock: u32,
    left_margin: u32,
    right_margin: u32,
    upper_margin: u32,
    lower_margin: u32,
    hsync_len: u32,
    vsync_len: u32,
    sync: u32,
    vmode: u32,
    rotate: u32,
    colorspace: u32,
    reserved: [u32; 4],
}

#[repr(C)]
struct FbFixScreeninfo {
    id: [u8; 16],
    smem_start: libc::c_ulong,
    smem_len: u32,
    type_: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    line_length: u32,
    mmio_start: libc::c_ulong,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}

struct Map {
    ptr: *mut u8,
    len: usize,
}

impl Drop for Map {
    fn drop(&mut self) {
        if !self.ptr.is_null() && self.len > 0 {
            unsafe {
                libc::munmap(self.ptr.cast(), self.len);
            }
        }
    }
}

/// Paint immediately if a framebuffer already exists (EFI GOP / simpledrm).
pub fn show_now() {
    start(Duration::ZERO);
}

/// After GPU modules: wait for `/dev/fb0` then paint.
pub fn show_wait() {
    start(Duration::from_secs(8));
}

/// Keep fbcon from drawing text over the mark.
pub fn hold() {
    quiet_console();
    graphics_mode();
}

fn start(wait: Duration) {
    hold();
    if crate::cmdline_flag("oath.nosplash") {
        return;
    }
    // Never block PID 1 on a GPU ioctl. The child is reaped in the main loop.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        show_inner(wait);
        unsafe { libc::_exit(0) };
    }
    if pid < 0 {
        show_inner(wait);
    }
}

fn show_inner(wait: Duration) {
    wait_fb(wait);
    let logo = LOGO.get_or_init(decode_logo);
    if logo.w == 0 || logo.h == 0 {
        log("splash: no logo");
        return;
    }
    let mut painted = 0u32;
    for n in 0..8 {
        let path = format!("/dev/fb{n}");
        if !Path::new(&path).exists() {
            continue;
        }
        if fb_is_amdgpu(&path) {
            log(&format!("splash skip amdgpu {path}"));
            continue;
        }
        match paint_fb(&path, logo) {
            Ok((w, h)) => {
                oath_core::tel("init", "splash", serde_json::json!({ "fb": path, "w": w, "h": h }));
                painted += 1;
            }
            Err(e) => log(&format!("splash {path}: {e}")),
        }
    }
    if painted == 0 {
        log("splash: no framebuffer");
    }
}

fn fb_is_amdgpu(dev: &str) -> bool {
    let Some(base) = Path::new(dev).file_name() else {
        return false;
    };
    let name = fs::read_to_string(Path::new("/sys/class/graphics").join(base).join("name"))
        .unwrap_or_default();
    name.to_ascii_lowercase().contains("amdgpu")
}

fn quiet_console() {
    let _ = fs::write("/proc/sys/kernel/printk", "0 0 0 0\n");
    let _ = fs::write("/sys/class/graphics/fbcon/cursor_blink", b"0");
}

fn graphics_mode() {
    unsafe {
        let fd = libc::open(c"/dev/tty0".as_ptr(), libc::O_RDWR);
        if fd >= 0 {
            libc::ioctl(fd, KDSETMODE, KD_GRAPHICS);
            libc::close(fd);
        }
        let fd = libc::open(c"/dev/tty1".as_ptr(), libc::O_RDWR);
        if fd >= 0 {
            libc::ioctl(fd, KDSETMODE, KD_GRAPHICS);
            libc::close(fd);
        }
    }
}

fn wait_fb(wait: Duration) {
    if wait.is_zero() || Path::new("/dev/fb0").exists() {
        return;
    }
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        if Path::new("/dev/fb0").exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn decode_logo() -> Logo {
    match decode_logo_png() {
        Ok(l) => l,
        Err(e) => {
            log(&format!("splash logo: {e}"));
            Logo { w: 0, h: 0, alpha: Vec::new() }
        }
    }
}

fn decode_logo_png() -> Result<Logo, String> {
    let mut decoder = png::Decoder::new(Cursor::new(LOGO_PNG));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let w = info.width;
    let h = info.height;
    let alpha: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].chunks(4).map(|p| p[3]).collect(),
        png::ColorType::GrayscaleAlpha => {
            buf[..info.buffer_size()].chunks(2).map(|p| p[1]).collect()
        }
        png::ColorType::Rgb | png::ColorType::Grayscale => {
            let bpp = if info.color_type == png::ColorType::Rgb { 3 } else { 1 };
            buf[..info.buffer_size()]
                .chunks(bpp)
                .map(|p| if p.iter().any(|&b| b > 8) { 255 } else { 0 })
                .collect()
        }
        other => return Err(format!("logo color {other:?}")),
    };
    if alpha.len() != (w * h) as usize {
        return Err("logo size mismatch".into());
    }
    Ok(Logo { w, h, alpha })
}

fn paint_fb(path: &str, logo: &Logo) -> Result<(u32, u32), String> {
    let file = OpenOptions::new().read(true).write(true).open(path).map_err(|e| e.to_string())?;
    let fd = file.as_raw_fd();
    let mut vinfo = unsafe { std::mem::zeroed::<FbVarScreeninfo>() };
    let mut finfo = unsafe { std::mem::zeroed::<FbFixScreeninfo>() };
    let rc = unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO, &mut vinfo) };
    if rc < 0 {
        return Err("FBIOGET_VSCREENINFO".into());
    }
    let rc = unsafe { libc::ioctl(fd, FBIOGET_FSCREENINFO, &mut finfo) };
    if rc < 0 {
        return Err("FBIOGET_FSCREENINFO".into());
    }
    if vinfo.xres == 0 || vinfo.yres == 0 || vinfo.bits_per_pixel == 0 {
        return Err("fb has no mode".into());
    }
    let len = finfo.smem_len as usize;
    if len < 64 {
        return Err("fb mmap too small".into());
    }
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        )
    };
    if ptr == libc::MAP_FAILED {
        return Err("mmap".into());
    }
    let map = Map { ptr: ptr.cast(), len };
    let buf = unsafe { std::slice::from_raw_parts_mut(map.ptr, map.len) };

    let (vis_w, vis_h) = visible_size(&vinfo);
    let stride = if finfo.line_length > 0 {
        finfo.line_length as usize
    } else {
        (vinfo.xres_virtual.max(vinfo.xres) as usize) * ((vinfo.bits_per_pixel as usize + 7) / 8)
    };
    let bpp = vinfo.bits_per_pixel;
    let bytes_pp = ((bpp + 7) / 8) as usize;
    // Touch only the visible rectangle. Zeroing a 4K-overalloc smem wedged
    // amdgpu SI when this ran in PID 1.
    for y in 0..vis_h {
        let i = (vinfo.yoffset + y) as usize * stride + vinfo.xoffset as usize * bytes_pp;
        let n = vis_w as usize * bytes_pp;
        if i + n <= buf.len() {
            buf[i..i + n].fill(0);
        }
    }
    let (rs, gs, bs) = rgb_shifts(&vinfo);
    blit_logo(buf, vis_w, vis_h, vinfo.xoffset, vinfo.yoffset, stride, bpp, rs, gs, bs, logo);
    // Keep `file` alive until munmap.
    drop(map);
    drop(file);
    Ok((vis_w, vis_h))
}

fn visible_size(v: &FbVarScreeninfo) -> (u32, u32) {
    let mut w = v.xres;
    let mut h = v.yres;
    if let Some((cw, ch)) = connected_mode() {
        if cw > 0 && cw <= w {
            w = cw;
        }
        if ch > 0 && ch <= h {
            h = ch;
        }
    }
    (w, h)
}

fn connected_mode() -> Option<(u32, u32)> {
    let drm = fs::read_dir("/sys/class/drm").ok()?;
    let mut cards: Vec<_> = drm.filter_map(|e| e.ok().map(|e| e.path())).collect();
    cards.sort();
    for card in cards {
        let name = card.file_name()?.to_string_lossy();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let mut conns: Vec<_> =
            fs::read_dir(&card).ok()?.filter_map(|e| e.ok().map(|e| e.path())).collect();
        conns.sort();
        for conn in conns {
            let cname =
                conn.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if !cname.contains('-') {
                continue;
            }
            let Ok(status) = fs::read_to_string(conn.join("status")) else {
                continue;
            };
            if status.trim() != "connected" {
                continue;
            }
            let Ok(modes) = fs::read_to_string(conn.join("modes")) else {
                continue;
            };
            let Some(line) = modes.lines().next() else {
                continue;
            };
            let Some((a, b)) = line.trim().split_once('x') else {
                continue;
            };
            let Ok(w) = a.parse() else {
                continue;
            };
            let Some(h) =
                b.split(|c: char| !c.is_ascii_digit()).next().and_then(|s| s.parse().ok())
            else {
                continue;
            };
            return Some((w, h));
        }
    }
    None
}

fn rgb_shifts(v: &FbVarScreeninfo) -> (u32, u32, u32) {
    if v.red.length == 0 {
        return match v.bits_per_pixel {
            16 => (11, 5, 0),
            _ => (16, 8, 0),
        };
    }
    (v.red.offset, v.green.offset, v.blue.offset)
}

fn blit_logo(
    buf: &mut [u8],
    vis_w: u32,
    vis_h: u32,
    xoff: u32,
    yoff: u32,
    stride: usize,
    bpp: u32,
    rs: u32,
    gs: u32,
    bs: u32,
    logo: &Logo,
) {
    if vis_w == 0 || vis_h == 0 {
        return;
    }
    let short = vis_w.min(vis_h);
    let dw = (short * MARK_NUM / MARK_DEN).max(1).min(short).min(logo.w.max(1));
    let dh = dw;
    if dw == 0 || dh == 0 {
        return;
    }
    let x0 = vis_w.saturating_sub(dw) / 2;
    let y0 = vis_h.saturating_sub(dh) / 2;
    let bytes_pp = ((bpp + 7) / 8) as usize;
    for y in 0..dh {
        for x in 0..dw {
            let a = sample_alpha(logo, x, y, dw, dh);
            if a == 0 {
                continue;
            }
            let dx = xoff + x0 + x;
            let dy = yoff + y0 + y;
            if dx >= vis_w + xoff || dy >= vis_h + yoff {
                continue;
            }
            let i = dy as usize * stride + dx as usize * bytes_pp;
            put_gray(buf, i, bpp, rs, gs, bs, a);
        }
    }
}

fn sample_alpha(logo: &Logo, x: u32, y: u32, dw: u32, dh: u32) -> u8 {
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

fn put_gray(buf: &mut [u8], i: usize, bpp: u32, rs: u32, gs: u32, bs: u32, gray: u8) {
    match bpp {
        16 => {
            if i + 1 >= buf.len() {
                return;
            }
            let g = gray as u32;
            let px = ((g >> 3) << rs) | ((g >> 2) << gs) | ((g >> 3) << bs);
            buf[i] = px as u8;
            buf[i + 1] = (px >> 8) as u8;
        }
        24 => {
            if i + 2 >= buf.len() {
                return;
            }
            // Packed BGR or RGB from offsets.
            let mut px = [0u8; 3];
            put_comp(&mut px, rs, gray);
            put_comp(&mut px, gs, gray);
            put_comp(&mut px, bs, gray);
            buf[i] = px[0];
            buf[i + 1] = px[1];
            buf[i + 2] = px[2];
        }
        _ => {
            if i + 3 >= buf.len() {
                return;
            }
            let g = gray as u32;
            let px = (g << rs) | (g << gs) | (g << bs);
            buf[i] = px as u8;
            buf[i + 1] = (px >> 8) as u8;
            buf[i + 2] = (px >> 16) as u8;
            buf[i + 3] = (px >> 24) as u8;
        }
    }
}

fn put_comp(px: &mut [u8; 3], offset: u32, gray: u8) {
    let byte = (offset / 8) as usize;
    if byte < 3 {
        px[byte] = gray;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark() -> Logo {
        // 4x4: opaque center 2x2, transparent ring.
        let mut alpha = vec![0u8; 16];
        for y in 1..3 {
            for x in 1..3 {
                alpha[y * 4 + x] = 255;
            }
        }
        Logo { w: 4, h: 4, alpha }
    }

    #[test]
    fn logo_png_decodes() {
        let l = decode_logo_png().expect("logo png");
        assert!(l.w >= 256 && l.h >= 256);
        assert_eq!(l.alpha.len(), (l.w * l.h) as usize);
        let mid = l.alpha[(l.h / 2 * l.w + l.w / 2) as usize];
        assert!(mid > 200, "center of mark should be opaque, got {mid}");
        assert_eq!(l.alpha[0], 0);
        assert_eq!(l.alpha[(l.w - 1) as usize], 0);
    }

    #[test]
    fn blit_centers_white_on_black() {
        let logo = mark();
        let w = 32u32;
        let h = 24u32;
        let stride = w as usize * 4;
        let mut buf = vec![0u8; stride * h as usize];
        blit_logo(&mut buf, w, h, 0, 0, stride, 32, 16, 8, 0, &logo);
        // Corners stay black.
        assert_eq!(&buf[0..4], &[0, 0, 0, 0]);
        let last = (h as usize - 1) * stride;
        assert_eq!(&buf[last..last + 4], &[0, 0, 0, 0]);
        // Center pixel is white-ish (XRGB: B,G,R,X on LE with R@16).
        let cx = (w / 2) as usize;
        let cy = (h / 2) as usize;
        let i = cy * stride + cx * 4;
        let b = buf[i];
        let g = buf[i + 1];
        let r = buf[i + 2];
        assert!(r > 200 && g > 200 && b > 200, "center {r},{g},{b}");
    }

    #[test]
    fn sample_downscale_keeps_core() {
        let logo = mark();
        assert_eq!(sample_alpha(&logo, 0, 0, 4, 4), 0);
        assert_eq!(sample_alpha(&logo, 1, 1, 4, 4), 255);
    }
}
