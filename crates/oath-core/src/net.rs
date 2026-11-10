//! Bring `net0` up or down with `/bin/ip`.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::kinds::{DhcpLease, Net};

pub const NET0: &str = "net0";
const IP: &str = "/bin/ip";
pub const APPLIANCE_IPV4: &str = "10.0.2.15/24";
pub const APPLIANCE_GATEWAY: &str = "10.0.2.2";

/// The DHCP client's last lease, written by [`record_lease`]. Gone after
/// a reboot or when the lease is lost.
pub const LEASE_FILE: &str = "/run/oath/net0.lease.json";

/// dhcpcd's hook (R18): it writes `/etc/resolv.conf` and runs `net-lease`.
pub const DHCP_HOOK: &str = "/lib/oath/dhcpcd.hook";
/// The DNS servers: from the lease for `dhcp` (the hook), from `dns` for a
/// static address (converge).
const RESOLV_CONF: &str = "/etc/resolv.conf";
/// Where dhcpcd keeps its DUID and leases, and its pid file.
pub const DHCP_DIRS: [&str; 2] = ["/var/lib/dhcpcd", "/run/dhcpcd"];

/// dhcpcd on `nic`, IPv4 only, with Oath's hook in place of its own (-c)
/// and no config file (-f /dev/null: every option is here). It keeps the
/// address when it stops (-p: a restart must not drop SSH), takes no
/// link-local address (-L), and names itself by the hardware address
/// (-I ''), so DHCP reservations made for the machine match. Being IPv4
/// only, it keeps `/run/dhcpcd/<nic>-4.pid`; `dhcpcd -4 -N <nic>` renews.
fn dhcpcd_common(nic: &str) -> Vec<String> {
    let opts = "subnet_mask,routers,domain_name_servers,domain_name";
    ["-f", "/dev/null", "-4", "-p", "-L", "-I", "", "-c", DHCP_HOOK, "-o", opts, nic]
        .map(String::from)
        .to_vec()
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
        dns: Vec::new(),
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
        for (args, must) in ipv4_steps(desired, &dev, &Link::read(&dev)) {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            if must {
                ip(&args)?;
            } else {
                quietly(IP, &args);
            }
        }
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
        // Converge runs at apply and again at every boot (PID 1), as
        // /etc/hosts is written, so the servers hold after a reboot.
        if !desired.dns.is_empty() {
            crate::write_atomic(Path::new(RESOLV_CONF), resolv_conf(desired).as_bytes())?;
        }
    } else {
        quietly(IP, &["route", "del", "default"]);
        quietly(IP, &["addr", "flush", "dev", &dev]);
        ip(&["link", "set", &dev, "down"])?;
    }
    Ok(desired.clone())
}

/// The `ip` commands that give `dev` the IPv4 address `desired` names, run
/// before a DHCP client starts for `dhcp`; each is false when its failure
/// does not matter. They touch IPv4 only: the link-local and
/// router-advertised IPv6 addresses stay. There are none for a static
/// address `link` (what `dev` holds now) already has.
fn ipv4_steps(desired: &Net, dev: &str, link: &Link) -> Vec<(Vec<String>, bool)> {
    let step = |args: &[&str], must: bool| {
        (args.iter().map(|a| a.to_string()).collect::<Vec<String>>(), must)
    };
    if desired.ipv4 == "dhcp" {
        // What a static address left goes, or its default route outranks
        // the lease's: the address with no lifetime, and the routes `ip
        // route add` made (proto boot). dhcpcd's address carries its
        // lease's lifetime and its routes are proto dhcp, so a converge that
        // stays dhcp keeps its lease.
        return vec![
            step(&["-4", "addr", "flush", "dev", dev, "permanent"], true),
            step(&["-4", "route", "flush", "dev", dev, "proto", "boot"], true),
        ];
    }
    // A change to `dns` alone, or an apply that changes nothing, keeps the
    // address on the link: the steps take it off before they put it back.
    if link.holds(desired, dev) {
        return Vec::new();
    }
    let mut steps = vec![
        step(&["-4", "addr", "flush", "dev", dev], true),
        step(&["addr", "add", &desired.ipv4, "dev", dev], true),
    ];
    if !desired.gateway.is_empty() {
        steps.push(step(&["route", "del", "default"], false));
        steps.push(step(&["route", "add", "default", "via", &desired.gateway], true));
    }
    steps
}

