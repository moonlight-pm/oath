//! A link: one TCP connection to another peer, encrypted by a Noise XX
//! handshake on the two machines' X25519 link keys. Each side learns the
//! other's link key in the handshake; the membership log says whose it is.
//!
//! On the wire, a frame is a 2-byte length and one Noise message. A message
//! of ours is a 4-byte length and its JSON, cut into frames of at most a
//! Noise message each.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use oath_core::consort::b64;
use snow::params::{CipherChoice, DHChoice, HashChoice};
use snow::resolvers::{CryptoResolver, DefaultResolver};
use snow::types::{Cipher, Dh, Hash, Random};
use snow::{Builder, HandshakeState, TransportState};

use crate::Bus;

const NOISE: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";
const PROLOGUE: &[u8] = b"oath-bus/1";
const MAX_NOISE: usize = 65535;
const TAG: usize = 16;
/// The largest message a peer may send: a peer's packages are tens of KB.
const MAX_MESSAGE: usize = 16 << 20;
/// A link with nothing on it this long is dead (pings go every 20 s).
pub const IDLE: Duration = Duration::from_secs(60);

pub type Noise = Arc<Mutex<TransportState>>;

fn builder(bus: &Bus) -> io::Result<Builder<'_>> {
    let params = NOISE.parse().map_err(err)?;
    Builder::with_resolver(params, Box::new(Resolver))
        .local_private_key(bus.id.link_secret())
        .map_err(err)?
        .prologue(PROLOGUE)
        .map_err(err)
}

/// snow's own X25519, ChaCha20-Poly1305 and BLAKE2s, with the kernel's
/// random numbers.
struct Resolver;

impl CryptoResolver for Resolver {
    fn resolve_rng(&self) -> Option<Box<dyn Random>> {
        Some(Box::new(Kernel))
    }

    fn resolve_dh(&self, choice: &DHChoice) -> Option<Box<dyn Dh>> {
        DefaultResolver.resolve_dh(choice)
    }

    fn resolve_hash(&self, choice: &HashChoice) -> Option<Box<dyn Hash>> {
        DefaultResolver.resolve_hash(choice)
    }

    fn resolve_cipher(&self, choice: &CipherChoice) -> Option<Box<dyn Cipher>> {
        DefaultResolver.resolve_cipher(choice)
    }
}

/// `getrandom(2)`: blocks only until the kernel's pool is first ready.
struct Kernel;

impl Random for Kernel {
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), snow::Error> {
        let mut done = 0;
        while done < dest.len() {
            let left = &mut dest[done..];
            let n = unsafe { libc::getrandom(left.as_mut_ptr().cast(), left.len(), 0) };
            if n < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(snow::Error::Rng);
            }
            done += n as usize;
        }
        Ok(())
    }
}

fn err(e: impl std::fmt::Debug) -> io::Error {
    io::Error::other(format!("{e:?}"))
}

/// Dial `addr` and run the link until it ends. `join`: the link is this
/// machine's ask to join that peer's consort.
pub fn dial(bus: Arc<Bus>, addr: SocketAddr, join: bool) {
    thread::spawn(move || {
        let result = (|| -> io::Result<()> {
            let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
            s.set_read_timeout(Some(Duration::from_secs(10)))?;
            s.set_nodelay(true)?;
            let mut hs = builder(&bus)?.build_initiator().map_err(err)?;
            send_hs(&mut s, &mut hs)?;
            recv_hs(&mut s, &mut hs)?;
            send_hs(&mut s, &mut hs)?;
            run(&bus, s, hs, addr, join)
        })();
        bus.dial_ended(addr);
        if let Err(e) = result {
            bus.say(&format!("link to {addr}: {e}"));
        }
    });
}

/// Answer a connection a peer made.
pub fn accept(bus: Arc<Bus>, mut s: TcpStream, addr: SocketAddr) {
    thread::spawn(move || {
        let result = (|| -> io::Result<()> {
            s.set_read_timeout(Some(Duration::from_secs(10)))?;
            s.set_nodelay(true)?;
            let mut hs = builder(&bus)?.build_responder().map_err(err)?;
            recv_hs(&mut s, &mut hs)?;
            send_hs(&mut s, &mut hs)?;
            recv_hs(&mut s, &mut hs)?;
            run(&bus, s, hs, addr, false)
        })();
        if let Err(e) = result {
            bus.say(&format!("link from {addr}: {e}"));
        }
    });
}

