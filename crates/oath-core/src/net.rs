//! Bring `net0` up or down with `/bin/ip`.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::kinds::{DhcpLease, Net};

pub const NET0: &str = "net0";
pub const APPLIANCE_IPV4: &str = "10.0.2.15/24";
pub const APPLIANCE_GATEWAY: &str = "10.0.2.2";

/// The DHCP client's last lease, written by [`record_lease`]. Gone after
/// a reboot or when the lease is lost.
pub const LEASE_FILE: &str = "/run/oath/net0.lease.json";

/// dhcpcd's hook (R18): it writes `/etc/resolv.conf` and runs `net-lease`.
pub const DHCP_HOOK: &str = "/lib/oath/dhcpcd.hook";
/// Where dhcpcd keeps its DUID and leases, and its pid file.
pub const DHCP_DIRS: [&str; 2] = ["/var/lib/dhcpcd", "/run/dhcpcd"];

/// dhcpcd on `nic`, IPv4 only, with Oath's hook in place of its own (-c).
/// It keeps the address when it stops (-p: a restart must not drop SSH),
/// takes no link-local address (-L), and names itself by the hardware
/// address (-I '', as busybox's udhcpc did, so DHCP reservations still
/// match).
fn dhcpcd_common(nic: &str) -> Vec<String> {
    let opts = "subnet_mask,routers,domain_name_servers,domain_name";
    ["-4", "-p", "-L", "-I", "", "-c", DHCP_HOOK, "-o", opts, nic].map(String::from).to_vec()
}

/// The supervised client of `net:net0` dhcp: in the foreground, renewing.
pub fn dhcpcd_args(nic: &str) -> Vec<String> {
    let mut v = vec!["-B".to_string()];
    v.extend(dhcpcd_common(nic));
    v
}

/// One lease and exit (boot, the installer): up to `timeout` seconds.
pub fn dhcpcd_oneshot(nic: &str, timeout: u32) -> Vec<String> {
    let mut v = vec!["-1".to_string(), "-t".to_string(), timeout.to_string()];
    v.extend(dhcpcd_common(nic));
    v
}

pub fn appliance_desired() -> Net {
    Net {
        up: true,
        ipv4: APPLIANCE_IPV4.into(),
        gateway: APPLIANCE_GATEWAY.into(),
        lease: None,
        dhcp: None,
    }
}

/// The DHCP client's hook (`/lib/oath/net-lease <reason>`, with dhcpcd's
/// environment): keep the lease the client holds in
/// [`LEASE_FILE`] and in `net:net0` actual, so a renewal shows. Called
/// with no catalog lock: an apply that converges `net:net0` waits for
/// the one-shot client that calls this.
pub fn record_lease(root: &Path, reason: &str, env: &dyn Fn(&str) -> Option<String>) -> Result<()> {
    let event = match reason {
        "BOUND" | "REBOOT" => "bound",
        "RENEW" | "REBIND" => "renew",
        // The lease is gone. A stop is not: -p keeps the address.
        "EXPIRE" | "NAK" | "RELEASE" | "NOCARRIER" | "DEPARTED" => "",
        _ => return Ok(()),
    };
    let lease = if event.is_empty() {
        None
    } else {
        let ip = env("new_ip_address").unwrap_or_default();
        if ip.is_empty() {
            return Ok(());
        }
        Some(DhcpLease {
            address: format!("{ip}/{}", env("new_subnet_cidr").unwrap_or_else(|| "32".into())),
            router: env("new_routers")
                .and_then(|r| r.split_whitespace().next().map(str::to_string))
                .unwrap_or_default(),
            dns: env("new_domain_name_servers")
                .map(|d| d.split_whitespace().map(str::to_string).collect())
                .unwrap_or_default(),
            seconds: env("new_dhcp_lease_time").and_then(|l| l.parse().ok()).unwrap_or(0),
            server: env("new_dhcp_server_identifier").unwrap_or_default(),
            event: event.into(),
            at: crate::now_rfc3339(),
        })
    };
    // The run file is for the next converge; actual is what matters.
    match &lease {
        Some(l) => {
            let _ = crate::write_json(Path::new(LEASE_FILE), l);
        }
        None => {
            let _ = fs::remove_file(LEASE_FILE);
        }
    }
    // Before PID 1 first observed net0 there is nothing to update; its
    // converge reads LEASE_FILE.
    let actual = crate::status_path(root, &crate::ObjectId::new("net", "net0"));
    let Ok(mut a) = crate::read_json::<serde_json::Value>(&actual) else { return Ok(()) };
    match lease {
        Some(l) => {
            a["lease"] = serde_json::json!(l.address);
            a["dhcp"] = serde_json::to_value(l)?;
        }
        None => {
            if let Some(m) = a.as_object_mut() {
                m.remove("lease");
                m.remove("dhcp");
            }
        }
    }
    crate::write_json(&actual, &a)
}

