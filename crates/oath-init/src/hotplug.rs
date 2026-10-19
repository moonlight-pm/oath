//! Hotplug: PID 1 reads the kernel's uevents (netlink), loads the driver an
//! added device asks for with the same index as coldplug, and has the
//! `dev:*` inventory discovered again when a disk, NIC or serial port
//! comes or goes. devtmpfs makes the nodes; there is no udev. PID 1 passes
//! each uevent on to group 4, where udevd would, for libudev-zero's
//! monitors (libinput and wlroots hear hotplug there).

use std::collections::HashSet;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::Path;
use std::time::{Duration, Instant};

use oath_core::{tel, Catalog, DEFAULT_ROOT};
use serde_json::json;

use crate::log;
use crate::modules::{self, Index};

/// Wait this long after the last device event before discovering again,
/// so a stick with four partitions is one pass.
const SETTLE: Duration = Duration::from_secs(1);
/// The catalog was busy: try again after this.
const BUSY: Duration = Duration::from_secs(2);
/// The netlink group libudev's "udev" monitors listen on.
const UDEV_GROUP: u32 = 4;

pub struct Hotplug {
    sock: Option<OwnedFd>,
    index: Option<Index>,
    /// When to discover devices again.
    rediscover: Option<Instant>,
}

/// One kernel uevent.
#[derive(Debug, Default, PartialEq)]
pub struct Event {
    pub action: String,
    pub subsystem: String,
    pub modalias: String,
}

impl Hotplug {
    /// Listen before the first coldplug, so nothing between the two is
    /// missed (a full buffer drops old events, which coldplug covered).
    pub fn open() -> Self {
        let sock = unsafe {
            let fd = libc::socket(
                libc::AF_NETLINK,
                libc::SOCK_DGRAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                libc::NETLINK_KOBJECT_UEVENT,
            );
            if fd < 0 {
                None
            } else {
                let fd = OwnedFd::from_raw_fd(fd);
                let mut addr: libc::sockaddr_nl = std::mem::zeroed();
                addr.nl_family = libc::AF_NETLINK as u16;
                // Group 1: the kernel's own broadcasts.
                addr.nl_groups = 1;
                let size: libc::c_int = 4 << 20;
                libc::setsockopt(
                    fd.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_RCVBUFFORCE,
                    (&size as *const libc::c_int).cast(),
                    std::mem::size_of::<libc::c_int>() as u32,
                );
                let ok = libc::bind(
                    fd.as_raw_fd(),
                    (&addr as *const libc::sockaddr_nl).cast(),
                    std::mem::size_of::<libc::sockaddr_nl>() as u32,
                ) == 0;
                ok.then_some(fd)
            }
        };
        if sock.is_none() {
            log("hotplug: no uevent socket; devices added after boot wait for the next one");
        }
        Self { sock, index: None, rediscover: None }
    }

    /// The socket, for a PID 1 that execs itself ([`crate::resume`]).
    pub fn take_socket(&mut self) -> Option<OwnedFd> {
        self.sock.take()
    }

    /// Go on with a socket a previous PID 1 opened.
    pub fn from_socket(sock: Option<OwnedFd>) -> Self {
        Self { sock, index: None, rediscover: None }
    }

    /// Handle what arrived since the last call. Never blocks.
    pub fn tick(&mut self) {
        let mut aliases = Vec::new();
        for ev in self.drain() {
            if ev.action == "add" && !ev.modalias.is_empty() {
                aliases.push(ev.modalias.clone());
            }
            if matches!(ev.subsystem.as_str(), "block" | "net" | "tty")
                && matches!(ev.action.as_str(), "add" | "remove" | "move" | "change")
            {
                self.rediscover = Some(Instant::now() + SETTLE);
            }
            if ev.subsystem == "drm" && ev.action == "add" {
                crate::open_render_nodes();
            }
            if ev.subsystem == "sound" && ev.action == "add" {
                crate::open_sound_nodes();
            }
        }
        if !aliases.is_empty() {
            self.load(&aliases);
        }
        if self.rediscover.is_some_and(|t| Instant::now() >= t) {
            self.rediscover = if discover() { None } else { Some(Instant::now() + BUSY) };
        }
    }