/// What a link holds of IPv4, as `ip` prints it: its addresses (`ip -4 -o
/// addr show dev <dev>`) and the default routes (`ip -4 route show
/// default`). Both are empty at boot.
#[derive(Default)]
struct Link {
    addrs: String,
    routes: String,
}

impl Link {
    fn read(dev: &str) -> Link {
        let out = |args: &[&str]| {
            Command::new(IP)
                .args(args)
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default()
        };
        Link {
            addrs: out(&["-4", "-o", "addr", "show", "dev", dev]),
            routes: out(&["-4", "route", "show", "default"]),
        }
    }

    /// Whether `dev` is as the static steps leave it for `desired`: one
    /// IPv4 address, `desired`'s, permanent (a lease's is `dynamic`), and
    /// one default route, `ip route add`'s via `gateway`, or none on `dev`
    /// when `gateway` is empty. Anything else takes the steps.
    fn holds(&self, desired: &Net, dev: &str) -> bool {
        let address = match words(&self.addrs).as_slice() {
            [w] if !w.contains(&"dynamic") => w.windows(2).find(|p| p[0] == "inet").map(|p| p[1]),
            _ => None,
        };
        if address != Some(desired.ipv4.as_str()) {
            return false;
        }
        let routes = words(&self.routes);
        if desired.gateway.is_empty() {
            return !routes.iter().any(|w| w.windows(2).any(|p| p == ["dev", dev]));
        }
        routes == [["default", "via", desired.gateway.as_str(), "dev", dev]]
    }
}

/// Each line of `ip`'s output as its words, blank lines left out.
fn words(out: &str) -> Vec<Vec<&str>> {
    out.lines()
        .map(|l| l.split_whitespace().collect::<Vec<&str>>())
        .filter(|w| !w.is_empty())
        .collect()
}

/// `/etc/resolv.conf` for a static address: a nameserver line for each of
/// `dns`.
fn resolv_conf(net: &Net) -> String {
    let mut body = String::from("# generated by oath from net:net0 (oath set net:net0 dns=...)\n");
    for server in &net.dns {
        body.push_str(&format!("nameserver {server}\n"));
    }
    body
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
    let mut child = match Command::new(IP)
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
    let o = Command::new(IP).args(["-o", "-4", "addr", "show", "dev", dev]).output().ok()?;
    let s = String::from_utf8_lossy(&o.stdout);
    s.split_whitespace().find(|w| w.contains('/') && !w.contains(':')).map(|w| w.to_string())
}

fn ip(args: &[&str]) -> Result<()> {
    let st = Command::new(IP)
        .args(args)
        .stdout(std::io::stderr())
        .status()
        .map_err(|e| Error::Msg(format!("ip {}: {e}", args.join(" "))))?;
    if !st.success() {
        return Err(Error::Msg(format!("ip {} failed", args.join(" "))));
    }
    Ok(())
}

