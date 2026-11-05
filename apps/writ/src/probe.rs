//! The attacks, from inside a box. `writ check` starts this and judges the lines it prints.
//! Each attack calls the kernel directly, so a missing tool cannot pass for a refusal, and
//! with arguments that do nothing even if it gets through.
//!
//! It refuses to run outside a box, where its attempts would be real.

use crate::{landlock, seccomp, sys};
use std::ffi::{c_int, c_long, OsString};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

const SIGSYS: i32 = 31;

pub struct Ctx {
    pub canary: String,
    pub canary_text: String,
    pub env_text: String,
    pub host_pid: i32,
    pub abstract_name: String,
    pub marker: String,
    pub absent: Vec<String>,
    pub project: String,
}

fn emit(name: &str, state: &str, detail: &str) {
    let clean = |s: &str| s.replace(['\t', '\n'], " ");
    println!("{}\t{state}\t{}", clean(name), clean(detail));
}

pub fn refused(name: &str, detail: &str) {
    emit(name, "refused", detail);
}
pub fn leak(name: &str, detail: &str) {
    emit(name, "LEAK", detail);
}
pub fn works(name: &str) {
    emit(name, "works", "");
}
pub fn broken(name: &str, detail: &str) {
    emit(name, "BROKEN", detail);
}
pub fn skip(name: &str, detail: &str) {
    emit(name, "skip", detail);
}

fn inside_a_box() -> bool {
    // The kernel refuses these if unused arguments are not zero, and a variadic call with
    // none leaves whatever was in the registers.
    unsafe {
        sys::prctl(sys::PR_GET_SECCOMP, 0i64, 0i64, 0i64, 0i64) == 2
            && sys::prctl(sys::PR_GET_NO_NEW_PRIVS, 0i64, 0i64, 0i64, 0i64) == 1
    }
}

fn guard() {
    if !inside_a_box() {
        eprintln!("writ probe runs only inside a box; `writ check` starts it");
        std::process::exit(2);
    }
}

fn absent(path: &str) {
    let name = format!("no {path}");
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == ErrorKind::NotFound => refused(&name, "not there"),
        Err(e) => refused(&name, &e.to_string()),
        Ok(_) => leak(&name, "it is in the box"),
    }
}

fn cannot_read_canary(name: &str, path: &str, cx: &Ctx) {
    match fs::read(path) {
        Err(e) => refused(name, &e.to_string()),
        Ok(b) if String::from_utf8_lossy(&b).contains(&cx.canary_text) => leak(name, "read the canary"),
        Ok(_) => refused(name, "read something, not the canary"),
    }
}

fn cannot_write(path: &str) {
    let name = format!("write {path}");
    let opened = fs::OpenOptions::new().write(true).create(true).truncate(false).open(path);
    match opened {
        Err(e) => refused(&name, &e.to_string()),
        Ok(_) => leak(&name, "opened for writing"),
    }
}

fn cannot_open_for_write(path: &str) {
    let name = format!("open {path} to write");
    match fs::OpenOptions::new().write(true).open(path) {
        Err(e) => refused(&name, &e.to_string()),
        Ok(_) => leak(&name, "opened for writing"),
    }
}

fn state() {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let field = |f: &str| status.lines().find_map(|l| l.strip_prefix(f)).map(|v| v.trim().to_string());
    let expect = |name: &str, f: &str, want: &str| match field(f) {
        Some(v) if v == want => refused(name, &format!("{f} {v}")),
        v => leak(name, &format!("{f} {v:?}")),
    };
    expect("no_new_privs is set", "NoNewPrivs:", "1");
    expect("seccomp filter is on", "Seccomp:", "2");
    for f in ["CapInh:", "CapPrm:", "CapEff:", "CapAmb:"] {
        expect("no capabilities", f, "0000000000000000");
    }
    for fd in 0..3 {
        if unsafe { sys::isatty(fd) } == 1 {
            leak("no terminal", &format!("fd {fd} is one"));
        }
    }
    match fs::File::open("/dev/tty") {
        Err(e) => refused("no controlling terminal", &e.to_string()),
        Ok(_) => leak("no controlling terminal", "opened /dev/tty"),
    }
}

