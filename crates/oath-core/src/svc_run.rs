//! Who a `svc` runs as, and the credential change PID 1 applies before exec.

use std::io;

use crate::kinds::{Svc, SvcUser};
use crate::seat;

/// `CAP_NET_BIND_SERVICE` — bind a port below 1024.
const CAP_NET_BIND_SERVICE: u32 = 10;
const CAP_SETGID: u32 = 6;
const CAP_SETUID: u32 = 7;
const CAPABILITY_VERSION_3: u32 = 0x20080522;

#[repr(C)]
struct CapHeader {
    version: u32,
    pid: i32,
}

#[repr(C)]
struct CapData {
    effective: u32,
    permitted: u32,
    inheritable: u32,
}

/// Credentials applied in the child before exec.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SvcCred {
    pub user: SvcUser,
    pub uid: u32,
    pub gid: u32,
    /// Grant `CAP_NET_BIND_SERVICE` to a non-root process. Always false for root.
    pub bind_low_ports: bool,
}

/// How PID 1 spawns one service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SvcSpawn {
    pub cred: SvcCred,
    pub home: &'static str,
    pub user_name: &'static str,
    pub shell: &'static str,
    pub xdg_runtime_dir: String,
}

/// Effective account for `spec`: the `user` field, or `root` when absent.
pub fn svc_spawn(_id: &str, spec: &Svc) -> SvcSpawn {
    let user = spec.user.unwrap_or(SvcUser::Root);
    let as_home = user == SvcUser::Home;
    SvcSpawn {
        cred: SvcCred {
            user,
            uid: user.uid(),
            gid: user.gid(),
            bind_low_ports: as_home && spec.bind_low_ports,
        },
        home: if as_home { seat::HOME } else { "/root" },
        user_name: if as_home { seat::NAME } else { "root" },
        shell: "/bin/sh",
        xdg_runtime_dir: format!("/run/user/{}", user.uid()),
    }
}

