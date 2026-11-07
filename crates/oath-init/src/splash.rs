//! The status under the boot mark, after oath-efi. oath-efi names the
//! firmware's framebuffer on the command line (`oath.fb=`); nothing in
//! Linux drives it until a display driver loads (the boot holds those
//! back), so PID 1 maps those rows through `/dev/mem` and says what it is
//! doing there, with a bar that moves while it lives. The first display
//! driver ends it: the mapping goes before the driver loads. When no
//! driver takes the screen, the line says so, with the machine's address.

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

use oath_efi::splash::{self as art, Bar, Fb, Layout, Rect, Rgb, Tone};

const FRAME: Duration = Duration::from_millis(40);
const REFRESH: Duration = Duration::from_secs(2);

/// The status rows of the framebuffer, mapped.
struct Map {
    ptr: *mut u8,
    len: usize,
    /// Where the first status row starts in the mapping.
    skew: usize,
    fb: Fb,
    layout: Layout,
}

// Only the splash's lock reaches the mapping.
unsafe impl Send for Map {}

impl Map {
    fn open(fb: Fb) -> Result<Map, String> {
        let layout = art::layout(fb.w, fb.h);
        let pitch = fb.stride as u64 * 4;
        let start = fb.base + layout.bar.y as u64 * pitch;
        let end = fb.base + (layout.text.y + layout.text.h) as u64 * pitch;
        let off = start & !4095;
        let len = (end - off) as usize;
        let fd = unsafe {
            libc::open(c"/dev/mem".as_ptr(), libc::O_RDWR | libc::O_SYNC | libc::O_CLOEXEC)
        };
        if fd < 0 {
            return Err(format!("/dev/mem: {}", std::io::Error::last_os_error()));
        }
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                off as libc::off_t,
            )
        };
        let err = std::io::Error::last_os_error();
        unsafe { libc::close(fd) };
        if ptr == libc::MAP_FAILED {
            return Err(format!("map 0x{off:x}+{len}: {err}"));
        }
        Ok(Map { ptr: ptr.cast(), len, skew: (start - off) as usize, fb, layout })
    }

    fn put(&self, r: Rect, px: &[Rgb]) {
        let row0 = self.layout.bar.y;
        let pitch = self.fb.stride as usize * 4;
        let mut line = vec![0u32; r.w as usize];
        for y in 0..r.h {
            let src = &px[(y * r.w) as usize..((y + 1) * r.w) as usize];
            for (d, p) in line.iter_mut().zip(src) {
                *d = art::word(*p, self.fb.bgr);
            }
            let at = self.skew + (r.y + y - row0) as usize * pitch + r.x as usize * 4;
            if at + line.len() * 4 > self.len {
                return;
            }
            unsafe {
                std::ptr::copy_nonoverlapping(
                    line.as_ptr().cast::<u8>(),
                    self.ptr.add(at),
                    line.len() * 4,
                );
            }
        }
    }

    fn text(&self, text: &str, tone: Tone) {
        let l = &self.layout;
        self.put(l.text, &art::paint_text(text, tone, l.text, l.scale));
    }

    fn bar(&self, bar: Bar, tone: Tone) {
        let l = &self.layout;
        self.put(l.bar, &art::paint_bar(bar, tone, l.bar.w, l.bar.h));
    }
}

impl Drop for Map {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr.cast(), self.len) };
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Booting: the bar sweeps.
    Booting,
    /// Boot failed: the line is amber, the bar full and still.
    Failed,
    /// Booted with no display driver: the line names the display and the
    /// machine's address, again every few seconds.
    Idle,
}

struct State {
    map: Map,
    text: String,
    mode: Mode,
    dirty: bool,
    /// The thread that draws this state; an older one ends.
    gen: u64,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);
static GEN: AtomicU64 = AtomicU64::new(0);
/// The framebuffer, kept after a driver's release to open it again if the
/// driver never took the screen.
static FB: Mutex<Option<Fb>> = Mutex::new(None);

/// Start, when oath-efi named a framebuffer.
pub fn open() {
    let Some(fb) = crate::cmdline_val("oath.fb").and_then(|s| Fb::parse(&s)) else { return };
    *FB.lock().unwrap_or_else(|e| e.into_inner()) = Some(fb);
    if let Err(e) = start(fb, String::new(), Mode::Booting) {
        crate::log(&format!("splash: {e}"));
    }
}

fn start(fb: Fb, text: String, mode: Mode) -> Result<(), String> {
    let map = Map::open(fb)?;
    let gen = GEN.fetch_add(1, Ordering::Relaxed) + 1;
    *lock() = Some(State { map, text, mode, dirty: true, gen });
    std::thread::Builder::new()
        .name("splash".into())
        .spawn(move || run(gen))
        .map(|_| ())
        .map_err(|e| format!("thread: {e}"))
}