fn no_sudo() {
    for path in ["/bin/sudo", "/lib/oath/sudo"] {
        let name = format!("{path} to root");
        if !Path::new(path).exists() {
            skip(&name, "not on this machine");
            continue;
        }
        match Command::new(path).args(["/bin/id", "-u"]).stdin(Stdio::null()).output() {
            Err(e) => refused(&name, &e.to_string()),
            Ok(o) => {
                let out = String::from_utf8_lossy(&o.stdout);
                if out.trim() == "0" {
                    leak(&name, "ran as root");
                } else {
                    let why = String::from_utf8_lossy(&o.stderr);
                    refused(&name, why.trim());
                }
            }
        }
    }
}

fn no_network() {
    let net = fs::read_to_string("/proc/net/dev").unwrap_or_default();
    let devices: Vec<&str> = net.lines().skip(2).filter_map(|l| l.split(':').next()).map(str::trim).collect();
    if devices.iter().all(|d| *d == "lo") {
        refused("only loopback", &format!("{devices:?}"));
    } else {
        leak("only loopback", &format!("{devices:?}"));
    }
    let routes = fs::read_to_string("/proc/net/route").unwrap_or_default().lines().count();
    if routes <= 1 {
        refused("no routes", "");
    } else {
        leak("no routes", &format!("{} lines", routes - 1));
    }
    for addr in ["1.1.1.1:443", "10.0.0.1:22", "[2606:4700:4700::1111]:443"] {
        let name = format!("connect {addr}");
        let a = addr.parse().expect("an address");
        match TcpStream::connect_timeout(&a, Duration::from_secs(2)) {
            Err(e) => refused(&name, &e.to_string()),
            Ok(_) => leak(&name, "connected"),
        }
    }
    match "openrouter.ai:443".to_socket_addrs() {
        Err(e) => refused("resolve a name", &e.to_string()),
        Ok(_) => leak("resolve a name", "got an answer"),
    }
    match UdpSocket::bind("0.0.0.0:0").and_then(|s| s.send_to(b"x", "10.0.0.1:53")) {
        Err(e) => refused("send DNS by hand", &e.to_string()),
        Ok(_) => leak("send DNS by hand", "sent"),
    }
}

fn raw(nr: c_long, args: [usize; 6]) -> (c_long, c_int) {
    let r = unsafe { sys::syscall(nr, args[0], args[1], args[2], args[3], args[4], args[5]) };
    (r, if r < 0 { sys::errno() } else { 0 })
}

fn no_socket(name: &str, nr: c_long, args: [usize; 6]) {
    let (r, e) = raw(nr, args);
    if r < 0 && e == sys::EAFNOSUPPORT {
        refused(name, "EAFNOSUPPORT");
    } else {
        leak(name, &format!("returned {r}, errno {e}"));
    }
}