/// Run `bin` (`ip`) for a step whose failure is expected and ignored, as
/// `route del default` once the flush has taken the route with the
/// address: what it prints would read as a fault, so it is dropped.
fn quietly(bin: &str, args: &[&str]) {
    let _ = Command::new(bin).args(args).output();
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

    fn net(ipv4: &str, gateway: &str, dns: &[&str]) -> Net {
        Net {
            up: true,
            ipv4: ipv4.into(),
            gateway: gateway.into(),
            dns: dns.iter().map(|s| s.to_string()).collect(),
            lease: None,
            dhcp: None,
        }
    }

    #[test]
    fn back_to_dhcp_takes_the_static_address_and_route_away() {
        // A VM, 2026-11-10: a bad static address undone to dhcp kept it and
        // its metric-0 default route, so traffic still took the bad gateway.
        let steps = ipv4_steps(&net("dhcp", "", &[]), "net0", &Link::default());
        let cmds: Vec<String> = steps.iter().map(|(a, _)| a.join(" ")).collect();
        assert_eq!(
            cmds,
            ["-4 addr flush dev net0 permanent", "-4 route flush dev net0 proto boot"]
        );
        assert!(steps.iter().all(|(_, must)| *must));
    }

    #[test]
    fn a_static_address_leaves_ipv6_alone() {
        let steps = ipv4_steps(&net("10.0.0.3/24", "10.0.0.1", &[]), "net0", &Link::default());
        let cmds: Vec<String> = steps.iter().map(|(a, _)| a.join(" ")).collect();
        assert_eq!(
            cmds,
            [
                "-4 addr flush dev net0",
                "addr add 10.0.0.3/24 dev net0",
                "route del default",
                "route add default via 10.0.0.1",
            ]
        );
        // Every flush is IPv4's alone: link-local and RA addresses stay.
        assert!(cmds.iter().filter(|c| c.contains("flush")).all(|c| c.starts_with("-4 ")));
        let musts: Vec<bool> = steps.iter().map(|(_, must)| *must).collect();
        assert_eq!(musts, [true, true, false, true]);
        assert_eq!(ipv4_steps(&net("10.0.0.3/24", "", &[]), "net0", &Link::default()).len(), 2);
    }

    #[test]
    fn a_dns_change_leaves_the_address_on_the_link() {
        // A VM, 2026-11-10: a change to `dns` alone, which needs no
        // --confirm, flushed the static address and put it back.
        let link = Link {
            addrs: inet("10.0.0.3/24", "", "forever"),
            routes: "default via 10.0.0.1 dev net0 \n".into(),
        };
        let dns = net("10.0.0.3/24", "10.0.0.1", &["10.0.0.1"]);
        assert!(ipv4_steps(&dns, "net0", &link).is_empty());
        // A new address or gateway, or none, takes the steps.
        assert_eq!(ipv4_steps(&net("10.0.0.4/24", "10.0.0.1", &[]), "net0", &link).len(), 4);
        assert_eq!(ipv4_steps(&net("10.0.0.3/24", "10.0.0.254", &[]), "net0", &link).len(), 4);
        assert_eq!(ipv4_steps(&net("10.0.0.3/24", "", &[]), "net0", &link).len(), 2);
        let no_route = Link { addrs: link.addrs.clone(), routes: String::new() };
        assert!(ipv4_steps(&net("10.0.0.3/24", "", &[]), "net0", &no_route).is_empty());
        // A lease's address (dynamic) is made permanent, and a second one goes.
        let lease = Link {
            addrs: inet("10.0.0.3/24", "dynamic noprefixroute ", "36688sec"),
            routes: "default via 10.0.0.1 dev net0 proto dhcp src 10.0.0.3 metric 1002 \n".into(),
        };
        assert_eq!(ipv4_steps(&dns, "net0", &lease).len(), 4);
        let two = Link { addrs: link.addrs.clone() + &inet("10.0.0.9/24", "", "forever"), ..link };
        assert_eq!(ipv4_steps(&dns, "net0", &two).len(), 4);
    }

    /// A line of `ip -4 -o addr show dev net0`.
    fn inet(address: &str, flags: &str, lifetime: &str) -> String {
        format!(
            "3: net0    inet {address} scope global {flags}net0\\       \
             valid_lft {lifetime} preferred_lft {lifetime}\n"
        )
    }

    #[test]
    fn an_ignored_step_says_nothing_to_the_user() {
        // A VM, 2026-11-10: every static converge printed `RTNETLINK
        // answers: No such process` from `route del default`, which runs
        // after the flush has taken the route.
        let d = tempfile::tempdir().unwrap();
        let fd2 = d.path().join("fd2");
        let script = format!(
            "readlink /proc/self/fd/2 > {}; echo 'RTNETLINK answers: No such process' >&2; exit 2",
            fd2.display()
        );
        quietly("/bin/sh", &["-c", &script]);
        let user = fs::read_link("/proc/self/fd/2").unwrap();
        let its = fs::read_to_string(&fd2).unwrap();
        assert_ne!(Path::new(its.trim()), user, "the step's stderr is the user's");
    }

    #[test]
    fn static_dns_is_one_nameserver_line_each() {
        let body = resolv_conf(&net("10.0.0.3/24", "10.0.0.1", &["10.0.0.1", "fd00::53"]));
        let lines: Vec<&str> = body.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(lines, ["nameserver 10.0.0.1", "nameserver fd00::53"]);
        assert!(body.starts_with("# generated by oath from net:net0"), "{body}");
        // Not written to desired or actual when empty.
        let doc = serde_json::to_value(net("dhcp", "", &[])).unwrap();
        assert!(doc.get("dns").is_none(), "{doc}");
        let seeded = serde_json::json!({"up": true, "ipv4": "dhcp"});
        assert!(serde_json::from_value::<Net>(seeded).unwrap().dns.is_empty());
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
