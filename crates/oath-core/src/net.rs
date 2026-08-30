//! Bring `net0` up or down with `/bin/ip`.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};
use crate::kinds::Net;

pub const NET0: &str = "net0";
pub const APPLIANCE_IPV4: &str = "10.0.2.15/24";
pub const APPLIANCE_GATEWAY: &str = "10.0.2.2";

pub fn appliance_desired() -> Net {
    Net { up: true, ipv4: APPLIANCE_IPV4.into(), gateway: APPLIANCE_GATEWAY.into(), lease: None }
}

/// Configure the sole non-loopback NIC as `net0`.
pub fn converge(desired: &Net) -> Result<Net> {
    let dev = match find_or_rename_net0()? {
        Some(d) => d,
        None => {
            if desired.up {
                return Err(Error::hint("no network interface", "oath schema net"));
            }
            return Ok(desired.clone());
        }
    };
    if desired.up {
        ip(&["link", "set", &dev, "up"])?;
        if desired.ipv4 == "dhcp" {
            let st = Command::new("/bin/udhcpc")
                .args([
                    "-i",
                    &dev,
                    "-n",
                    "-q",
                    "-f",
                    "-s",
                    "/usr/lib/oath/udhcpc.script",
                    "-T",
                    "2",
                    "-t",
                    "5",
                ])
                .status()
                .map_err(|e| Error::Msg(format!("udhcpc: {e}")))?;
            if !st.success() {
                return Err(Error::hint("udhcpc failed", "oath schema net"));
            }
            let mut out = desired.clone();
            out.lease = read_lease(&dev);
            return Ok(out);
        }
        ip(&["addr", "flush", "dev", &dev])?;
        ip(&["addr", "add", &desired.ipv4, "dev", &dev])?;
        if !desired.gateway.is_empty() {
            let _ = ip(&["route", "del", "default"]);
            ip(&["route", "add", "default", "via", &desired.gateway])?;
        }
    } else {
        let _ = ip(&["route", "del", "default"]);
        let _ = ip(&["addr", "flush", "dev", &dev]);
        ip(&["link", "set", &dev, "down"])?;
    }
    Ok(desired.clone())
}

fn find_or_rename_net0() -> Result<Option<String>> {
    let nics = nics()?;
    if nics.iter().any(|n| n == NET0) {
        return Ok(Some(NET0.into()));
    }
    if nics.len() == 1 {
        let old = &nics[0];
        let _ = ip(&["link", "set", old, "down"]);
        ip(&["link", "set", old, "name", NET0])?;
        return Ok(Some(NET0.into()));
    }
    if nics.is_empty() {
        return Ok(None);
    }
    Err(Error::hint(format!("expected one NIC, found {}", nics.join(", ")), "oath schema net"))
}

fn nics() -> Result<Vec<String>> {
    let dir = Path::new("/sys/class/net");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut v = Vec::new();
    for e in fs::read_dir(dir)? {
        let n = e?.file_name().to_string_lossy().into_owned();
        if n != "lo" {
            v.push(n);
        }
    }
    v.sort();
    Ok(v)
}

fn read_lease(dev: &str) -> Option<String> {
    let o = Command::new("/bin/ip").args(["-o", "-4", "addr", "show", "dev", dev]).output().ok()?;
    let s = String::from_utf8_lossy(&o.stdout);
    s.split_whitespace().find(|w| w.contains('/') && !w.contains(':')).map(|w| w.to_string())
}

fn ip(args: &[&str]) -> Result<()> {
    let st = Command::new("/bin/ip")
        .args(args)
        .status()
        .map_err(|e| Error::Msg(format!("ip {}: {e}", args.join(" "))))?;
    if !st.success() {
        return Err(Error::Msg(format!("ip {} failed", args.join(" "))));
    }
    Ok(())
}