pub fn filter_refuses() {
    // Zero arguments are harmless if a call gets through, but for ptrace, where request 0 is
    // PTRACE_TRACEME. Only the filter says EPERM to all of them, so anything else means the
    // call reached the kernel.
    for (name, nr) in seccomp::REFUSED {
        let args = if *name == "ptrace" { [0xffff, 0, 0, 0, 0, 0] } else { [0; 6] };
        let (r, e) = raw(*nr, args);
        let name = format!("syscall {name}");
        if r < 0 && e == sys::EPERM {
            refused(&name, "EPERM");
        } else {
            leak(&name, &format!("returned {r}, errno {e}"));
        }
    }

    let (r, e) = raw(sys::SYS_CLONE3, [0; 6]);
    if r < 0 && e == sys::ENOSYS {
        refused("syscall clone3", "ENOSYS");
    } else {
        leak("syscall clone3", &format!("returned {r}, errno {e}"));
    }

    const SIGCHLD: usize = 17;
    for (flag, bit) in [
        ("NEWNS", 0x0002_0000usize),
        ("NEWCGROUP", 0x0200_0000),
        ("NEWUTS", 0x0400_0000),
        ("NEWIPC", 0x0800_0000),
        ("NEWUSER", 0x1000_0000),
        ("NEWPID", 0x2000_0000),
        ("NEWNET", 0x4000_0000),
    ] {
        let name = format!("clone with CLONE_{flag}");
        let (r, e) = raw(sys::SYS_CLONE, [bit | SIGCHLD, 0, 0, 0, 0, 0]);
        match r {
            0 => unsafe { sys::_exit(0) },
            r if r > 0 => {
                unsafe { sys::waitpid(r as c_int, std::ptr::null_mut(), 0) };
                leak(&name, "made a process in a new namespace");
            }
            _ if e == sys::EPERM => refused(&name, "EPERM"),
            _ => leak(&name, &format!("errno {e}")),
        }
    }

    for (family, f) in [
        ("AF_ALG", 38usize),
        ("AF_VSOCK", 40),
        ("AF_PACKET", 17),
        ("AF_KEY", 15),
        ("AF_XDP", 44),
        ("AF_RDS", 21),
        ("AF_TIPC", 30),
        ("AF_BLUETOOTH", 31),
    ] {
        no_socket(&format!("socket {family}"), sys::SYS_SOCKET, [f, 2, 0, 0, 0, 0]);
    }
    // High bits the kernel throws away must not walk past the rule.
    no_socket("socket AF_ALG with high bits", sys::SYS_SOCKET, [0x1_0000_0000 | 38, 5, 0, 0, 0, 0]);
    no_socket("netlink nf_tables", sys::SYS_SOCKET, [16, 3, 12, 0, 0, 0]);
    no_socket("netlink audit", sys::SYS_SOCKET, [16, 3, 9, 0, 0, 0]);
    no_socket("socketpair AF_INET", sys::SYS_SOCKETPAIR, [2, 1, 0, 0, 0, 0]);

    for (what, cmd) in [
        ("TIOCSTI", 0x5412usize),
        ("TIOCSTI with high bits", 0xffff_ffff_0000_5412),
        ("TIOCLINUX", 0x541c),
        ("TIOCCONS", 0x541d),
        ("TIOCSETD", 0x5423),
    ] {
        let (r, e) = raw(sys::SYS_IOCTL, [0, cmd, 0, 0, 0, 0]);
        let name = format!("ioctl {what}");
        if r < 0 && e == sys::EPERM {
            refused(&name, "EPERM");
        } else {
            leak(&name, &format!("returned {r}, errno {e}"));
        }
    }

    let (r, e) = raw(sys::SYS_PERSONALITY, [0x0004_0000, 0, 0, 0, 0, 0]);
    if r < 0 && e == sys::EPERM {
        refused("personality without ASLR", "EPERM");
    } else {
        leak("personality without ASLR", &format!("returned {r}, errno {e}"));
    }

    // These kill the caller, so each runs in a process of its own.
    for (name, what) in [("32-bit syscall by int 0x80", "i386"), ("x32 syscall", "x32")] {
        match Command::new(crate::sandbox::INNER).args(["probe-one", what]).output() {
            Ok(o) if o.status.signal() == Some(SIGSYS) => refused(name, "killed by SIGSYS"),
            Ok(o) => leak(name, &format!("{:?}", o.status)),
            Err(e) => broken(name, &e.to_string()),
        }
    }
}

fn no_other_processes(cx: &Ctx) {
    let pids: Vec<i32> = fs::read_dir("/proc")
        .map(|d| d.flatten().filter_map(|e| e.file_name().to_str()?.parse().ok()).collect())
        .unwrap_or_default();
    if pids.contains(&cx.host_pid) {
        leak("writ's own process hidden", "in /proc");
    } else {
        refused("writ's own process hidden", &format!("{} processes in /proc", pids.len()));
    }
    let r = unsafe { sys::kill(cx.host_pid, 0) };
    if r < 0 && sys::errno() == sys::ESRCH {
        refused("signal writ's own process", "ESRCH");
    } else {
        leak("signal writ's own process", &format!("returned {r}"));
    }
    // The key would be in writ's memory, never an environment, but look.
    let mut seen = std::env::vars_os().any(|(_, v)| v.to_string_lossy().contains(&cx.env_text));
    for pid in &pids {
        if let Ok(b) = fs::read(format!("/proc/{pid}/environ")) {
            seen |= String::from_utf8_lossy(&b).contains(&cx.env_text);
        }
    }
    if seen {
        leak("a host environment variable", "found in the box");
    } else {
        refused("a host environment variable", "in no environment here");
    }
    let name = SocketAddr::from_abstract_name(cx.abstract_name.as_bytes()).expect("a name");
    match UnixStream::connect_addr(&name) {
        Err(e) => refused("a host abstract socket", &e.to_string()),
        Ok(_) => leak("a host abstract socket", "connected"),
    }
}