fn read_recorded_lease() -> Option<DhcpLease> {
    crate::read_json(Path::new(LEASE_FILE)).ok()
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
            for d in DHCP_DIRS {
                let _ = fs::create_dir_all(d);
            }
            let st = Command::new("/bin/dhcpcd")
                .args(dhcpcd_oneshot(&dev, 10))
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map_err(|e| Error::Msg(format!("dhcpcd: {e}")))?;
            if !st.success() {
                return Err(Error::hint("dhcpcd got no lease", "oath logs net:net0"));
            }
            let mut out = desired.clone();
            out.lease = read_lease(&dev);
            out.dhcp = read_recorded_lease();
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
        .stdout(std::io::stderr())
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
    fn a_renewal_lands_in_actual() {
        let d = tempfile::tempdir().unwrap();
        let actual = crate::status_path(d.path(), &crate::ObjectId::new("net", "net0"));
        crate::write_json(&actual, &serde_json::json!({"up": true, "ipv4": "dhcp"})).unwrap();
        // dhcpcd's names (dhcpcd-run-hooks(8)).
        let env = |k: &str| {
            let v = match k {
                "new_ip_address" => "10.0.2.15",
                "new_subnet_cidr" => "24",
                "new_routers" => "10.0.2.2",
                "new_domain_name_servers" => "10.0.2.3 10.0.2.4",
                "new_dhcp_lease_time" => "86400",
                "new_dhcp_server_identifier" => "10.0.2.2",
                _ => return None,
            };
            Some(v.to_string())
        };
        record_lease(d.path(), "REBIND", &env).unwrap();
        let a: serde_json::Value = crate::read_json(&actual).unwrap();
        assert_eq!(a["lease"], "10.0.2.15/24");
        assert_eq!(a["dhcp"]["event"], "renew");
        assert_eq!(a["dhcp"]["dns"][1], "10.0.2.4");
        assert_eq!(a["dhcp"]["seconds"], 86400);
        assert_eq!(a["dhcp"]["server"], "10.0.2.2");
        // A stop keeps the address (-p), and so the lease.
        record_lease(d.path(), "STOP", &env).unwrap();
        assert!(crate::read_json::<serde_json::Value>(&actual).unwrap().get("lease").is_some());
        record_lease(d.path(), "EXPIRE", &env).unwrap();
        let a: serde_json::Value = crate::read_json(&actual).unwrap();
        assert!(a.get("lease").is_none() && a.get("dhcp").is_none());
    }

    #[test]
    fn dhcpcd_is_ipv4_persistent_and_runs_oaths_hook() {
        let a = dhcpcd_args("net0");
        assert_eq!(a[0], "-B");
        let o = dhcpcd_oneshot("net0", 10);
        assert_eq!(o[..3], ["-1", "-t", "10"]);
        for v in [&a, &o] {
            assert_eq!(v.last().unwrap(), "net0");
            assert!(v.windows(2).any(|w| w == ["-c", DHCP_HOOK]));
            assert!(v.windows(2).any(|w| w == ["-I", ""]));
            assert!(v.iter().any(|x| x == "-p") && v.iter().any(|x| x == "-4"));
        }
    }

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
