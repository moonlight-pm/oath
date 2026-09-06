//! EFI splash: native GOP, white mark on black, then LoadImage the kernel.
//! Never touches Linux framebuffers.

#![no_main]
#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::ptr;

use uefi::boot::{self, LoadImageSource, ScopedProtocol};
use uefi::mem::memory_map::MemoryType;
use uefi::prelude::*;
use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::file::{File, FileAttribute, FileMode, FileType};
use uefi::CStr16;
use uefi::{guid, Guid};

use oath_efi::{
    logo, mark_size, menu_step, parse_bls, parse_loader_conf, parse_oath_boots, pick_mode,
    raster_mark, Logo, MenuKey,
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
    let _ = draw_mark();
    match boot_linux() {
        Ok(()) => Status::SUCCESS,
        Err(_) => boot_systemd_boot().unwrap_or(Status::LOAD_ERROR),
    }
}

fn draw_mark() -> Result<(), ()> {
    let handle = boot::get_handle_for_protocol::<GraphicsOutput>().map_err(|_| ())?;
    let mut gop = boot::open_protocol_exclusive::<GraphicsOutput>(handle).map_err(|_| ())?;
    set_native(&mut gop);
    let info = gop.current_mode_info();
    let (w, h) = info.resolution();
    let _ = gop.blt(BltOp::VideoFill { color: BltPixel::new(0, 0, 0), dest: (0, 0), dims: (w, h) });
    let Some(logo) = logo() else { return Ok(()) };
    blit_logo(&mut gop, w as u32, h as u32, &logo);
    Ok(())
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

fn blit_logo(gop: &mut GraphicsOutput, vis_w: u32, vis_h: u32, logo: &Logo) {
    let dw = mark_size(vis_w, vis_h, logo);
    let dh = dw;
    let mut gray = vec![0u8; (dw * dh) as usize];
    raster_mark(logo, dw, dh, &mut gray);
    let mut px = vec![BltPixel::new(0, 0, 0); gray.len()];
    for (i, g) in gray.iter().enumerate() {
        px[i] = BltPixel::new(*g, *g, *g);
    }
    let x0 = vis_w.saturating_sub(dw) / 2;
    let y0 = vis_h.saturating_sub(dh) / 2;
    let _ = gop.blt(BltOp::BufferToVideo {
        buffer: &px,
        src: BltRegion::Full,
        dest: (x0 as usize, y0 as usize),
        dims: (dw as usize, dh as usize),
    });
}

fn boot_linux() -> Result<(), ()> {
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
    let idx = if timeout == 0 || entries.len() <= 1 { 0 } else { pick_entry(&entries, timeout) };
    let e = entries.get(idx).ok_or(())?;
    start_kernel(&e.linux, &e.initrd, &e.options)
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

fn start_kernel(kernel_path: &str, initrd_path: &str, options: &str) -> Result<(), ()> {
    let kernel = read_path(kernel_path).map_err(|_| ())?;
    let initrd = read_path(initrd_path).ok();
    let cmdline = quiet_cmdline(options);
    if let Some(initrd) = initrd {
        register_initrd(initrd)?;
    }
    let khandle = boot::load_image(
        boot::image_handle(),
        LoadImageSource::FromBuffer { buffer: &kernel, file_path: None },
    )
    .map_err(|_| ())?;
    set_cmdline(khandle, &cmdline)?;
    boot::start_image(khandle).map_err(|_| ())?;
    Ok(())
}

fn boot_systemd_boot() -> Result<Status, ()> {
    let bytes = read_path("\\EFI\\systemd\\systemd-bootx64.efi").map_err(|_| ())?;
    let handle = boot::load_image(
        boot::image_handle(),
        LoadImageSource::FromBuffer { buffer: &bytes, file_path: None },
    )
    .map_err(|_| ())?;
    boot::start_image(handle).map_err(|_| ())?;
    Ok(Status::SUCCESS)
}

fn quiet_cmdline(options: &str) -> String {
    let mut out = String::from(
        "quiet loglevel=0 vt.global_cursor_default=0 logo.nologo drm_kms_helper.fbdev_emulation=0",
    );
    for tok in options.split_whitespace() {
        if tok == "quiet"
            || tok.starts_with("loglevel=")
            || tok == "logo.nologo"
            || tok.starts_with("vt.global_cursor_default=")
            || tok.starts_with("drm_kms_helper.fbdev_emulation=")
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
    out
}

fn read_path(path: &str) -> Result<Vec<u8>, ()> {
    let mut path16 = vec![0u16; path.len() + 2];
    let mut n = 0usize;
    for c in path.encode_utf16() {
        path16[n] = c;
        n += 1;
    }
    path16[n] = 0;
    let cpath = CStr16::from_u16_with_nul(&path16[..=n]).map_err(|_| ())?;
    let mut sfs = boot::get_image_file_system(boot::image_handle()).map_err(|_| ())?;
    let mut dir = sfs.open_volume().map_err(|_| ())?;
    let fh = dir.open(cpath, FileMode::Read, FileAttribute::empty()).map_err(|_| ())?;
    let mut regular = match fh.into_type().map_err(|_| ())? {
        FileType::Regular(f) => f,
        FileType::Dir(_) => return Err(()),
    };
    let info = regular.get_boxed_info::<uefi::proto::media::file::FileInfo>().map_err(|_| ())?;
    let mut buf = vec![0u8; info.file_size() as usize];
    let mut off = 0usize;
    while off < buf.len() {
        let n = regular.read(&mut buf[off..]).map_err(|_| ())?;
        if n == 0 {
            break;
        }
        off += n;
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