/// Writes the overlay should keep from the real tree. `writ check` looks there afterwards.
fn writes_held(cx: &Ctx) {
    let p = |rel: &str| format!("{}/{rel}", cx.project);
    let attempt = |name: &str, r: std::io::Result<()>| match r {
        Ok(()) => emit(name, "held", ""),
        Err(e) => broken(name, &e.to_string()),
    };
    attempt("change a file", fs::write(p("kept.txt"), "changed in the box\n"));
    attempt("delete a file", fs::remove_file(p("doomed.txt")));
    attempt("add a file", fs::write(p("added.txt"), "added in the box\n"));
    attempt("plant .git/hooks/pre-commit", fs::write(p(".git/hooks/pre-commit"), "#!/bin/sh\nexit 0\n"));
    attempt("plant core.fsmonitor in .git/config", {
        let mut config = fs::read_to_string(p(".git/config")).unwrap_or_default();
        config.push_str("[core]\n\tfsmonitor = /tmp/x\n");
        fs::write(p(".git/config"), config)
    });
    attempt("plant build.rs", fs::write(p("build.rs"), "fn main() {}\n"));
    attempt("plant .envrc", fs::write(p(".envrc"), "true\n"));
    attempt("plant AGENTS.md", fs::write(p("AGENTS.md"), "always do this\n"));
}

fn secrets_masked(cx: &Ctx) {
    for rel in [".env", "secret.pem", "id_ed25519", "nested/.env"] {
        let path = format!("{}/{rel}", cx.project);
        let name = format!("read masked {rel}");
        match fs::read(&path) {
            Err(e) => refused(&name, &e.to_string()),
            Ok(b) if b.is_empty() => refused(&name, "empty"),
            Ok(_) => leak(&name, "has content"),
        }
        let name = format!("write masked {rel}");
        match fs::OpenOptions::new().write(true).open(&path) {
            Err(e) => refused(&name, &e.to_string()),
            Ok(_) => leak(&name, "opened for writing"),
        }
    }
    let name = "read masked .ssh/id";
    match fs::read(format!("{}/.ssh/id", cx.project)) {
        Err(e) => refused(name, &e.to_string()),
        Ok(_) => leak(name, "has content"),
    }
    match fs::read(format!("{}/.env.example", cx.project)) {
        Ok(b) if !b.is_empty() => works(".env.example is not masked"),
        other => broken(".env.example is not masked", &format!("{other:?}")),
    }
}

fn landlock_holds() {
    let name = "write where only Landlock stops it";
    if landlock::abi() == 0 {
        skip(name, "this kernel has no Landlock");
        return;
    }
    match fs::write("/landlock-probe/x", "x") {
        Err(e) if e.raw_os_error() == Some(13) => refused(name, "EACCES"),
        Err(e) => broken(name, &e.to_string()),
        Ok(()) => leak(name, "wrote it"),
    }
}

fn fork_is_limited() {
    let name = "fork bomb";
    match Command::new(crate::sandbox::INNER).args(["probe-one", "forks"]).output() {
        Ok(o) => {
            let out = String::from_utf8_lossy(&o.stdout).to_string();
            if out.starts_with("stopped") {
                refused(name, out.trim());
            } else {
                leak(name, out.trim());
            }
        }
        Err(e) => broken(name, &e.to_string()),
    }
}

