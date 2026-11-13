//! The beacon: every few seconds each peer says on its LAN who it is, which
//! consort it is in, and where its links listen, signed. Peers that hear a
//! member link to it; a machine in another consort shows as nearby.

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::os::fd::FromRawFd;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use oath_core::consort::{Beacon, Signed};

use crate::Bus;

pub const EVERY: Duration = Duration::from_secs(5);
/// A beacon older than this, by its own clock, is ignored.
const FRESH_SECS: i64 = 120;

/// A UDP socket on the beacon port that several processes may share (two
/// daemons on one machine in a test), and that may broadcast.
fn socket(port: u16) -> std::io::Result<UdpSocket> {
    unsafe {
        let fd = libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0);
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let sock = UdpSocket::from_raw_fd(fd);
        let one: libc::c_int = 1;
        for opt in [libc::SO_REUSEADDR, libc::SO_REUSEPORT, libc::SO_BROADCAST] {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                opt,
                (&one as *const libc::c_int).cast(),
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
        let addr = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: port.to_be(),
            sin_addr: libc::in_addr { s_addr: 0 },
            sin_zero: [0; 8],
        };
        if libc::bind(fd, (&addr as *const libc::sockaddr_in).cast(), std::mem::size_of_val(&addr) as libc::socklen_t)
            < 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(sock)
    }
}

pub fn start(bus: Arc<Bus>) -> std::io::Result<()> {
    let sock = socket(bus.beacon_port)?;
    let send = sock.try_clone()?;
    let to = SocketAddrV4::new(Ipv4Addr::BROADCAST, bus.beacon_port);
    let b = bus.clone();
    thread::spawn(move || loop {
        if let Some(s) = b.beacon() {
            if let Ok(line) = serde_json::to_vec(&s) {
                if let Err(e) = send.send_to(&line, to) {
                    b.say(&format!("beacon: {e}"));
                }
            }
        }
        thread::sleep(EVERY);
    });
    thread::spawn(move || {
        let mut buf = [0u8; 2048];
        loop {
            let Ok((n, from)) = sock.recv_from(&mut buf) else {
                thread::sleep(Duration::from_secs(1));
                continue;
            };
            let Ok(s) = serde_json::from_slice::<Signed>(&buf[..n]) else { continue };
            let Ok(beacon) = s.open::<Beacon>("beacon") else { continue };
            if beacon.v != 1 || !fresh(&beacon.at) {
                continue;
            }
            bus.heard(beacon, SocketAddr::new(from.ip(), 0));
        }
    });
    Ok(())
}

fn fresh(at: &str) -> bool {
    oath_core::consort::age_secs(at).is_some_and(|a| a.abs() <= FRESH_SECS)
}
