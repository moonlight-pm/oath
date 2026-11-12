//! `oath wake <mac>` (R84): wake another machine on this network, one whose
//! `net:net0.wake` is on, with Wake-on-LAN's magic packet: six 0xff bytes,
//! then its MAC sixteen times, as a UDP broadcast to port 9. Any user may
//! send it.

use std::net::UdpSocket;

use oath_core::{Error, Result};

/// How many times the packet goes: it is one unacknowledged datagram.
pub const SENDS: usize = 3;

pub fn wake(mac: &str) -> Result<()> {
    let packet = magic(&parse_mac(mac)?);
    let send = || -> std::io::Result<()> {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        sock.set_broadcast(true)?;
        for _ in 0..SENDS {
            sock.send_to(&packet, "255.255.255.255:9")?;
        }
        Ok(())
    };
    send().map_err(|e| Error::hint(format!("sending the magic packet: {e}"), "oath get net:net0"))
}

/// `60:cf:84:bc:ff:3e`, or with `-` between the bytes.
fn parse_mac(mac: &str) -> Result<[u8; 6]> {
    let bytes: Vec<u8> = mac
        .split([':', '-'])
        .filter(|b| b.len() == 2)
        .filter_map(|b| u8::from_str_radix(b, 16).ok())
        .collect();
    bytes.try_into().map_err(|_| {
        Error::hint(
            format!("{mac:?} is not a MAC address (six hex bytes, 60:cf:84:bc:ff:3e)"),
            "the machine's own `oath get dev:net0` gives its `mac`",
        )
    })
}

fn magic(mac: &[u8; 6]) -> Vec<u8> {
    let mut p = vec![0xff; 6];
    for _ in 0..16 {
        p.extend_from_slice(mac);
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_magic_packet_is_sync_then_the_mac_sixteen_times() {
        let mac = parse_mac("60:cf:84:bc:ff:3e").unwrap();
        assert_eq!(mac, [0x60, 0xcf, 0x84, 0xbc, 0xff, 0x3e]);
        assert_eq!(parse_mac("60-CF-84-BC-FF-3E").unwrap(), mac);
        let p = magic(&mac);
        assert_eq!(p.len(), 102);
        assert_eq!(p[..6], [0xff; 6]);
        assert!(p[6..].chunks(6).all(|c| c == mac));
    }

    #[test]
    fn anything_else_is_refused() {
        for bad in ["", "koeurax", "60:cf:84:bc:ff", "60:cf:84:bc:ff:3e:00", "60:cf:84:bc:ff:zz", "60cf84bcff3e"] {
            assert!(parse_mac(bad).is_err(), "{bad}");
        }
    }
}