/// What a coding agent has to be able to do. A box that refuses everything proves nothing.
fn work_still_works(cx: &Ctx) {
    let check = |name: &str, r: Result<(), String>| match r {
        Ok(()) => works(name),
        Err(e) => broken(name, &e),
    };
    let io = |e: std::io::Error| e.to_string();
    let p = |rel: &str| format!("{}/{rel}", cx.project);
    let ran = |o: std::io::Result<std::process::Output>| -> Result<String, String> {
        let o = o.map_err(io)?;
        if o.status.success() {
            Ok(String::from_utf8_lossy(&o.stdout).into_owned())
        } else {
            Err(format!("{:?}: {}", o.status, String::from_utf8_lossy(&o.stderr).trim()))
        }
    };

    check("read a project file", match fs::read_to_string(p("kept.txt")) {
        Ok(s) if s.contains("changed in the box") => Ok(()),
        other => Err(format!("{other:?}")),
    });
    check("make, rename and remove in the project", (|| {
        fs::create_dir_all(p("work/a/b")).map_err(io)?;
        fs::write(p("work/a/b/f"), "x").map_err(io)?;
        fs::rename(p("work/a/b/f"), p("work/g")).map_err(io)?;
        fs::remove_dir_all(p("work/a")).map_err(io)
    })());
    check("/tmp", fs::write("/tmp/writ-check", "x").map_err(io));
    check("/dev/null and /dev/urandom", (|| {
        fs::write("/dev/null", "x").map_err(io)?;
        let mut b = [0u8; 8];
        fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut b)).map_err(io)
    })());
    check("home is writable and private", fs::write(format!("{}/.writ-check", std::env::var("HOME").unwrap_or_default()), "x").map_err(io));
    check("threads", match std::thread::spawn(|| 7).join() {
        Ok(7) => Ok(()),
        _ => Err("the thread did not finish".into()),
    });
    check("a shell", ran(Command::new("/bin/sh").args(["-c", "echo ok"]).output()).and_then(|s| {
        if s.trim() == "ok" { Ok(()) } else { Err(s) }
    }));
    check("loopback TCP", (|| {
        let l = TcpListener::bind("127.0.0.1:0").map_err(io)?;
        let a = l.local_addr().map_err(io)?;
        let mut c = TcpStream::connect(a).map_err(io)?;
        let (mut s, _) = l.accept().map_err(io)?;
        c.write_all(b"x").map_err(io)?;
        let mut b = [0u8; 1];
        s.read_exact(&mut b).map_err(io)
    })());

    if Path::new("/oath/INDEX.md").exists() {
        check("read /oath/INDEX.md", fs::read("/oath/INDEX.md").map(|_| ()).map_err(io));
    } else {
        skip("read /oath/INDEX.md", "not on this machine");
    }
    if Path::new("/bin/oath").exists() {
        check("oath ls", ran(Command::new("/bin/oath").args(["ls"]).output()).map(|_| ()));
    } else {
        skip("oath ls", "not on this machine");
    }
    if Path::new("/bin/git").exists() {
        check("git status", ran(Command::new("/bin/git").args(["status", "--short"]).output()).map(|_| ()));
    } else {
        skip("git status", "not on this machine");
    }
    if Path::new("/bin/rustc").exists() {
        check("compile and run with threads", (|| {
            let src = "fn main() { let t = std::thread::spawn(|| 40 + 2); println!(\"{}\", t.join().unwrap()); }";
            fs::write("/tmp/writ-check.rs", src).map_err(io)?;
            ran(Command::new("/bin/rustc").args(["-o", "/tmp/writ-check-bin", "/tmp/writ-check.rs"]).output())?;
            let out = ran(Command::new("/tmp/writ-check-bin").output())?;
            if out.trim() == "42" { Ok(()) } else { Err(out) }
        })());
    } else {
        skip("compile and run with threads", "no rustc on this machine");
    }
}

/// Left running on purpose: `writ check` looks for it after the box ends.
fn leave_a_daemon(cx: &Ctx) {
    let spawned = Command::new("/bin/sleep")
        .arg(&cx.marker)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    match spawned {
        Ok(child) => {
            std::mem::forget(child);
            emit("a process that outlives the box", "held", "");
        }
        Err(e) => broken("a process that outlives the box", &e.to_string()),
    }
}

