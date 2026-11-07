//! EFI splash: native GOP, white mark on black, then LoadImage the kernel.
//! Under the mark it says what it is doing, with a bar for the reads, and
//! hands the framebuffer to PID 1 (`oath.fb=`), which goes on saying it.
//! Never touches Linux framebuffers.

#![no_main]
#![no_std]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::fmt::Write;
use core::ptr;

use uefi::boot::{self, LoadImageSource, ScopedProtocol};
use uefi::mem::memory_map::MemoryType;
use uefi::prelude::*;
use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput, PixelFormat};
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileType, RegularFile};
use uefi::CStr16;
use uefi::{guid, Guid};

use oath_efi::splash::{self, Bar, Fb, Layout, Rect, Rgb, Tone};
use oath_efi::{
    logo, menu_step, parse_bls, parse_loader_conf, parse_oath_boots, pick_mode, raster_mark, Logo,
    MenuKey,
};

const LINUX_INITRD_MEDIA_GUID: Guid = guid!("5568e427-68fc-4f3d-ac74-ca555231cc68");
const LOAD_FILE2_GUID: Guid = guid!("4006c0c1-fcb3-403e-996d-4a6c8724e06d");
const DEVICE_PATH_GUID: Guid = guid!("09576e91-6d3f-11d2-8e39-00a0c969723b");

#[repr(C, packed)]
struct VendorPath {
    type_: u8,
    subtype: u8,
    length: u16,
    guid: Guid,
}

#[repr(C, packed)]
struct EndPath {
    type_: u8,
    subtype: u8,
    length: u16,
}

#[repr(C, packed)]
struct InitrdDevicePath {
    vendor: VendorPath,
    end: EndPath,
}

static INITRD_DP: InitrdDevicePath = InitrdDevicePath {
    vendor: VendorPath {
        type_: 4,   // MEDIA
        subtype: 3, // VENDOR
        length: 20,
        guid: LINUX_INITRD_MEDIA_GUID,
    },
    end: EndPath { type_: 0x7F, subtype: 0xFF, length: 4 },
};

#[repr(C)]
struct InitrdLoader {
    load_file: unsafe extern "efiapi" fn(
        this: *mut InitrdLoader,
        file_path: *const u8,
        boot_policy: bool,
        buffer_size: *mut usize,
        buffer: *mut u8,
    ) -> Status,
    data: *const u8,
    len: usize,
}

unsafe extern "efiapi" fn initrd_load_file(
    this: *mut InitrdLoader,
    file_path: *const u8,
    boot_policy: bool,
    buffer_size: *mut usize,
    buffer: *mut u8,
) -> Status {
    if this.is_null() || buffer_size.is_null() || file_path.is_null() {
        return Status::INVALID_PARAMETER;
    }
    if boot_policy {
        return Status::UNSUPPORTED;
    }
    let loader = unsafe { &*this };
    let need = loader.len;
    if buffer.is_null() || unsafe { *buffer_size } < need {
        unsafe { *buffer_size = need };
        return Status::BUFFER_TOO_SMALL;
    }
    unsafe {
        ptr::copy_nonoverlapping(loader.data, buffer, need);
        *buffer_size = need;
    }
    Status::SUCCESS
}

#[entry]
fn efi_main() -> Status {
    let _ = uefi::helpers::init();
    let mut screen = draw_mark();
    match boot_linux(&mut screen) {
        Ok(()) => Status::SUCCESS,
        Err(why) => {
            // Say why, then back to the firmware, which tries its next
            // boot option.
            if let Some(s) = &screen {
                s.say(&why, Tone::Warn, Bar::Fill(1000));
                boot::stall(10_000_000);
            }
            Status::LOAD_ERROR
        }
    }
}

/// The screen the mark is on: where things go, and the framebuffer PID 1
/// may draw in (none when the firmware only blits).
struct Screen {
    layout: Layout,
    fb: Option<Fb>,
}

impl Screen {
    fn say(&self, text: &str, tone: Tone, bar: Bar) {
        let Some(mut gop) = gop() else { return };
        let l = &self.layout;
        blit(&mut gop, l.bar, &splash::paint_bar(bar, tone, l.bar.w, l.bar.h));
        blit(&mut gop, l.text, &splash::paint_text(text, tone, l.text, l.scale));
    }

    fn bar(&self, bar: Bar) {
        let Some(mut gop) = gop() else { return };
        let l = &self.layout;
        blit(&mut gop, l.bar, &splash::paint_bar(bar, Tone::Normal, l.bar.w, l.bar.h));
    }
}