fn lock() -> MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The lock, unless it stays taken for a tenth of a second: the panic
/// hook must not wait on a thread forever.
fn try_lock() -> Option<MutexGuard<'static, Option<State>>> {
    for _ in 0..10 {
        match STATE.try_lock() {
            Ok(g) => return Some(g),
            Err(TryLockError::Poisoned(e)) => return Some(e.into_inner()),
            Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    None
}

/// What PID 1 is doing now. Drawn on the next frame; a later line before
/// then replaces it.
pub fn say(text: &str) {
    if let Some(st) = lock().as_mut().filter(|st| st.mode == Mode::Booting) {
        st.text = text.to_string();
        st.dirty = true;
    }
}

/// The boot failed: amber, drawn now, since PID 1 may not live to the
/// next frame.
pub fn fail(text: &str) {
    let Some(mut g) = try_lock() else { return };
    let Some(st) = g.as_mut().filter(|st| st.mode == Mode::Booting) else { return };
    st.mode = Mode::Failed;
    st.text = text.to_string();
    st.dirty = false;
    st.map.text(text, Tone::Warn);
    st.map.bar(Bar::Fill(1000), Tone::Warn);
}

/// A display driver is about to load: let go of its framebuffer first.
pub fn release() {
    let _ = lock().take();
}

/// Boot is done. If no display driver took the screen, say so there.
pub fn done() {
    if has_display_driver() {
        release();
        return;
    }
    let mut g = lock();
    if let Some(st) = g.as_mut() {
        st.mode = Mode::Idle;
        st.text = idle_text();
        st.dirty = true;
        return;
    }
    drop(g);
    let Some(fb) = *FB.lock().unwrap_or_else(|e| e.into_inner()) else { return };
    if let Err(e) = start(fb, idle_text(), Mode::Idle) {
        crate::log(&format!("splash: {e}"));
    }
}

fn run(gen: u64) {
    let mut frame = 0u32;
    let mut refreshed = Instant::now();
    loop {
        {
            let mut g = lock();
            let Some(st) = g.as_mut().filter(|st| st.gen == gen) else { return };
            if st.mode == Mode::Idle && refreshed.elapsed() >= REFRESH {
                refreshed = Instant::now();
                let text = idle_text();
                st.dirty |= text != st.text;
                st.text = text;
            }
            if st.dirty {
                st.dirty = false;
                st.map.text(&st.text, Tone::Normal);
                if st.mode == Mode::Idle {
                    st.map.bar(Bar::Hidden, Tone::Normal);
                }
            }
            if st.mode == Mode::Booting {
                st.map.bar(Bar::Sweep(frame), Tone::Normal);
            }
        }
        frame = frame.wrapping_add(1);
        std::thread::sleep(FRAME);
    }
}

/// A DRM device is there: a driver has the screen.
fn has_display_driver() -> bool {
    fs::read_dir("/sys/class/drm").into_iter().flatten().flatten().any(|e| {
        let n = e.file_name();
        let n = n.to_string_lossy();
        n.starts_with("card") && !n.contains('-')
    })
}

fn idle_text() -> String {
    let display = match display_device() {
        Some((vendor, device)) => {
            let name = match vendor.as_str() {
                "10de" => "NVIDIA ",
                "1002" => "AMD ",
                "8086" => "Intel ",
                _ => "",
            };
            format!("Oath is running, with no driver for this {name}display ({vendor}:{device}).")
        }
        None => "Oath is running, with no driver for this display.".to_string(),
    };
    // The installer's ramdisk takes root; a system, home.
    let user = if crate::cmdline_flag("oath.install") { "root" } else { "home" };
    let reach = match ipv4_addrs().first() {
        Some(a) => format!(" Reach it with ssh {user}@{a}"),
        None => " No network address yet.".to_string(),
    };
    display + &reach
}

/// The PCI display controller the firmware booted on (`boot_vga`), else
/// the first one: vendor and device, in hex.
fn display_device() -> Option<(String, String)> {
    let read = |p: &Path, f: &str| {
        fs::read_to_string(p.join(f)).ok().map(|s| s.trim().trim_start_matches("0x").to_string())
    };
    let mut found: Vec<(bool, String, String)> = fs::read_dir("/sys/bus/pci/devices")
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| read(p, "class").is_some_and(|c| c.starts_with("03")))
        .filter_map(|p| {
            let boot = read(&p, "boot_vga").as_deref() == Some("1");
            Some((boot, read(&p, "vendor")?, read(&p, "device")?))
        })
        .collect();
    found.sort_by_key(|(boot, _, _)| !boot);
    found.into_iter().next().map(|(_, v, d)| (v, d))
}

/// The machine's IPv4 addresses but loopback's.
fn ipv4_addrs() -> Vec<String> {
    let mut out = Vec::new();
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return out;
    }
    let mut cur = head;
    while !cur.is_null() {
        let ifa = unsafe { &*cur };
        let addr = ifa.ifa_addr;
        if !addr.is_null() && unsafe { (*addr).sa_family } as i32 == libc::AF_INET {
            let sin = unsafe { &*(addr as *const libc::sockaddr_in) };
            let ip = std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
            if !ip.is_loopback() {
                out.push(ip.to_string());
            }
        }
        cur = ifa.ifa_next;
    }
    unsafe { libc::freeifaddrs(head) };
    out
}

/// The disks the kernel sees now (no partitions), for a boot that waits
/// on one.
pub fn disks() -> String {
    let mut names: Vec<String> = fs::read_dir("/sys/class/block")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !e.path().join("partition").exists())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !["loop", "ram", "zram", "sr", "fd"].iter().any(|p| n.starts_with(p)))
        .collect();
    names.sort();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_without_a_framebuffer() {
        // No `oath.fb=` in a test's command line: nothing opens, and every
        // call is a no-op.
        say("Loading drivers");
        fail("Boot failed");
        release();
        assert!(lock().is_none());
    }
}