/// `svc.env` entries PID 1 puts in the child. Invalid names are skipped,
/// same rule as `host:local.env`.
pub fn svc_env_pairs(spec: &Svc) -> Vec<(String, String)> {
    spec.env
        .iter()
        .filter(|(key, value)| {
            seat::valid_env_name(key) && !value.contains('\n') && !value.contains('\0')
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// Drop to the service account. Root returns immediately.
///
/// A home process gets `CAP_NET_BIND_SERVICE` only when `bind_low_ports`
/// is set. The kernel clears the ambient set on the uid change even if
/// permitted is kept, so the ambient bit is raised after `setuid`. Exec
/// of an ordinary binary then keeps the capability.
pub fn apply_svc_cred(cred: &SvcCred) -> io::Result<()> {
    if cred.user == SvcUser::Root {
        return Ok(());
    }
    if cred.bind_low_ports {
        let switch = bit(CAP_SETGID) | bit(CAP_SETUID) | bit(CAP_NET_BIND_SERVICE);
        // Inheritable is only the low-port cap. SETUID/SETGID stay effective
        // long enough to change uid, then the post-drop capset removes them.
        capset(switch, switch, bit(CAP_NET_BIND_SERVICE))?;
        prctl_keepcaps()?;
    }
    let gid: libc::gid_t = cred.gid;
    if unsafe { libc::setgroups(1, &gid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::setgid(gid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::setuid(cred.uid) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if cred.bind_low_ports {
        let bind = bit(CAP_NET_BIND_SERVICE);
        capset(bind, bind, bind)?;
        raise_ambient_bind()?;
    }
    Ok(())
}

fn bit(cap: u32) -> u32 {
    1u32 << cap
}

fn prctl_keepcaps() -> io::Result<()> {
    let rc = unsafe { libc::prctl(libc::PR_SET_KEEPCAPS, 1u64, 0, 0, 0) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn raise_ambient_bind() -> io::Result<()> {
    let rc = unsafe {
        libc::prctl(
            libc::PR_CAP_AMBIENT,
            libc::PR_CAP_AMBIENT_RAISE,
            CAP_NET_BIND_SERVICE as libc::c_ulong,
            0,
            0,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn capset(permitted: u32, effective: u32, inheritable: u32) -> io::Result<()> {
    let mut version = CAPABILITY_VERSION_3;
    for _ in 0..2 {
        let mut hdr = CapHeader { version, pid: 0 };
        let data = [
            CapData { effective, permitted, inheritable },
            CapData { effective: 0, permitted: 0, inheritable: 0 },
        ];
        let rc = unsafe { libc::syscall(libc::SYS_capset, &mut hdr, data.as_ptr()) };
        if rc == 0 {
            return Ok(());
        }
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EINVAL) && hdr.version != version && hdr.version != 0 {
            version = hdr.version;
            continue;
        }
        return Err(err);
    }
    Err(io::Error::last_os_error())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::os::unix::process::CommandExt;

    use super::*;
    use crate::kinds::SvcRestart;

    fn bare(exec: &[&str]) -> Svc {
        Svc {
            exec: exec.iter().map(|s| (*s).to_string()).collect(),
            wants: Vec::new(),
            restart: SvcRestart::Never,
            enabled: true,
            user: None,
            env: BTreeMap::new(),
            bind_low_ports: false,
        }
    }

    #[test]
    fn default_user_is_root() {
        let spec = bare(&["/bin/sleep", "1"]);
        let spawn = svc_spawn("svc:hold", &spec);
        assert_eq!(spawn.cred.user, SvcUser::Root);
        assert_eq!(spawn.cred.uid, 0);
        assert_eq!(spawn.cred.gid, 0);
        assert!(!spawn.cred.bind_low_ports);
        assert_eq!(spawn.home, "/root");
        assert_eq!(spawn.user_name, "root");
    }

    #[test]
    fn user_home_is_uid_1() {
        let mut spec = bare(&["/bin/app"]);
        spec.user = Some(SvcUser::Home);
        let spawn = svc_spawn("svc:app", &spec);
        assert_eq!(spawn.cred.user, SvcUser::Home);
        assert_eq!(spawn.cred.uid, 1);
        assert_eq!(spawn.cred.uid, seat::UID);
        assert_eq!(spawn.cred.gid, seat::GID);
        assert_eq!(spawn.home, "/home");
        assert_eq!(spawn.user_name, "home");
        assert!(!spawn.cred.bind_low_ports);
    }

    #[test]
    fn absent_user_is_root_and_explicit_user_wins() {
        let spec = bare(&["/bin/app"]);
        assert_eq!(svc_spawn("svc:app", &spec).cred.uid, 0);
        let mut home = spec.clone();
        home.user = Some(SvcUser::Home);
        assert_eq!(svc_spawn("svc:app", &home).cred.uid, 1);
    }

    #[test]
    fn bind_low_ports_only_for_non_root() {
        let mut home = bare(&["/bin/caddy"]);
        home.user = Some(SvcUser::Home);
        assert!(!svc_spawn("svc:caddy", &home).cred.bind_low_ports);
        home.bind_low_ports = true;
        assert!(svc_spawn("svc:caddy", &home).cred.bind_low_ports);

        let mut root = bare(&["/bin/caddy"]);
        root.user = Some(SvcUser::Root);
        root.bind_low_ports = true;
        let spawn = svc_spawn("svc:caddy", &root);
        assert_eq!(spawn.cred.user, SvcUser::Root);
        assert!(!spawn.cred.bind_low_ports);
    }

    #[test]
    fn env_pairs_skip_invalid_names() {
        let mut spec = bare(&["/bin/true"]);
        spec.env.insert("APP_STATE_DIR".into(), "/var/lib/app".into());
        spec.env.insert("BAD-NAME".into(), "x".into());
        spec.env.insert("HAS_NL".into(), "a\nb".into());
        let pairs = svc_env_pairs(&spec);
        assert_eq!(pairs, vec![("APP_STATE_DIR".into(), "/var/lib/app".into())]);
    }

    #[test]
    fn live_child_is_uid_1_sees_env_and_low_port_cap() {
        if std::env::var_os("OATH_SVC_PROBE").is_some() {
            write_probe();
            std::process::exit(0);
        }
        if unsafe { libc::geteuid() } != 0 {
            let exe = std::env::current_exe().unwrap();
            let thread = std::thread::current();
            let name = thread.name().unwrap_or("");
            assert!(!name.is_empty(), "libtest thread name missing");
            let out = std::process::Command::new("sudo")
                .args(["-n", "--"])
                .arg(&exe)
                .args(["--exact", name, "--nocapture"])
                .output()
                .expect("sudo -n");
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !out.status.success() || !format!("{stdout}{stderr}").contains("running 1 test") {
                panic!("live drop failed\nstdout:\n{stdout}\nstderr:\n{stderr}");
            }
            return;
        }
        assert_child(SvcUser::Home, false);
        assert_child(SvcUser::Home, true);
        assert_child(SvcUser::Root, false);
        assert_child(SvcUser::Root, true);
    }

    fn assert_child(user: SvcUser, bind_flag: bool) {
        let mut spec = bare(&["/bin/caddy"]);
        spec.user = Some(user);
        spec.bind_low_ports = bind_flag;
        spec.env.insert("OATH_MARK".into(), "visible".into());
        let plan = svc_spawn("svc:caddy", &spec);
        // `/proc/self/exe` stays reachable after the uid drop. The build
        // tree under a 0700 home directory is not.
        let mut cmd = std::process::Command::new("/proc/self/exe");
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("");
        assert!(!name.is_empty(), "libtest thread name missing");
        cmd.args(["--exact", name, "--nocapture"])
            .env("OATH_SVC_PROBE", "1")
            .stdin(std::process::Stdio::null());
        for (key, value) in svc_env_pairs(&spec) {
            cmd.env(key, value);
        }
        let cred = plan.cred.clone();
        unsafe {
            cmd.pre_exec(move || apply_svc_cred(&cred));
        }
        let out =
            cmd.output().unwrap_or_else(|err| panic!("spawn {user:?} bind={bind_flag}: {err}"));
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.status.success(), "{user:?} bind={bind_flag}\n{text}");
        let got = probe_map(&text);
        assert_eq!(got.get("mark").map(String::as_str), Some("visible"), "{text}");
        let uid: u32 = got["uid"].parse().unwrap();
        assert_eq!(uid, user.uid(), "{text}");
        let prm = parse_hex(got.get("prm").map(String::as_str).unwrap_or(""));
        let eff = parse_hex(got.get("eff").map(String::as_str).unwrap_or(""));
        let amb = parse_hex(got.get("amb").map(String::as_str).unwrap_or(""));
        let bind_bit = 1u128 << CAP_NET_BIND_SERVICE;
        let setuid_bit = 1u128 << CAP_SETUID;
        if user == SvcUser::Home {
            assert_eq!(prm & setuid_bit, 0, "home kept CAP_SETUID\n{text}");
            if bind_flag {
                assert_ne!(prm & bind_bit, 0, "{text}");
                assert_ne!(eff & bind_bit, 0, "{text}");
                assert_ne!(amb & bind_bit, 0, "{text}");
                assert!(got["bind"].starts_with("ok:"), "{text}");
            } else {
                assert_eq!(prm & bind_bit, 0, "{text}");
                assert_eq!(eff & bind_bit, 0, "{text}");
                assert_eq!(amb & bind_bit, 0, "{text}");
                assert_eq!(got["bind"].as_str(), "denied", "{text}");
            }
        } else {
            assert_eq!(uid, 0, "{text}");
            assert_ne!(eff & bind_bit, 0, "root lost the low-port cap\n{text}");
        }
    }

    fn probe_map(text: &str) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        for line in text.lines() {
            let Some(rest) = line.strip_prefix("OATH_PROBE ") else { continue };
            if let Some((key, value)) = rest.split_once('=') {
                map.insert(key.to_string(), value.to_string());
            }
        }
        map
    }

    fn parse_hex(hex: &str) -> u128 {
        u128::from_str_radix(hex, 16).unwrap_or(0)
    }

    fn write_probe() {
        use std::io::Write;
        let uid = unsafe { libc::getuid() };
        let mark = std::env::var("OATH_MARK").unwrap_or_default();
        let mut out = std::io::stdout();
        let _ = writeln!(out, "OATH_PROBE uid={uid}");
        let _ = writeln!(out, "OATH_PROBE mark={mark}");
        let _ = writeln!(out, "OATH_PROBE prm={:x}", cap_field("CapPrm:"));
        let _ = writeln!(out, "OATH_PROBE eff={:x}", cap_field("CapEff:"));
        let _ = writeln!(out, "OATH_PROBE amb={:x}", cap_field("CapAmb:"));
        let _ = writeln!(out, "OATH_PROBE bind={}", bind_low());
        let _ = out.flush();
    }

    fn cap_field(name: &str) -> u128 {
        let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        for line in status.lines() {
            let Some(rest) = line.strip_prefix(name) else { continue };
            let hex = rest.trim().trim_start_matches(':').trim();
            return u128::from_str_radix(hex, 16).unwrap_or(0);
        }
        0
    }

    fn bind_low() -> String {
        let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return format!("socket:{}", std::io::Error::last_os_error());
        }
        let one: libc::c_int = 1;
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                &one as *const libc::c_int as *const libc::c_void,
                std::mem::size_of_val(&one) as libc::socklen_t,
            );
        }
        let mut saw_denied = false;
        let mut saw_busy = false;
        for port in [1u16, 79, 81, 88, 99, 113] {
            let addr = libc::sockaddr_in {
                sin_family: libc::AF_INET as libc::sa_family_t,
                sin_port: port.to_be(),
                sin_addr: libc::in_addr { s_addr: libc::htonl(0x7f000001) },
                sin_zero: [0; 8],
            };
            let rc = unsafe {
                libc::bind(
                    fd,
                    &addr as *const libc::sockaddr_in as *const libc::sockaddr,
                    std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
                )
            };
            if rc == 0 {
                unsafe { libc::close(fd) };
                return format!("ok:{port}");
            }
            let err = std::io::Error::last_os_error();
            match err.raw_os_error() {
                Some(e) if e == libc::EACCES || e == libc::EPERM => saw_denied = true,
                Some(e) if e == libc::EADDRINUSE => saw_busy = true,
                _ => {
                    unsafe { libc::close(fd) };
                    return format!("err:{err}");
                }
            }
        }
        unsafe { libc::close(fd) };
        if saw_denied {
            "denied".into()
        } else if saw_busy {
            "busy".into()
        } else {
            "none".into()
        }
    }

    #[test]
    fn old_svc_document_parses_and_round_trips() {
        let raw = r#"{"exec":["/bin/sleep","86400000"],"wants":["svc:serial"],"restart":"always","enabled":true}"#;
        let spec: Svc = serde_json::from_str(raw).unwrap();
        assert!(spec.user.is_none());
        assert!(spec.env.is_empty());
        assert!(!spec.bind_low_ports);
        assert_eq!(svc_spawn("svc:hold", &spec).cred.user, SvcUser::Root);
        let back = serde_json::to_value(&spec).unwrap();
        assert!(back.get("user").is_none());
        assert!(back.get("env").is_none());
        assert!(back.get("bind_low_ports").is_none());
        let bad =
            r#"{"exec":["/bin/true"],"wants":[],"restart":"never","enabled":true,"user":"app"}"#;
        assert!(serde_json::from_str::<Svc>(bad).is_err());
        let bad = r#"{"exec":["/bin/true"],"wants":[],"restart":"never","enabled":true,"user":"postgres"}"#;
        assert!(serde_json::from_str::<Svc>(bad).is_err());
    }
}