    fn drain(&mut self) -> Vec<Event> {
        let Some(sock) = &self.sock else { return Vec::new() };
        let mut out = Vec::new();
        let mut buf = vec![0u8; 16 * 1024];
        loop {
            let n = unsafe { libc::recv(sock.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0) };
            if n <= 0 {
                // EAGAIN: drained. ENOBUFS: the kernel dropped some; keep going.
                if n < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOBUFS) {
                    continue;
                }
                break;
            }
            if let Some(ev) = parse(&buf[..n as usize]) {
                forward(sock, &buf[..n as usize]);
                out.push(ev);
            }
        }
        out
    }

    /// The drivers these aliases ask for (display and sound included: boot
    /// is over).
    fn load(&mut self, aliases: &[String]) {
        if self.index.is_none() {
            self.index = Index::open(&modules::base());
        }
        let Some(idx) = &self.index else { return };
        let mut seen = HashSet::new();
        for alias in aliases {
            for m in idx.resolve(alias) {
                match idx.load(&m, &mut seen) {
                    Ok(true) => {
                        log(&format!("hotplug: {m} for {alias}"));
                        tel("init", "hotplug", json!({ "module": m, "alias": alias }));
                    }
                    Ok(false) => {}
                    Err(e) => log(&format!("hotplug: {e}")),
                }
            }
        }
    }
}

/// Send a kernel uevent, unchanged, to [`UDEV_GROUP`]. libudev-zero takes
/// it there (it reads `KEY=value` fields and skips the `action@devpath`
/// head); nothing listens when no desk runs.
fn forward(sock: &OwnedFd, msg: &[u8]) {
    let mut addr: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    addr.nl_family = libc::AF_NETLINK as u16;
    addr.nl_groups = UDEV_GROUP;
    // SAFETY: msg and addr outlive the call; an error (no listener) is ignored.
    unsafe {
        libc::sendto(
            sock.as_raw_fd(),
            msg.as_ptr().cast(),
            msg.len(),
            libc::MSG_DONTWAIT,
            (&addr as *const libc::sockaddr_nl).cast(),
            std::mem::size_of::<libc::sockaddr_nl>() as u32,
        );
    }
}

/// Discover `dev:*` again, if the catalog is free. False when it was busy.
pub fn discover() -> bool {
    let Ok(cat) = Catalog::open(DEFAULT_ROOT) else { return true };
    match cat.try_lock() {
        Ok(Some(lock)) => {
            sync(&cat, &lock);
            true
        }
        Ok(None) => false,
        Err(e) => {
            log(&format!("dev: {e}"));
            true
        }
    }
}

/// Write what sysfs has into `dev:*` (the caller holds the lock).
pub fn sync(cat: &Catalog, lock: &oath_core::CatalogLock) {
    let found = oath_core::discover_devices(Path::new("/sys"));
    match cat.write_devices(lock, &found) {
        Ok(changes) if !changes.is_empty() => {
            log(&format!("dev: {}", changes.join(" ")));
            tel("init", "dev", json!({ "changes": changes }));
        }
        Ok(_) => {}
        Err(e) => log(&format!("dev: {e}")),
    }
}

/// `add@/devices/…\0ACTION=add\0SUBSYSTEM=pci\0MODALIAS=…\0…`. Messages
/// from udev's library (`libudev` first) are not the kernel's.
pub fn parse(msg: &[u8]) -> Option<Event> {
    let mut fields = msg.split(|b| *b == 0).map(|f| String::from_utf8_lossy(f).into_owned());
    let head = fields.next()?;
    if !head.contains('@') {
        return None;
    }
    let mut ev = Event::default();
    for f in fields {
        if let Some((k, v)) = f.split_once('=') {
            match k {
                "ACTION" => ev.action = v.to_string(),
                "SUBSYSTEM" => ev.subsystem = v.to_string(),
                "MODALIAS" => ev.modalias = v.to_string(),
                _ => {}
            }
        }
    }
    (!ev.action.is_empty()).then_some(ev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_kernel_uevent() {
        let msg = b"add@/devices/pci0000:00/0000:00:05.0\0ACTION=add\0DEVPATH=/devices/pci0000:00/0000:00:05.0\0SUBSYSTEM=pci\0MODALIAS=pci:v00001AF4d00001005sv00001AF4sd00000004bc00scFFi00\0SEQNUM=1234\0";
        let ev = parse(msg).unwrap();
        assert_eq!(ev.action, "add");
        assert_eq!(ev.subsystem, "pci");
        assert!(ev.modalias.starts_with("pci:v00001AF4"));
        assert!(parse(b"libudev\0\xfe\xed").is_none());
    }
}