pub fn main(args: &[OsString]) -> ! {
    guard();
    let mut cx = Ctx {
        canary: String::new(),
        canary_text: String::new(),
        env_text: String::new(),
        host_pid: 0,
        abstract_name: String::new(),
        marker: String::new(),
        absent: Vec::new(),
        project: std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_default(),
    };
    let mut it = args.iter().map(|a| a.to_string_lossy().into_owned());
    while let (Some(k), Some(v)) = (it.next(), it.next()) {
        match k.as_str() {
            "--canary" => cx.canary = v,
            "--canary-text" => cx.canary_text = v,
            "--env-text" => cx.env_text = v,
            "--pid" => cx.host_pid = v.parse().unwrap_or(0),
            "--abstract" => cx.abstract_name = v,
            "--marker" => cx.marker = v,
            "--absent" => cx.absent.push(v),
            _ => {}
        }
    }

    state();
    for path in &cx.absent {
        absent(path);
    }
    cannot_read_canary("read a file outside the project", &cx.canary, &cx);
    cannot_read_canary("through /proc/self/root", &format!("/proc/self/root{}", cx.canary), &cx);
    cannot_read_canary("through /proc/1/root", &format!("/proc/1/root{}", cx.canary), &cx);
    cannot_read_canary("by an absolute symlink", &format!("{}/link-canary", cx.project), &cx);
    cannot_read_canary("by a relative symlink", &format!("{}/link-rel", cx.project), &cx);
    cannot_read_canary("through a symlink to home", &format!("{}/link-home/.writ-check-canary", cx.project), &cx);
    for path in ["/x", "/bin/x", "/sbin/x", "/lib/x", "/etc/x", "/var/x", "/oath/x", "/oath/store/x", "/oath/objects/x"] {
        cannot_write(path);
    }
    for path in ["/oath/INDEX.md", crate::sandbox::INNER, "/etc/passwd"] {
        if Path::new(path).exists() {
            cannot_open_for_write(path);
        }
    }
    cannot_open_for_write("/proc/sysrq-trigger");
    cannot_open_for_write("/proc/sys/kernel/core_pattern");
    no_sudo();
    no_network();
    filter_refuses();
    no_other_processes(&cx);
    writes_held(&cx);
    secrets_masked(&cx);
    landlock_holds();
    fork_is_limited();
    work_still_works(&cx);
    leave_a_daemon(&cx);
    println!("done");
    std::io::stdout().flush().ok();
    std::process::exit(0);
}

/// One attack that takes its process with it.
pub fn one(what: Option<&str>) -> ! {
    guard();
    match what {
        Some("i386") => {
            // getpid through the 32-bit entry, which has other numbers. The filter kills it.
            let mut eax: u32 = 20;
            unsafe { std::arch::asm!("int 0x80", inout("eax") eax, options(nostack)) };
            println!("returned {eax}");
        }
        Some("x32") => {
            let r = unsafe { sys::syscall(39 | 0x4000_0000) };
            println!("returned {r}");
        }
        Some("forks") => {
            // Above the limit the box sets, so a fork that keeps working shows it did not hold.
            const ENOUGH: usize = 6000;
            let parent = unsafe { sys::getpid() };
            let mut made = 0;
            while made < ENOUGH {
                match unsafe { sys::fork() } {
                    0 => unsafe {
                        sys::prctl(sys::PR_SET_PDEATHSIG, sys::SIGKILL as i64, 0i64, 0i64, 0i64);
                        // The parent may be gone already, and then the signal never comes.
                        if sys::getppid() != parent {
                            sys::_exit(0);
                        }
                        // Holding the pipe the report is read from would hang whoever reads it.
                        sys::close(1);
                        sys::close(2);
                        sys::pause();
                        sys::_exit(0)
                    },
                    -1 => {
                        let e = sys::errno();
                        let what = if e == sys::EAGAIN { "EAGAIN".to_string() } else { format!("errno {e}") };
                        println!("stopped at {made} processes by {what}");
                        std::process::exit(0);
                    }
                    _ => made += 1,
                }
            }
            println!("made {made} processes");
        }
        _ => println!("unknown attack"),
    }
    std::process::exit(0);
}