fn gop() -> Option<ScopedProtocol<GraphicsOutput>> {
    let handle = boot::get_handle_for_protocol::<GraphicsOutput>().ok()?;
    boot::open_protocol_exclusive::<GraphicsOutput>(handle).ok()
}

fn blit(gop: &mut GraphicsOutput, r: Rect, px: &[Rgb]) {
    let buf: Vec<BltPixel> = px.iter().map(|p| BltPixel::new(p.0, p.1, p.2)).collect();
    let _ = gop.blt(BltOp::BufferToVideo {
        buffer: &buf,
        src: BltRegion::Full,
        dest: (r.x as usize, r.y as usize),
        dims: (r.w as usize, r.h as usize),
    });
}

fn draw_mark() -> Option<Screen> {
    let mut gop = gop()?;
    set_native(&mut gop);
    let info = gop.current_mode_info();
    let (w, h) = info.resolution();
    let _ = gop.blt(BltOp::VideoFill { color: BltPixel::new(0, 0, 0), dest: (0, 0), dims: (w, h) });
    let layout = splash::layout(w as u32, h as u32);
    if let Some(logo) = logo() {
        blit_logo(&mut gop, layout.mark, &logo);
    }
    let bgr = match info.pixel_format() {
        PixelFormat::Rgb => Some(false),
        PixelFormat::Bgr => Some(true),
        _ => None,
    };
    let fb = bgr.map(|bgr| Fb {
        base: gop.frame_buffer().as_mut_ptr() as u64,
        w: w as u32,
        h: h as u32,
        stride: info.stride() as u32,
        bgr,
    });
    Some(Screen { layout, fb })
}

fn set_native(gop: &mut GraphicsOutput) {
    let current = gop.current_mode_info().resolution();
    let current = (current.0 as u32, current.1 as u32);
    let modes: Vec<(u32, u32)> = gop
        .modes()
        .map(|m| {
            let (w, h) = m.info().resolution();
            (w as u32, h as u32)
        })
        .collect();
    let want = pick_mode(&modes, current);
    if want == current {
        return;
    }
    let chosen = gop.modes().find(|m| {
        let (w, h) = m.info().resolution();
        (w as u32, h as u32) == want
    });
    if let Some(mode) = chosen {
        let _ = gop.set_mode(&mode);
    }
}

fn blit_logo(gop: &mut GraphicsOutput, at: Rect, logo: &Logo) {
    let mut gray = vec![0u8; (at.w * at.h) as usize];
    raster_mark(logo, at.w, at.h, &mut gray);
    let px: Vec<Rgb> = gray.iter().map(|g| Rgb(*g, *g, *g)).collect();
    blit(gop, at, &px);
}

fn boot_linux(screen: &mut Option<Screen>) -> Result<(), String> {
    let (names, timeout) = boot_list();
    let mut entries = Vec::new();
    for name in &names {
        let mut path = String::from("\\loader\\entries\\");
        path.push_str(name);
        if let Ok(bytes) = read_path(&path) {
            if let Ok(text) = core::str::from_utf8(&bytes) {
                entries.push(parse_bls(text));
            }
        }
    }
    if entries.is_empty() {
        let bls = read_path("\\loader\\entries\\oath.conf").unwrap_or_default();
        let text = core::str::from_utf8(&bls).unwrap_or("");
        entries.push(parse_bls(text));
    }
    let idx = if timeout == 0 || entries.len() <= 1 {
        0
    } else {
        let i = pick_entry(&entries, timeout);
        // The menu wrote over the mark.
        *screen = draw_mark();
        i
    };
    let e = entries.get(idx).ok_or_else(|| String::from("No boot entry"))?;
    start_kernel(screen.as_ref(), &e.linux, &e.initrd, &e.options)
}

fn boot_list() -> (Vec<String>, u32) {
    let loader = read_path("\\loader\\loader.conf").unwrap_or_default();
    let text = core::str::from_utf8(&loader).unwrap_or("");
    let (default, timeout) = parse_loader_conf(text);
    let boots = read_path("\\loader\\oath-boots").unwrap_or_default();
    let mut names = parse_oath_boots(core::str::from_utf8(&boots).unwrap_or(""));
    if names.is_empty() {
        names.push(default);
    }
    (names, timeout)
}

fn pick_entry(entries: &[oath_efi::Bls], timeout: u32) -> usize {
    let n = entries.len();
    let mut idx = 0usize;
    let mut left = timeout;
    print_menu(entries, idx, left);
    loop {
        if let Some(key) = read_menu_key() {
            match menu_step(idx, n, key) {
                None => return 0,
                Some((i, true)) => return i,
                Some((i, false)) => {
                    idx = i;
                    print_menu(entries, idx, left);
                }
            }
            continue;
        }
        boot::stall(1_000_000);
        if left == 0 {
            return idx;
        }
        left -= 1;
        print_menu(entries, idx, left);
    }
}