fn send_hs(s: &mut TcpStream, hs: &mut HandshakeState) -> io::Result<()> {
    let mut buf = vec![0u8; MAX_NOISE];
    let n = hs.write_message(&[], &mut buf).map_err(err)?;
    write_frame(s, &buf[..n])
}

fn recv_hs(s: &mut TcpStream, hs: &mut HandshakeState) -> io::Result<()> {
    let frame = read_frame(s)?;
    let mut buf = vec![0u8; MAX_NOISE];
    hs.read_message(&frame, &mut buf).map_err(err)?;
    Ok(())
}

fn run(
    bus: &Arc<Bus>,
    s: TcpStream,
    hs: HandshakeState,
    addr: SocketAddr,
    join: bool,
) -> io::Result<()> {
    let remote = hs.get_remote_static().map(b64).ok_or_else(|| err("no remote key"))?;
    let noise: Noise = Arc::new(Mutex::new(hs.into_transport_mode().map_err(err)?));
    s.set_read_timeout(Some(IDLE))?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let serial = bus.link_up(tx, s.try_clone()?, remote, addr, join);
    let mut w = s.try_clone()?;
    let wn = noise.clone();
    thread::spawn(move || {
        for msg in rx {
            if write_message(&mut w, &wn, &msg).is_err() {
                let _ = w.shutdown(std::net::Shutdown::Both);
                break;
            }
        }
    });
    let mut r = s;
    let result = loop {
        match read_message(&mut r, &noise) {
            Ok(m) => bus.handle(serial, &m),
            Err(e) => break e,
        }
    };
    let _ = r.shutdown(std::net::Shutdown::Both);
    bus.link_down(serial);
    if result.kind() == io::ErrorKind::UnexpectedEof {
        Ok(())
    } else {
        Err(result)
    }
}

fn write_frame(s: &mut impl Write, b: &[u8]) -> io::Result<()> {
    let mut out = Vec::with_capacity(2 + b.len());
    out.extend_from_slice(&(b.len() as u16).to_be_bytes());
    out.extend_from_slice(b);
    s.write_all(&out)
}

fn read_frame(s: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 2];
    s.read_exact(&mut len)?;
    let mut b = vec![0u8; u16::from_be_bytes(len) as usize];
    s.read_exact(&mut b)?;
    Ok(b)
}

fn write_message(s: &mut TcpStream, noise: &Noise, msg: &[u8]) -> io::Result<()> {
    let mut plain = Vec::with_capacity(4 + msg.len());
    plain.extend_from_slice(&(msg.len() as u32).to_be_bytes());
    plain.extend_from_slice(msg);
    let mut out = Vec::new();
    let mut buf = vec![0u8; MAX_NOISE];
    {
        let mut n = noise.lock().unwrap_or_else(|p| p.into_inner());
        for chunk in plain.chunks(MAX_NOISE - TAG) {
            let len = n.write_message(chunk, &mut buf).map_err(err)?;
            out.extend_from_slice(&(len as u16).to_be_bytes());
            out.extend_from_slice(&buf[..len]);
        }
    }
    s.write_all(&out)
}

fn read_message(s: &mut TcpStream, noise: &Noise) -> io::Result<Vec<u8>> {
    let mut plain: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; MAX_NOISE];
    loop {
        let frame = read_frame(s)?;
        let n = noise.lock().unwrap_or_else(|p| p.into_inner()).read_message(&frame, &mut buf).map_err(err)?;
        plain.extend_from_slice(&buf[..n]);
        if plain.len() >= 4 {
            let want = u32::from_be_bytes([plain[0], plain[1], plain[2], plain[3]]) as usize;
            if want > MAX_MESSAGE {
                return Err(err(format!("a message of {want} bytes")));
            }
            if plain.len() >= 4 + want {
                plain.drain(..4);
                plain.truncate(want);
                return Ok(plain);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_reads_back() {
        let mut b = Vec::new();
        write_frame(&mut b, b"hello").unwrap();
        assert_eq!(read_frame(&mut &b[..]).unwrap(), b"hello");
    }
}
