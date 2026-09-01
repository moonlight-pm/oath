//! Bring `net0` up or down with `/bin/ip`.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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
    if nics.is_empty() {
        return Ok(None);
    }
    if nics.iter().any(|n| n == NET0) {
        timed_link_up(NET0);
        let _ = wait_carrier(&[NET0.to_string()], Duration::from_secs(15));
        return Ok(Some(NET0.into()));
    }
    for n in &nics {
        timed_link_up(n);
    }
    let old = wait_carrier(&nics, Duration::from_secs(15)).ok_or_else(|| {
        Error::hint(format!("no carrier on {}", nics.join(", ")), "oath schema net")
    })?;
    if old != NET0 {
        // Rename while up. Do not `ip link set down` — unplugged tg3 can block there.
        ip(&["link", "set", &old, "name", NET0])?;
        timed_link_up(NET0);
    }
    Ok(Some(NET0.into()))
}

fn timed_link_up(nic: &str) {
    let mut child = match Command::new("/bin/ip")
        .args(["link", "set", nic, "up"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return,
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if start.elapsed() > Duration::from_secs(2) => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => return,
        }
    }
}

fn wait_carrier(nics: &[String], timeout: Duration) -> Option<String> {
    let start = Instant::now();
    loop {
        if let Some(n) = nics.iter().find(|n| nic_has_carrier(n)) {
            return Some(n.clone());
        }
        if start.elapsed() > timeout {
            return nics.iter().find(|n| nic_has_carrier(n)).cloned();
        }
        thread::sleep(Duration::from_millis(200));
    }
}

fn nics() -> Result<Vec<String>> {
    let dir = Path::new("/sys/class/net");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut v = Vec::new();
    for e in fs::read_dir(dir)? {
        let n = e?.file_name().to_string_lossy().into_owned();
        if skip_nic(&n) {
            continue;
        }
        v.push(n);
    }
    v.sort();
    Ok(v)
}

fn skip_nic(n: &str) -> bool {
    n == "lo"
        || n.starts_with("docker")
        || n.starts_with("br-")
        || n.starts_with("virbr")
        || n.starts_with("veth")
        || n.starts_with("wl")
        || n.starts_with("wwan")
}

fn nic_has_carrier(n: &str) -> bool {
    fs::read_to_string(format!("/sys/class/net/{n}/carrier"))
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skip_virtual_and_wifi() {
        assert!(skip_nic("lo"));
        assert!(skip_nic("docker0"));
        assert!(skip_nic("wlp13s0"));
        assert!(!skip_nic("enp12s0"));
        assert!(!skip_nic("eth0"));
        assert!(!skip_nic("net0"));
    }
}