fn read_menu_key() -> Option<MenuKey> {
    use uefi::proto::console::text::{Input, Key, ScanCode};
    let handle = boot::get_handle_for_protocol::<Input>().ok()?;
    let mut input = boot::open_protocol_exclusive::<Input>(handle).ok()?;
    match input.read_key() {
        Ok(Some(Key::Special(ScanCode::UP))) => Some(MenuKey::Up),
        Ok(Some(Key::Special(ScanCode::DOWN))) => Some(MenuKey::Down),
        Ok(Some(Key::Special(ScanCode::ESCAPE))) => Some(MenuKey::Esc),
        Ok(Some(Key::Printable(c))) => {
            let u = u16::from(c);
            if u == b'\r' as u16 || u == b'\n' as u16 {
                Some(MenuKey::Enter)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn print_menu(entries: &[oath_efi::Bls], idx: usize, left: u32) {
    let _ = uefi::system::with_stdout(|out| {
        let _ = out.clear();
        let _ = out.write_str("Oath boot\r\n");
        let mut t = String::from("timeout ");
        t.push_str(u32_str(left).as_str());
        t.push_str("  (Up/Down/Enter)\r\n\r\n");
        let _ = out.write_str(&t);
        for (i, e) in entries.iter().enumerate() {
            let mark = if i == idx { '>' } else { ' ' };
            let mut line = String::new();
            line.push(mark);
            line.push(' ');
            line.push_str(&e.title);
            line.push_str("\r\n");
            let _ = out.write_str(&line);
        }
    });
}

fn u32_str(n: u32) -> String {
    let mut buf = [0u8; 10];
    let mut x = n;
    if x == 0 {
        return String::from("0");
    }
    let mut i = buf.len();
    while x > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (x % 10) as u8;
        x /= 10;
    }
    String::from(core::str::from_utf8(&buf[i..]).unwrap_or("0"))
}

/// Reads go in pieces this size, so the bar moves.
const CHUNK: usize = 1 << 20;

fn start_kernel(
    screen: Option<&Screen>,
    kernel_path: &str,
    initrd_path: &str,
    options: &str,
) -> Result<(), String> {
    let say = |text: &str, bar| screen.inspect(|s| s.say(text, Tone::Normal, bar));
    let mut kfile = open_regular(kernel_path)
        .map_err(|e| format!("Cannot open the kernel {kernel_path}: {:?}", e.status()))?;
    let mut ifile = open_regular(initrd_path).ok();
    let ksize = file_size(&mut kfile);
    let isize = ifile.as_mut().map(file_size).unwrap_or(0);
    let total = (ksize + isize).max(1);
    let mut done = 0u64;
    let mut step = |n: usize| {
        done += n as u64;
        screen.inspect(|s| s.bar(Bar::Fill((done * 1000 / total) as u32)));
    };
    say(&format!("Reading the kernel ({})", mb(ksize)), Bar::Fill(0));
    let kernel = read_file(&mut kfile, &mut step)
        .map_err(|e| format!("Cannot read the kernel {kernel_path}: {:?}", e.status()))?;
    let initrd = match ifile.as_mut() {
        Some(f) => {
            say(
                &format!("Reading the initrd ({})", mb(isize)),
                Bar::Fill((ksize * 1000 / total) as u32),
            );
            let data = read_file(f, &mut step)
                .map_err(|e| format!("Cannot read the initrd {initrd_path}: {:?}", e.status()))?;
            Some(data)
        }
        None => None,
    };
    let release = oath_efi::kernel_release(&kernel).unwrap_or("");
    let cmdline = quiet_cmdline(options, screen.and_then(|s| s.fb));
    if let Some(initrd) = initrd {
        register_initrd(initrd).map_err(|_| String::from("Cannot hand the initrd to Linux"))?;
    }
    let khandle = boot::load_image(
        boot::image_handle(),
        LoadImageSource::FromBuffer { buffer: &kernel, file_path: None },
    )
    .map_err(|e| {
        let s = e.status();
        let hint = if s == Status::SECURITY_VIOLATION || s == Status::ACCESS_DENIED {
            " (Secure Boot is on? Oath's loader is not signed for it)"
        } else {
            ""
        };
        format!("The firmware would not load the kernel: {s:?}{hint}")
    })?;
    set_cmdline(khandle, &cmdline).map_err(|_| String::from("Cannot give Linux its command line"))?;
    // From here the screen is Linux's: this line stays until PID 1 says
    // the next, so a machine that stops here stopped in the kernel.
    say(&format!("Starting Linux {release}"), Bar::Fill(1000));
    boot::start_image(khandle).map_err(|e| format!("Linux stopped: {:?}", e.status()))?;
    Ok(())
}

fn mb(bytes: u64) -> String {
    format!("{}.{} MB", bytes / 1_000_000, bytes % 1_000_000 / 100_000)
}

fn quiet_cmdline(options: &str, fb: Option<Fb>) -> String {
    let mut out = String::from("quiet loglevel=0 vt.global_cursor_default=0 logo.nologo");
    for tok in options.split_whitespace() {
        if tok == "quiet"
            || tok.starts_with("oath.fb=")
            || tok.starts_with("loglevel=")
            || tok == "logo.nologo"
            || tok.starts_with("vt.global_cursor_default=")
            || tok == "console=tty0"
            || tok.starts_with("console=tty0,")
        {
            continue;
        }
        out.push(' ');
        out.push_str(tok);
    }
    if !out.split_whitespace().any(|t| t.starts_with("console=")) {
        out.push_str(" console=ttyS0,115200");
    }
    if let Some(fb) = fb {
        out.push(' ');
        out.push_str(&fb.arg());
    }
    out
}

fn read_path(path: &str) -> Result<Vec<u8>, ()> {
    let mut f = open_regular(path).map_err(|_| ())?;
    read_file(&mut f, &mut |_| {}).map_err(|_| ())
}

fn open_regular(path: &str) -> uefi::Result<RegularFile> {
    let mut path16 = vec![0u16; path.len() + 2];
    let mut n = 0usize;
    for c in path.encode_utf16() {
        path16[n] = c;
        n += 1;
    }
    path16[n] = 0;
    let cpath = CStr16::from_u16_with_nul(&path16[..=n])
        .map_err(|_| uefi::Error::from(Status::INVALID_PARAMETER))?;
    let mut sfs = boot::get_image_file_system(boot::image_handle())?;
    let mut dir = sfs.open_volume()?;
    let fh = dir.open(cpath, FileMode::Read, FileAttribute::empty())?;
    match fh.into_type()? {
        FileType::Regular(f) => Ok(f),
        FileType::Dir(_) => Err(Status::NOT_FOUND.into()),
    }
}

fn file_size(f: &mut RegularFile) -> u64 {
    f.get_boxed_info::<uefi::proto::media::file::FileInfo>().map(|i| i.file_size()).unwrap_or(0)
}

/// The whole file, in `CHUNK`s, telling `step` each piece's length.
fn read_file(f: &mut RegularFile, step: &mut dyn FnMut(usize)) -> uefi::Result<Vec<u8>> {
    let mut buf = vec![0u8; file_size(f) as usize];
    let mut off = 0usize;
    while off < buf.len() {
        let end = (off + CHUNK).min(buf.len());
        let n = f.read(&mut buf[off..end]).map_err(|e| uefi::Error::from(e.status()))?;
        if n == 0 {
            break;
        }
        off += n;
        step(n);
    }
    buf.truncate(off);
    Ok(buf)
}

fn set_cmdline(handle: uefi::Handle, cmdline: &str) -> Result<(), ()> {
    let mut opts: Vec<u16> = cmdline.encode_utf16().collect();
    opts.push(0);
    let bytes = opts.len() * 2;
    let mut li: ScopedProtocol<LoadedImage> =
        boot::open_protocol_exclusive::<LoadedImage>(handle).map_err(|_| ())?;
    unsafe {
        li.set_load_options(opts.as_ptr() as *const u8, bytes as u32);
    }
    // Keep opts alive for StartImage.
    core::mem::forget(opts);
    Ok(())
}

fn register_initrd(data: Vec<u8>) -> Result<(), ()> {
    let leaked = data.leak();
    let loader = boot::allocate_pool(MemoryType::LOADER_DATA, core::mem::size_of::<InitrdLoader>())
        .map_err(|_| ())?;
    let loader = loader.cast::<InitrdLoader>();
    unsafe {
        ptr::write(
            loader.as_ptr(),
            InitrdLoader { load_file: initrd_load_file, data: leaked.as_ptr(), len: leaked.len() },
        );
    }
    unsafe {
        let h = boot::install_protocol_interface(
            None,
            &DEVICE_PATH_GUID,
            ptr::addr_of!(INITRD_DP) as *mut c_void,
        )
        .map_err(|_| ())?;
        boot::install_protocol_interface(Some(h), &LOAD_FILE2_GUID, loader.as_ptr().cast())
            .map_err(|_| ())?;
    }
    Ok(())
}
