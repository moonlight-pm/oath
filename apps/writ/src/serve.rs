//! A local model, served from a box (R65), as `work` (R55). A model file is untrusted input to a
//! large C++ parser, and this keeps a bad one from mattering. In the box the server has its model
//! through one read-only descriptor and one directory for its socket, with no network, no home and
//! no way back through `sudo`. Past the box it is `work`, so an escape short of the kernel stops
//! there: `home` has `sudo` and the keys.
//!
//! `work` cannot reach anything of `home`'s by path, so `home` hands over what it needs, open,
//! through `sudo`: the model, and the listening ends of two sockets it made in its own directory. A
//! small relay as `work`, outside the box, carries connections to the server's own socket. So
//! nothing the box or `work` can write is anywhere `home` looks.
//!
//! A GPU is the one thing `--gpu` adds, and it is a real cost: the driver's ioctls are a large
//! surface in the kernel, and the box does not narrow them. An exploit there has the kernel, and
//! then the account does not matter.

use crate::{inner, json, landlock, model, sandbox, sys, term};
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::net::Shutdown;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SERVER: &str = "/bin/llama-server";
/// Where the socket is in the box. The model is its descriptor: see `model_in_box`.
const SOCKET_DIR_IN_BOX: &str = "/sock";
const SOCKET_NAME: &str = "llama.sock";
/// Connecting here stops a server, and the connection ends when it has stopped.
const STOP_NAME: &str = "stop.sock";
const LOG_NAME: &str = "log";
const RUNNING_NAME: &str = "running.json";
/// Where `work` keeps the server's own socket, under its runtime directory.
const WORK_DIR_NAME: &str = "writ-llama";
/// The account a server runs as (R55). `sudo` reaches it, and refuses it the way back.
const WORK: &str = "work";
const WORK_UID: u32 = 2;
const SUDO: &str = "/bin/sudo";
/// Stages of this program that run as `work`.
const WORK_STAGE: &str = "serve-work";
const CHECK_STAGE: &str = "serve-check";
/// What each of a server's streams may add to its log, which a bad server could otherwise grow.
const LOG_LIMIT: u64 = 32 << 20;
/// A large model from a slow disk, held to a share of the GPU.
const LOAD_LIMIT: Duration = Duration::from_secs(1200);
/// What a server gets when nobody has said: what was measured on a real GPU, and the CPU's default.
const GPU_CONTEXT: u32 = 32768;
const CPU_CONTEXT: u32 = 16384;
const GPU_SHARE: u32 = 40;
/// The model is mapped from the file, so what a box's memory holds is its context.
const TMPFS_BYTES: u64 = 256 << 20;

/// A box has a little of the system: the program and the libraries it loads.
const SYSTEM: &[&str] = &["/bin", "/lib", "/oath/store"];

/// A period of `--gpu-share`: short beside a token's work, long beside a signal.
const SHARE_PERIOD: Duration = Duration::from_millis(100);

/// NVIDIA's compute nodes in `dev`, all a model needs of its GPU. Not `nvidia-modeset` or
/// `dri/card*`, the display, or `nvidia-caps`, the driver's privileges, or `nvidia-uvm`, which
/// Vulkan does not use. Only NVIDIA: nothing else has been tried in a box.
fn gpu_nodes(dev: &Path) -> Vec<PathBuf> {
    let mut nodes: Vec<PathBuf> = fs::read_dir(dev)
        .map(|d| {
            d.flatten()
                .filter(|e| {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    name == "nvidiactl"
                        || name.strip_prefix("nvidia").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                })
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    nodes.sort();
    nodes
}

/// Where `home` makes the sockets. Only `home` can reach this directory, and no box or `work`.
pub fn socket_dir(home: &Path) -> PathBuf {
    home.join(".local/share/writ/llama")
}

pub fn socket(home: &Path) -> PathBuf {
    socket_dir(home).join(SOCKET_NAME)
}

/// The model, in the box. bwrap cannot mount it: it resolves a descriptor's path first, and `work`
/// cannot walk a path through `home`. So the server opens the descriptor it was handed.
fn model_in_box(fd: i32) -> String {
    format!("/proc/self/fd/{fd}")
}

/// bwrap's arguments for a server. Not the project's box: nothing here is writable but the
/// socket's directory and a small /tmp, so there is nothing to hold or apply.
fn args(sock_dir: &Path, exe: &Path, gpu: &[PathBuf]) -> Vec<OsString> {
    let mut a: Vec<OsString> = Vec::new();
    let mut push = |items: &[&std::ffi::OsStr]| a.extend(items.iter().map(|s| s.to_os_string()));
    let s = |t: &'static str| std::ffi::OsStr::new(t);
    push(&[s("--unshare-all"), s("--unshare-user"), s("--disable-userns"), s("--assert-userns-disabled")]);
    push(&[s("--die-with-parent"), s("--new-session"), s("--hostname"), s("writ"), s("--clearenv")]);
    push(&[s("--setenv"), s("PATH"), s("/bin"), s("--setenv"), s("HOME"), s("/tmp"), s("--setenv"), s("LANG"), s("C.UTF-8")]);
    for p in SYSTEM {
        push(&[s("--ro-bind"), s(p), s(p)]);
    }
    push(&[s("--proc"), s("/proc"), s("--dev"), s("/dev")]);
    for node in gpu {
        push(&[s("--dev-bind"), node.as_os_str(), node.as_os_str()]);
    }
    if !gpu.is_empty() {
        // Vulkan's implicit layers are for windows and choosing among GPUs: code a server has no use for.
        push(&[s("--setenv"), s("VK_LOADER_LAYERS_DISABLE"), s("~implicit~")]);
    }
    let size = TMPFS_BYTES.to_string();
    push(&[s("--size"), size.as_ref(), s("--tmpfs"), s("/tmp")]);
    push(&[s("--bind"), sock_dir.as_os_str(), s(SOCKET_DIR_IN_BOX)]);
    push(&[s("--ro-bind"), exe.as_os_str(), s(sandbox::INNER)]);
    push(&[s("--remount-ro"), s("/"), s("--chdir"), s("/tmp")]);
    a
}

/// The server's arguments. No port: `.sock` makes it a Unix socket. `--jinja` runs the model's
/// own template, which is how tool calls work.
fn server_args(model_fd: i32, threads: Option<u32>, context: u32, gpu: bool, extra: &[OsString]) -> Vec<OsString> {
    let mut a: Vec<OsString> = vec![SERVER.into(), "--model".into(), model_in_box(model_fd).into()];
    a.push("--host".into());
    a.push(format!("{SOCKET_DIR_IN_BOX}/{SOCKET_NAME}").into());
    a.extend(["--no-webui".into(), "--jinja".into(), "--parallel".into(), "1".into()]);
    // Its default echoes any origin back as allowed. No browser reaches a socket; still.
    a.extend(["--cors-origins".into(), "localhost".into()]);
    // One person, one slot, whose own memory keeps a conversation's prefix. Its default also
    // copies each prompt's state to RAM, up to 8 GiB of it.
    a.extend(["--cache-ram".into(), "0".into()]);
    a.push("--ctx-size".into());
    a.push(context.to_string().into());
    // Exactly this, or a failure: the server's default shrinks a context to fit what is free.
    a.extend(["--fit".into(), "off".into(), "--n-gpu-layers".into()]);
    a.push(if gpu { "all" } else { "0" }.into());
    if let Some(t) = threads {
        a.push("--threads".into());
        a.push(t.to_string().into());
    }
    a.extend_from_slice(extra);
    a
}

/// AVX2 is what the package is built for. Better a sentence now than an illegal instruction.
fn cpu_can_run_it() -> bool {
    fs::read_to_string("/proc/cpuinfo")
        .map(|c| c.lines().find(|l| l.starts_with("flags")).is_some_and(|l| {
            ["avx2", "fma", "f16c", "bmi2"].iter().all(|f| l.split_whitespace().any(|w| w == *f))
        }))
        .unwrap_or(false)
}

/// Clear a socket a dead server left, and stop if a live one has it: it does not unlink its own.
fn claim_socket(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Err(_) => Ok(()),
        Ok(m) if m.file_type().is_socket() => {
            if UnixStream::connect(path).is_ok() {
                return Err(format!("a server is already listening on {}", path.display()));
            }
            fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))
        }
        Ok(_) => Err(format!("{} is not a socket", path.display())),
    }
}

/// bwrap with the box's second stage, ready for the program to run in it. Of everything this
/// process holds, the model's descriptor alone goes in.
fn command(model_fd: i32, dir: &Path, gpu: &[PathBuf]) -> Result<Command, String> {
    sandbox::trusted_bwrap()?;
    let exe = std::env::current_exe().map_err(|e| format!("own path: {e}"))?;
    let mut c = Command::new(sandbox::BWRAP);
    c.env_clear();
    c.args(args(dir, &exe, gpu));
    c.arg(sandbox::INNER).arg("inner");
    for w in [Path::new("/tmp"), Path::new(SOCKET_DIR_IN_BOX)] {
        c.arg("--write").arg(w);
    }
    c.arg("--read").arg(model_in_box(model_fd));
    c.arg("--keep-fd").arg(model_fd.to_string());
    c.arg("--");
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    hand_over(&mut c, vec![model_fd], false);
    Ok(c)
}

/// These descriptors survive the program's start, and no others: `harden` marked the rest.
fn hand_over(c: &mut Command, fds: Vec<i32>, detach: bool) {
    unsafe {
        c.pre_exec(move || {
            for fd in &fds {
                if sys::fcntl(*fd, sys::F_SETFD, 0i32) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            if detach && sys::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// A directory of `work`'s own, under its runtime directory, which `sudo` makes.
fn work_dir(name: &str) -> Result<PathBuf, String> {
    let run = std::env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is not set")?;
    let dir = PathBuf::from(run).join(name);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// `work` has to be able to run this program, which rules out anywhere under `home`.
fn program_for_work(home: &Path) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("own path: {e}"))?;
    if exe.starts_with(home) {
        return Err(format!("`work` cannot run {}, which is under your home: use the installed writ", exe.display()));
    }
    Ok(exe)
}

/// `sudo` to `work`, running a stage of this program. It closes nothing it is handed.
fn as_work(exe: &Path, stage: &str) -> Command {
    let mut c = Command::new(SUDO);
    c.env_clear().args(["-n", "-u", WORK, "--"]).arg(exe).arg(stage);
    c
}

/// Attack what a server has, from where it has it. Runs as `home`; the attacks run as `work`.
pub fn attacks_as_work(home: &Path, model: &Path, env_text: &str) -> Result<String, String> {
    let exe = program_for_work(home)?;
    let file = fs::File::open(model).map_err(|e| format!("{}: {e}", model.display()))?;
    let fd = file.as_raw_fd();
    let mut c = as_work(&exe, CHECK_STAGE);
    c.arg(home).arg(model).arg(fd.to_string()).arg(env_text).arg(std::process::id().to_string());
    hand_over(&mut c, vec![fd], false);
    let out = c.output().map_err(|e| format!("{SUDO}: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if !text.lines().any(|l| l == "done") {
        return Err(format!("the attacks as `work` did not finish: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(text)
}

/// `writ check`, as `work`: what an escape from the box would find here, then the box itself.
pub fn check_main(rest: &[OsString]) -> Result<i32, String> {
    let [home, model, fd, env_text, host_pid] = rest else {
        return Err("serve-check: home, model, its descriptor, a value from the host's environment, a host PID".into());
    };
    let fd: i32 = fd.to_str().and_then(|n| n.parse().ok()).ok_or("a descriptor is a number")?;
    attack_past_the_box(Path::new(home), Path::new(model), host_pid.to_str().unwrap_or(""));
    let dir = work_dir(&format!("{WORK_DIR_NAME}-check-{}", std::process::id()))?;
    // Once as it is by default, and once with this machine's GPU, if it has one.
    let gpu = this_machines_gpu();
    for nodes in [&[][..], &gpu[..]].iter().take(if gpu.is_empty() { 1 } else { 2 }) {
        let mut c = command(fd, &dir, nodes)?;
        c.arg(sandbox::INNER).arg("probe-serve").arg(home).arg(model).arg(fd.to_string()).arg(env_text).arg(host_pid);
        c.args(*nodes);
        let out = c.output().map_err(|e| format!("bwrap: {e}"))?;
        let text = String::from_utf8_lossy(&out.stdout);
        if !text.lines().any(|l| l == "done") {
            let _ = fs::remove_dir_all(&dir);
            return Err(format!("the attacks on a server's box did not finish: {}", term::scrub(&String::from_utf8_lossy(&out.stderr))));
        }
        text.lines().filter(|l| *l != "done").for_each(|l| println!("{l}"));
    }
    let _ = fs::remove_dir_all(&dir);
    println!("done");
    Ok(0)
}

/// An escape from the box, as `work`, with the box gone. This is what the account is for. It asks
/// whether a key is reachable and no more, so it reads no real secret even if one is.
fn attack_past_the_box(home: &Path, model: &Path, host_pid: &str) {
    use crate::probe::{broken, leak, refused, works};
    use std::io::ErrorKind::PermissionDenied;
    if unsafe { sys::getuid() } == WORK_UID {
        works("work: these run as `work`");
    } else {
        broken("work: these run as `work`", "another account");
    }
    // A file anyone may read, under directories only `home` may enter.
    match fs::read(model) {
        Ok(_) => leak("work: a file under home cannot be reached by path", "it was read"),
        Err(e) => refused("work: a file under home cannot be reached by path", &e.to_string()),
    }
    match fs::read_dir(home) {
        Ok(_) => leak("work: home cannot be listed", "it was listed"),
        Err(e) => refused("work: home cannot be listed", &e.to_string()),
    }
    for (name, path) in [
        ("work: an SSH key cannot be reached", ".ssh/id_ed25519"),
        ("work: an agent's credentials cannot be reached", ".claude/.credentials.json"),
        ("work: writ's own sockets cannot be reached", ".local/share/writ/llama/llama.sock"),
        ("work: a session cannot be reached", ".local/share/writ/sessions"),
    ] {
        // Absent would mean the walk got through, and only that this file is not there.
        match fs::symlink_metadata(home.join(path)) {
            Err(e) if e.kind() == PermissionDenied => refused(name, "permission denied"),
            Ok(_) => leak(name, "it is there"),
            Err(_) => leak(name, "the path can be walked"),
        }
    }
    match fs::read_dir("/run/user/1") {
        Ok(_) => leak("work: home's runtime directory cannot be listed", "it was listed"),
        Err(e) => refused("work: home's runtime directory cannot be listed", &e.to_string()),
    }
    for (name, args) in [("work: no way back to root", &["-n", "id"][..]), ("work: sudo does not take it to `work` again", &["-n", "-u", WORK, "id"][..])] {
        match Command::new(SUDO).args(args).output() {
            Ok(o) if o.status.success() => leak(name, "sudo ran"),
            Ok(o) => refused(name, &String::from_utf8_lossy(&o.stderr).replace('\n', " ")),
            Err(e) => refused(name, &e.to_string()),
        }
    }
    for what in ["environ", "mem"] {
        let name = format!("work: a process of home's cannot be read: {what}");
        match fs::read(format!("/proc/{host_pid}/{what}")) {
            Ok(_) => leak(&name, "it was read"),
            Err(e) => refused(&name, &e.to_string()),
        }
    }
    match host_pid.parse::<i32>().map(|p| unsafe { sys::kill(p, 0) }) {
        Ok(0) => leak("work: a process of home's cannot be signalled", "it could be"),
        Ok(_) if sys::errno() == sys::EPERM => refused("work: a process of home's cannot be signalled", "EPERM"),
        Ok(_) => broken("work: a process of home's cannot be signalled", &format!("errno {}", sys::errno())),
        Err(e) => broken("work: a process of home's cannot be signalled", &e.to_string()),
    }
    match fs::OpenOptions::new().write(true).open("/oath/.lock") {
        Ok(_) => leak("work: the catalog's lock cannot be taken", "it was opened"),
        Err(e) => refused("work: the catalog's lock cannot be taken", &e.to_string()),
    }
}

/// What `writ serve --gpu` would give a box on this machine.
pub fn this_machines_gpu() -> Vec<PathBuf> {
    gpu_nodes(Path::new("/dev"))
}

/// The processes under `root`, from each one's parent in /proc: this kernel has no `children` file.
fn deepest_under(root: u32) -> Option<u32> {
    let mut parents: Vec<(u32, u32)> = Vec::new();
    for e in fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<u32>().ok()) else { continue };
        let Ok(stat) = fs::read_to_string(e.path().join("stat")) else { continue };
        // A program's name can hold anything, up to the last parenthesis.
        let Some(after) = stat.rfind(')').map(|i| &stat[i + 1..]) else { continue };
        if let Some(ppid) = after.split_whitespace().nth(1).and_then(|p| p.parse().ok()) {
            parents.push((pid, ppid));
        }
    }
    let mut at = root;
    while let Some((pid, _)) = parents.iter().find(|(_, ppid)| *ppid == at) {
        at = *pid;
    }
    (at != root).then_some(at)
}

/// Hold a server to a share of the GPU: a GPU has no share to set, short of virtualizing it, so
/// the server is stopped for the rest of each period. Its work on the GPU ends soon after. This is
/// a limit on time, on average; memory is the model's and the context's, and the owner's to size.
fn hold_to_share(bwrap: u32, share: u32, done: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    let run = SHARE_PERIOD * share / 100;
    let rest = SHARE_PERIOD - run;
    std::thread::spawn(move || {
        // bwrap, the namespace's first process, then the server.
        let found = (0..200).find_map(|_| {
            std::thread::sleep(Duration::from_millis(50));
            deepest_under(bwrap).filter(|pid| fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|c| c.trim() == "llama-server"))
        });
        let Some(pid) = found else {
            eprintln!("writ: the server was not found, so --gpu-share is not holding it");
            return;
        };
        // A descriptor names this process and no later one with its number.
        let fd = unsafe { sys::syscall(sys::SYS_PIDFD_OPEN, pid as i32, 0u32) };
        if fd < 0 {
            eprintln!("writ: pidfd_open: errno {}, so --gpu-share is not holding the server", sys::errno());
            return;
        }
        let signal = |s: i32| unsafe { sys::syscall(sys::SYS_PIDFD_SEND_SIGNAL, fd as i32, s, std::ptr::null::<u8>(), 0u32) == 0 };
        while !done.load(Ordering::Relaxed) {
            std::thread::sleep(run);
            if !signal(sys::SIGSTOP) {
                break;
            }
            std::thread::sleep(rest);
            if !signal(sys::SIGCONT) {
                break;
            }
        }
        signal(sys::SIGCONT);
        unsafe { sys::close(fd as i32) };
    })
}

/// What a server is started with.
#[derive(Clone, Debug, PartialEq)]
pub struct Load {
    model: PathBuf,
    threads: Option<u32>,
    context: u32,
    gpu: bool,
    share: u32,
}

impl Load {
    /// What a machine gets when nobody has said otherwise.
    fn for_this_machine(model: PathBuf) -> Load {
        let gpu = !this_machines_gpu().is_empty();
        Load { model, threads: None, context: if gpu { GPU_CONTEXT } else { CPU_CONTEXT }, gpu, share: if gpu { GPU_SHARE } else { 100 } }
    }

    fn json(&self) -> Result<String, String> {
        let model = self.model.to_str().ok_or("the model's path is not UTF-8")?;
        let threads = self.threads.map_or(json::Value::Null, |t| json::n(t.into()));
        Ok(json::obj([
            ("model", json::s(model)),
            ("threads", threads),
            ("context", json::n(self.context.into())),
            ("gpu", json::Value::Bool(self.gpu)),
            ("share", json::n(self.share.into())),
        ])
        .text())
    }

    fn parse(text: &str) -> Result<Load, String> {
        let v = json::parse(text)?;
        let number = |k: &str| v.get(k).and_then(|n| n.u64()).and_then(|n| u32::try_from(n).ok()).ok_or(format!("no {k}"));
        Ok(Load {
            model: PathBuf::from(v.get("model").and_then(|m| m.str()).ok_or("no model")?),
            threads: number("threads").ok(),
            context: number("context")?,
            gpu: matches!(v.get("gpu"), Some(json::Value::Bool(true))),
            share: number("share")?,
        })
    }

    /// What `work`'s stage is told, the same options a person gives.
    fn stage_args(&self) -> Vec<OsString> {
        let mut a: Vec<OsString> = vec!["--context".into(), self.context.to_string().into()];
        if let Some(t) = self.threads {
            a.extend(["--threads".into(), t.to_string().into()]);
        }
        if self.gpu {
            a.extend(["--gpu-share".into(), self.share.to_string().into()]);
        }
        a
    }

    fn say(&self) -> String {
        let name = self.model.file_name().map_or_else(|| self.model.display().to_string(), |n| n.to_string_lossy().into_owned());
        let place = match (self.gpu, self.share) {
            (false, _) => "on the CPU".to_string(),
            (true, 100) => "on the GPU".to_string(),
            (true, p) => format!("on the GPU at a {p}% share"),
        };
        format!("{name}, a {}-token context, {place}", self.context)
    }
}

/// The options `writ serve` and `writ use local` share, with nothing assumed.
#[derive(Default, Debug)]
struct Options {
    model: Option<PathBuf>,
    threads: Option<u32>,
    context: Option<u32>,
    gpu: Option<bool>,
    share: Option<u32>,
    extra: Vec<OsString>,
}

fn options(args: &[OsString]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    let number = |v: Option<&OsString>, what: &str| -> Result<u32, String> {
        v.and_then(|s| s.to_str()).and_then(|s| s.parse().ok()).ok_or(format!("{what} takes a number"))
    };
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--model") => o.model = it.next().map(PathBuf::from),
            Some("--threads") => o.threads = Some(number(it.next(), "--threads")?),
            Some("--context") => o.context = Some(number(it.next(), "--context")?),
            Some("--gpu") => o.gpu = Some(true),
            Some("--cpu") => o.gpu = Some(false),
            Some("--gpu-share") => {
                let share = number(it.next(), "--gpu-share")?;
                if !(1..=100).contains(&share) {
                    return Err("--gpu-share is a percentage, 1 to 100".into());
                }
                o.share = Some(share);
                o.gpu = Some(true);
            }
            Some("--") => {
                o.extra = it.by_ref().cloned().collect();
                break;
            }
            _ => return Err(format!("unknown argument {}", a.to_string_lossy())),
        }
    }
    Ok(o)
}

impl Options {
    fn over(&self, base: Load, home: &Path) -> Result<Load, String> {
        let mut l = base;
        if let Some(m) = &self.model {
            l.model = resolve_model(home, m)?;
        }
        l.threads = self.threads.or(l.threads);
        l.context = self.context.unwrap_or(l.context);
        l.gpu = self.gpu.unwrap_or(l.gpu);
        l.share = if l.gpu { self.share.unwrap_or(l.share) } else { 100 };
        Ok(l)
    }
}

fn models_dir(home: &Path) -> PathBuf {
    home.join(".local/share/writ/models")
}

fn settings_path(home: &Path) -> PathBuf {
    home.join(".config/writ/local.json")
}

/// A name in the models directory, or a path. `work` reads it through a descriptor, so the file
/// must let others read it; directories only `home` may enter keep them out by path. It must not
/// let them write it, or the descriptor is a way to change it on a kernel without Landlock.
fn resolve_model(home: &Path, given: &Path) -> Result<PathBuf, String> {
    let bare = given.components().count() == 1;
    let path = if bare && !given.exists() { models_dir(home).join(given) } else { given.to_path_buf() };
    let path = fs::canonicalize(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let m = fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !m.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    if m.permissions().mode() & 0o004 == 0 {
        return Err(format!("`work` cannot read {}: chmod o+r it", path.display()));
    }
    if m.permissions().mode() & 0o002 != 0 {
        return Err(format!("anyone can change {}: chmod o-w it", path.display()));
    }
    Ok(path)
}

/// The one model in the directory, or a sentence about why there is not one.
fn only_model(dir: &Path) -> Result<PathBuf, String> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .map(|d| d.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "gguf")).collect())
        .unwrap_or_default();
    found.sort();
    match found.len() {
        0 => Err(format!("no model in {}: put a .gguf there, or name one with `writ use local --model FILE`", dir.display())),
        1 => Ok(found.remove(0)),
        n => Err(format!("{n} models in {}: choose with `writ use local --model NAME`", dir.display())),
    }
}

/// What `writ use local` last settled on, if it has.
fn saved(home: &Path) -> Result<Option<Load>, String> {
    let path = settings_path(home);
    match fs::read_to_string(&path) {
        Ok(t) => Load::parse(&t).map(Some).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn running(dir: &Path) -> Option<Load> {
    Load::parse(&fs::read_to_string(dir.join(RUNNING_NAME)).ok()?).ok()
}

fn prepare_dir(home: &Path) -> Result<PathBuf, String> {
    let dir = socket_dir(home);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// Held while a server is being started or stopped, so two sessions cannot both start one.
fn lock(dir: &Path) -> Result<fs::File, String> {
    let path = dir.join("lock");
    let f = fs::File::create(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    f.lock().map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(f)
}

fn forget_server(dir: &Path) {
    for name in [SOCKET_NAME, STOP_NAME, RUNNING_NAME] {
        let _ = fs::remove_file(dir.join(name));
    }
}

/// Bind `home`'s sockets and start `work`'s stage with their listening ends and the model. `log`
/// is where a server that outlives this goes: its own session, so a closed terminal leaves it be.
/// This process's copies close on return, so a dead stage is a refused connection.
fn start_as_work(home: &Path, load: &Load, extra: &[OsString], dir: &Path, log: Option<fs::File>) -> Result<Child, String> {
    fs::metadata(SERVER).map_err(|_| format!("{SERVER} is not here: this needs pkg:llama-cpp"))?;
    if !cpu_can_run_it() {
        return Err("pkg:llama-cpp is built for AVX2, FMA, F16C and BMI2, and this CPU lacks one".into());
    }
    if load.gpu && this_machines_gpu().is_empty() {
        return Err("--gpu needs NVIDIA's /dev/nvidiactl and /dev/nvidia0, and this machine has neither".into());
    }
    let exe = program_for_work(home)?;
    claim_socket(&dir.join(SOCKET_NAME))?;
    claim_socket(&dir.join(STOP_NAME))?;
    let model = fs::File::open(resolve_model(home, &load.model)?).map_err(|e| format!("{}: {e}", load.model.display()))?;
    let listen = UnixListener::bind(dir.join(SOCKET_NAME)).map_err(|e| format!("{}: {e}", dir.display()))?;
    let stop = UnixListener::bind(dir.join(STOP_NAME)).map_err(|e| format!("{}: {e}", dir.display()))?;
    let fds = vec![listen.as_raw_fd(), stop.as_raw_fd(), model.as_raw_fd()];
    let mut c = as_work(&exe, WORK_STAGE);
    for fd in &fds {
        c.arg(fd.to_string());
    }
    c.args(load.stage_args());
    if !extra.is_empty() {
        c.arg("--").args(extra);
    }
    c.stdin(Stdio::null());
    let detach = log.is_some();
    if let Some(log) = &log {
        c.stdout(log.try_clone().map_err(|e| e.to_string())?);
        c.stderr(log.try_clone().map_err(|e| e.to_string())?);
    }
    hand_over(&mut c, fds, detach);
    let child = c.spawn().map_err(|e| format!("{SUDO}: {e}"))?;
    fs::write(dir.join(RUNNING_NAME), load.json()?).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(child)
}

/// `writ serve`: a server, here, until it ends. `writ llama start` is the same one, in the background.
pub fn main(args_in: &[OsString]) -> Result<i32, String> {
    let o = options(args_in)?;
    let home = sandbox::home()?;
    let given = o.model.as_ref().ok_or("writ serve needs --model FILE")?;
    let base = Load { model: given.clone(), threads: None, context: CPU_CONTEXT, gpu: false, share: 100 };
    let load = o.over(base, &home)?;
    let dir = prepare_dir(&home)?;
    let held = lock(&dir)?;
    let mut child = start_as_work(&home, &load, &o.extra, &dir, None)?;
    drop(held);
    eprintln!("writ: serving {}, as `work`, on {}", load.say(), dir.join(SOCKET_NAME).display());
    let status = child.wait().map_err(|e| e.to_string())?;
    forget_server(&dir);
    Ok(status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(0)))
}

/// The end of a server's log, for a sentence about why it stopped. It is the server's own words.
fn log_tail(dir: &Path) -> String {
    let text = fs::read_to_string(dir.join(LOG_NAME)).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    term::scrub(&lines[lines.len().saturating_sub(8)..].join("\n"))
}

fn wait_until_ready(dir: &Path, mut child: Option<&mut Child>, cancel: &model::Cancel) -> Result<(), String> {
    let sock = dir.join(SOCKET_NAME);
    let endpoint = model::Endpoint::Local(sock.clone());
    let start = Instant::now();
    while !model::ready(&endpoint) {
        if cancel.fired() {
            return Err("interrupted: the server goes on loading".into());
        }
        let gone = match &mut child {
            Some(c) => c.try_wait().map_err(|e| e.to_string())?.is_some(),
            None => UnixStream::connect(&sock).is_err(),
        };
        if gone {
            forget_server(dir);
            return Err(format!("the server stopped while it was loading. Its log says:\n{}", log_tail(dir)));
        }
        if start.elapsed() > LOAD_LIMIT {
            return Err(format!("the server had not loaded after {} seconds; see {}", LOAD_LIMIT.as_secs(), dir.join(LOG_NAME).display()));
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    Ok(())
}

/// Have the chosen model loaded, starting a server if none is up, and waiting if one is loading.
pub fn ensure_loaded(home: &Path, say: &mut dyn FnMut(&str), cancel: &model::Cancel) -> Result<(), String> {
    load(home, None, say, cancel)
}

fn load(home: &Path, chosen: Option<&Load>, say: &mut dyn FnMut(&str), cancel: &model::Cancel) -> Result<(), String> {
    let dir = prepare_dir(home)?;
    let held = lock(&dir)?;
    if UnixStream::connect(dir.join(SOCKET_NAME)).is_ok() {
        drop(held);
        return wait_until_ready(&dir, None, cancel);
    }
    let load = match (chosen, saved(home)?) {
        (Some(l), _) => l.clone(),
        (None, Some(l)) => l,
        (None, None) => Load::for_this_machine(only_model(&models_dir(home))?),
    };
    let log = fs::File::create(dir.join(LOG_NAME)).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut child = start_as_work(home, &load, &[], &dir, Some(log))?;
    drop(held);
    say(&format!("loading {}", load.say()));
    let start = Instant::now();
    let ready = wait_until_ready(&dir, Some(&mut child), cancel);
    // It is this process's child for as long as this lasts, and a terminal session can outlast it.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    ready?;
    say(&format!("loaded in {} seconds", start.elapsed().as_secs()));
    Ok(())
}

/// Stop the server, and free what it held. Whether there was one to stop.
pub fn stop(home: &Path) -> Result<bool, String> {
    let dir = prepare_dir(home)?;
    let _held = lock(&dir)?;
    let Ok(mut s) = UnixStream::connect(dir.join(STOP_NAME)) else {
        forget_server(&dir);
        return Ok(false);
    };
    s.set_read_timeout(Some(Duration::from_secs(30))).map_err(|e| e.to_string())?;
    // Nothing is sent: the connection ends when the server has.
    let mut byte = [0u8; 1];
    if let Err(e) = s.read(&mut byte) {
        return Err(format!("the server did not stop in 30 seconds: {e}"));
    }
    forget_server(&dir);
    Ok(true)
}

/// One line on the server: what it has loaded, or that it has not.
pub fn status(home: &Path) -> String {
    let dir = socket_dir(home);
    let sock = dir.join(SOCKET_NAME);
    if UnixStream::connect(&sock).is_err() {
        return "not loaded".into();
    }
    let what = running(&dir).map_or_else(|| "a server".to_string(), |l| l.say());
    if model::ready(&model::Endpoint::Local(sock)) {
        format!("loaded: {what}")
    } else {
        format!("loading: {what}")
    }
}

/// Which model, and with `local`, switch to it and make sure it is loaded. New settings restart a
/// server that has others, which ends what it was doing. What comes back is one line to show.
pub fn use_local(home: &Path, args: &[OsString], say: &mut dyn FnMut(&str), cancel: &model::Cancel) -> Result<String, String> {
    let Some(which) = args.first() else {
        return Ok(format!("local: {}", status(home)));
    };
    if which != "local" {
        return Err(format!("{} is not a model writ can use: only `local`, so far", which.to_string_lossy()));
    }
    let o = options(&args[1..])?;
    if !o.extra.is_empty() {
        return Err("arguments for the server go to `writ serve`".into());
    }
    let base = match (saved(home)?, &o.model) {
        (Some(l), _) => l,
        (None, Some(m)) => Load::for_this_machine(resolve_model(home, m)?),
        (None, None) => Load::for_this_machine(only_model(&models_dir(home))?),
    };
    let chosen = o.over(base, home)?;
    let dir = prepare_dir(home)?;
    if UnixStream::connect(dir.join(SOCKET_NAME)).is_ok() && running(&dir).as_ref() != Some(&chosen) {
        say("restarting the server with these settings");
        stop(home)?;
    }
    load(home, Some(&chosen), say, cancel)?;
    // Only now: a model that would not load is not what later sessions should try.
    let path = settings_path(home);
    if let Some(d) = path.parent() {
        fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    fs::write(&path, chosen.json()?).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!("local: {}", status(home)))
}

/// The local server by hand. What comes back is one line to show.
pub fn llama(home: &Path, args: &[OsString], say: &mut dyn FnMut(&str), cancel: &model::Cancel) -> Result<String, String> {
    match args.first().and_then(|a| a.to_str()) {
        Some("start") => {
            ensure_loaded(home, say, cancel)?;
            Ok(status(home))
        }
        Some("stop") => Ok(if stop(home)? { "stopped" } else { "not loaded" }.to_string()),
        Some("status") | None => Ok(status(home)),
        Some(other) => Err(format!("writ llama takes start, stop or status, not {other}")),
    }
}

/// `writ use`.
pub fn use_main(args: &[OsString]) -> Result<i32, String> {
    println!("{}", use_local(&sandbox::home()?, args, &mut |t| eprintln!("writ: {t}"), &model::Cancel::default())?);
    Ok(0)
}

/// `writ llama`.
pub fn llama_main(args: &[OsString]) -> Result<i32, String> {
    println!("{}", llama(&sandbox::home()?, args, &mut |t| eprintln!("writ: {t}"), &model::Cancel::default())?);
    Ok(0)
}

/// One connection, both ways. Either end closing ends it, or a dropped request would run on.
fn relay_one(client: UnixStream, server: UnixStream) {
    let (Ok(mut client_in), Ok(mut server_in)) = (client.try_clone(), server.try_clone()) else { return };
    let (Ok(client_end), Ok(server_end)) = (client.try_clone(), server.try_clone()) else { return };
    let (mut client_out, mut server_out) = (client, server);
    let up = std::thread::spawn(move || {
        let _ = std::io::copy(&mut client_in, &mut server_out);
        let _ = server_out.shutdown(Shutdown::Both);
        let _ = client_end.shutdown(Shutdown::Both);
    });
    let _ = std::io::copy(&mut server_in, &mut client_out);
    let _ = client_out.shutdown(Shutdown::Both);
    let _ = server_end.shutdown(Shutdown::Both);
    let _ = up.join();
}

/// Carry each connection to `home`'s socket across to the server's own. Before the server is
/// listening, a connection just closes, which is what "not loaded yet" looks like to `home`.
fn relay(listener: UnixListener, to: PathBuf) {
    std::thread::spawn(move || {
        for client in listener.incoming().flatten() {
            let to = to.clone();
            std::thread::spawn(move || {
                if let Ok(server) = UnixStream::connect(&to) {
                    relay_one(client, server);
                }
            });
        }
    });
}

/// The stage `work` runs: the box, the relay and the share, until the server ends or is stopped.
pub fn work_main(args: &[OsString]) -> Result<i32, String> {
    if unsafe { sys::getuid() } != WORK_UID {
        return Err("this stage runs only as `work`: `writ serve` gets there through sudo".into());
    }
    let [listen, stop, model_fd, rest @ ..] = args else {
        return Err("serve-work: the sockets' and the model's descriptors, then options".into());
    };
    let fd = |v: &OsString| -> Result<i32, String> { v.to_str().and_then(|n| n.parse().ok()).ok_or("a descriptor is a number".to_string()) };
    let (listen, stop, model_fd) = (fd(listen)?, fd(stop)?, fd(model_fd)?);
    // SAFETY: `home` opened these for this stage, and nothing else here holds them.
    let (listen, stop) = unsafe { (UnixListener::from_raw_fd(listen), UnixListener::from_raw_fd(stop)) };
    let o = options(rest)?;
    let (gpu, share) = (o.gpu.unwrap_or(false), o.share.unwrap_or(100));
    let nodes = if gpu { this_machines_gpu() } else { Vec::new() };
    let dir = work_dir(WORK_DIR_NAME)?;
    let sock = dir.join(SOCKET_NAME);
    claim_socket(&sock)?;
    if landlock::abi() == 0 {
        eprintln!("writ: this kernel has no Landlock; the box rests on namespaces and seccomp");
    }

    let mut c = command(model_fd, &dir, &nodes)?;
    c.args(server_args(model_fd, o.threads, o.context.unwrap_or(CPU_CONTEXT), gpu, &o.extra));
    let mut child = c.spawn().map_err(|e| format!("bwrap: {e}"))?;
    if gpu {
        // Not silent: an escape here has the driver's whole surface to try.
        eprintln!("writ: the box has the GPU: {}", nodes.iter().map(|n| n.display().to_string()).collect::<Vec<_>>().join(" "));
    }
    relay(listen, sock.clone());
    let stopping = Arc::new(AtomicBool::new(false));
    {
        let stopping = stopping.clone();
        std::thread::spawn(move || {
            if let Ok((held, _)) = stop.accept() {
                stopping.store(true, Ordering::Relaxed);
                // Open until this process ends, which is how `home` knows that it has.
                std::mem::forget(held);
            }
        });
    }
    let done = Arc::new(AtomicBool::new(false));
    let holding = (share < 100).then(|| hold_to_share(child.id(), share, done.clone()));

    let clean = |to_err: bool| {
        let mut clean = term::Clean::default();
        let mut left = LOG_LIMIT;
        move |b: &[u8]| {
            let text = if b.is_empty() { clean.finish() } else { clean.push(b) };
            let n = (text.len() as u64).min(left) as usize;
            left -= n as u64;
            if to_err {
                let _ = std::io::stderr().write_all(&text.as_bytes()[..n]);
            } else {
                let mut o = std::io::stdout();
                let _ = o.write_all(&text.as_bytes()[..n]);
                let _ = o.flush();
            }
        }
    };
    let t1 = sandbox::drain(child.stdout.take().ok_or("no stdout")?, clean(false));
    let t2 = sandbox::drain(child.stderr.take().ok_or("no stderr")?, clean(true));
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if stopping.load(Ordering::Relaxed) {
            let _ = child.kill();
            break child.wait().map_err(|e| e.to_string())?;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    done.store(true, Ordering::Relaxed);
    if let Some(h) = holding {
        let _ = h.join();
    }
    let _ = t1.join();
    let _ = t2.join();
    let _ = fs::remove_file(&sock);
    if stopping.load(Ordering::Relaxed) {
        return Ok(0);
    }
    Ok(status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(0)))
}

/// Attacks from inside a server's box, printed for `writ check`, one `name state detail` a line.
pub fn probe(rest: &[OsString]) -> ! {
    use crate::probe::{broken, filter_refuses, leak, refused, works};
    let [home, model, model_fd, env_text, host_pid, gpu @ ..] = rest else {
        eprintln!("probe-serve: home, model, its descriptor, a value from the host's environment, a host PID, GPU nodes");
        std::process::exit(inner::REFUSED_TO_START);
    };
    if unsafe { sys::prctl(sys::PR_GET_SECCOMP, 0i64, 0i64, 0i64, 0i64) } != 2 {
        eprintln!("probe-serve runs only in a box");
        std::process::exit(inner::REFUSED_TO_START);
    }

    // The same attacks, named apart, on a box with a GPU.
    let tag = if gpu.is_empty() { "serve" } else { "serve gpu" };
    let refused = |name: &str, why: &str| refused(&name.replacen("serve", tag, 1), why);
    let leak = |name: &str, why: &str| leak(&name.replacen("serve", tag, 1), why);
    let works = |name: &str| works(&name.replacen("serve", tag, 1));
    let broken = |name: &str, why: &str| broken(&name.replacen("serve", tag, 1), why);

    let pid_path = format!("/proc/{}", host_pid.to_string_lossy());
    let model_fd = model_fd.to_string_lossy().into_owned();
    let model_in_box = format!("/proc/self/fd/{model_fd}");
    if unsafe { sys::getuid() } == WORK_UID {
        refused("serve: the server is `work`, not `home`", "uid 2");
    } else {
        leak("serve: the server is `work`, not `home`", "another account");
    }
    // What it was handed, and nothing of what the stage outside holds.
    let names: Vec<String> = fs::read_dir("/proc/self/fd").map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
    let open: Vec<String> = names
        .iter()
        .filter(|n| !["0", "1", "2"].contains(&n.as_str()) && **n != model_fd)
        .filter_map(|n| fs::read_link(format!("/proc/self/fd/{n}")).ok())
        .map(|p| p.display().to_string())
        .collect();
    if open.is_empty() {
        refused("serve: no descriptor but the model's came in", "none");
    } else {
        leak("serve: no descriptor but the model's came in", &open.join(" "));
    }
    match fs::read_dir(format!("{model_in_box}/..")) {
        Ok(_) => leak("serve: the model's descriptor is no way to its directory", "it was listed"),
        Err(e) => refused("serve: the model's descriptor is no way to its directory", &e.to_string()),
    }
    for (name, path) in [
        ("serve: the host's home is not here", Path::new(home)),
        ("serve: the model's real path is not here", Path::new(model)),
        ("serve: /etc is not here", Path::new("/etc")),
        ("serve: /work is not here", Path::new("/work")),
        ("serve: /oath/log is not here", Path::new("/oath/log")),
        ("serve: /oath/ssh is not here", Path::new("/oath/ssh")),
        ("serve: /oath/run is not here", Path::new("/oath/run")),
        ("serve: /run is not here", Path::new("/run")),
        ("serve: /dev/kvm is not here", Path::new("/dev/kvm")),
        ("serve: /dev/nvme0n1 is not here", Path::new("/dev/nvme0n1")),
        ("serve: /dev/sda is not here", Path::new("/dev/sda")),
        ("serve: the display is not here", Path::new("/dev/nvidia-modeset")),
        ("serve: the display's card is not here", Path::new("/dev/dri")),
        ("serve: the driver's privileges are not here", Path::new("/dev/nvidia-caps")),
        ("serve: the driver's tools are not here", Path::new("/dev/nvidia-uvm-tools")),
        ("serve: the keyboard and mouse are not here", Path::new("/dev/input")),
        ("serve: a host process is not here", Path::new(&pid_path)),
    ] {
        match fs::symlink_metadata(path) {
            Err(_) => refused(name, "absent"),
            Ok(_) => leak(name, &format!("{} exists", path.display())),
        }
    }
    for (name, path) in [
        ("serve: the model cannot be changed", model_in_box.as_str()),
        ("serve: /bin cannot be written", "/bin/x"),
        ("serve: the root cannot be written", "/x"),
    ] {
        match fs::OpenOptions::new().write(true).create(true).open(path) {
            Err(e) => refused(name, &e.to_string()),
            Ok(_) => leak(name, &format!("wrote {path}")),
        }
    }
    // A refused connection proves nothing, so count what can carry one.
    match fs::read_to_string("/proc/net/dev") {
        Ok(d) => {
            let links: Vec<&str> = d.lines().skip(2).filter_map(|l| l.split(':').next()).map(str::trim).collect();
            if links.iter().all(|l| *l == "lo") {
                refused("serve: no network link", "only loopback");
            } else {
                leak("serve: no network link", &links.join(" "));
            }
        }
        Err(e) => broken("serve: no network link", &e.to_string()),
    }
    let env_text = env_text.to_string_lossy();
    for path in ["/proc/self/environ", "/proc/1/environ"] {
        let name = format!("serve: the host's environment is not in {path}");
        match fs::read(path) {
            Ok(b) if String::from_utf8_lossy(&b).contains(env_text.as_ref()) => leak(&name, "a key is there"),
            Ok(_) => refused(&name, "not there"),
            Err(e) => refused(&name, &e.to_string()),
        }
    }
    match Command::new("/bin/sudo").arg("-n").arg("id").output() {
        Ok(o) if o.status.success() => leak("serve: no sudo", "sudo ran"),
        Ok(o) => refused("serve: no sudo", &String::from_utf8_lossy(&o.stderr).replace('\n', " ")),
        Err(e) => refused("serve: no sudo", &e.to_string()),
    }
    match UnixListener::bind("/x.sock") {
        Ok(_) => leak("serve: a socket cannot be made elsewhere", "bound in /"),
        Err(e) => refused("serve: a socket cannot be made elsewhere", &e.to_string()),
    }
    filter_refuses();

    if gpu.is_empty() {
        for path in ["/dev/nvidiactl", "/dev/nvidia0"] {
            match fs::symlink_metadata(path) {
                Err(_) => refused("serve: no GPU unless asked", "absent"),
                Ok(_) => leak("serve: no GPU unless asked", &format!("{path} exists")),
            }
        }
    }
    for node in gpu {
        let name = format!("serve: {} can be opened", node.to_string_lossy());
        match fs::OpenOptions::new().read(true).write(true).open(node) {
            Ok(_) => works(&name),
            Err(e) => broken(&name, &e.to_string()),
        }
    }
    match fs::read(&model_in_box) {
        Ok(_) => works("serve: the model can be read"),
        Err(e) => broken("serve: the model can be read", &e.to_string()),
    }
    let sock = format!("{SOCKET_DIR_IN_BOX}/probe.sock");
    match UnixListener::bind(&sock) {
        Ok(_) => {
            works("serve: a socket can be made in its directory");
            let _ = fs::remove_file(&sock);
        }
        Err(e) => broken("serve: a socket can be made in its directory", &e.to_string()),
    }
    println!("done");
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_with(gpu: &[PathBuf]) -> String {
        let a = args(Path::new("/run/user/2/writ-llama"), Path::new("/oath/store/pkg/writ/h/bin/writ"), gpu);
        a.iter().map(|s| s.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ")
    }

    fn text() -> String {
        text_with(&[])
    }

    #[test]
    fn the_box_shares_nothing_and_binds_little() {
        let s = text();
        for need in ["--unshare-all", "--unshare-user", "--disable-userns", "--die-with-parent", "--new-session", "--clearenv", "--remount-ro /"] {
            assert!(s.contains(need), "{need}");
        }
        // The one thing from /run is the socket's directory, and it is /sock in the box.
        let s = s.replace("--bind /run/user/2/writ-llama /sock", "");
        for bad in ["--share-net", "--dev-bind", "--cap-add", "/etc", "/work", "/run", "/oath/log", "/oath/ssh", "--bind / "] {
            assert!(!s.contains(bad), "{bad} in {s}");
        }
    }

    #[test]
    fn the_model_is_no_mount_and_only_the_socket_directory_is_writable() {
        let s = text();
        assert!(!s.contains("gguf") && !s.contains("/home"));
        assert_eq!(s.matches("--bind ").count(), 1);
        assert!(s.contains("--bind /run/user/2/writ-llama /sock"));
    }

    #[test]
    fn the_server_is_told_the_descriptor_for_its_model() {
        let a: Vec<String> = server_args(7, None, 8192, false, &[]).iter().map(|s| s.to_string_lossy().into_owned()).collect();
        assert_eq!(a[1..3], ["--model".to_string(), "/proc/self/fd/7".to_string()]);
    }

    #[test]
    fn work_is_handed_what_it_needs_and_no_path_of_homes() {
        let load = Load { model: "/home/.local/share/writ/models/m.gguf".into(), threads: Some(4), context: 8192, gpu: true, share: 40 };
        let a: Vec<String> = load.stage_args().iter().map(|s| s.to_string_lossy().into_owned()).collect();
        assert_eq!(a, ["--context", "8192", "--threads", "4", "--gpu-share", "40"]);
        let c = as_work(Path::new("/bin/writ"), WORK_STAGE);
        let said: Vec<String> = c.get_args().map(|s| s.to_string_lossy().into_owned()).collect();
        assert_eq!(c.get_program(), SUDO);
        assert_eq!(said, ["-n", "-u", "work", "--", "/bin/writ", WORK_STAGE]);
    }

    #[test]
    fn settings_come_back_as_they_went_in() {
        for load in [
            Load { model: "/m/a.gguf".into(), threads: None, context: 32768, gpu: true, share: 40 },
            Load { model: "/m/b.gguf".into(), threads: Some(8), context: 16384, gpu: false, share: 100 },
        ] {
            assert_eq!(Load::parse(&load.json().unwrap()).unwrap(), load);
        }
        assert!(Load::parse("{}").is_err());
        assert!(Load::parse("not json").is_err());
    }

    #[test]
    fn options_change_what_they_name_and_leave_the_rest() {
        let home = Path::new("/nowhere");
        let base = Load { model: "/m/a.gguf".into(), threads: Some(8), context: 32768, gpu: true, share: 40 };
        let of = |args: &[&str]| options(&args.iter().map(OsString::from).collect::<Vec<_>>());
        assert_eq!(of(&[]).unwrap().over(base.clone(), home).unwrap(), base);
        let l = of(&["--gpu-share", "25"]).unwrap().over(base.clone(), home).unwrap();
        assert_eq!((l.share, l.context, l.gpu), (25, 32768, true));
        let l = of(&["--cpu"]).unwrap().over(base.clone(), home).unwrap();
        assert_eq!((l.gpu, l.share), (false, 100));
        for bad in [&["--gpu-share", "0"][..], &["--gpu-share", "101"], &["--context"], &["--nonsense"]] {
            assert!(of(bad).is_err(), "{bad:?}");
        }
        assert!(of(&["--", "--anything"]).unwrap().extra.len() == 1);
    }

    #[test]
    fn it_says_what_it_loaded() {
        let gpu = Load { model: "/m/a.gguf".into(), threads: None, context: 32768, gpu: true, share: 40 };
        assert_eq!(gpu.say(), "a.gguf, a 32768-token context, on the GPU at a 40% share");
        assert_eq!(Load { share: 100, ..gpu.clone() }.say(), "a.gguf, a 32768-token context, on the GPU");
        assert_eq!(Load { gpu: false, share: 100, ..gpu }.say(), "a.gguf, a 32768-token context, on the CPU");
    }

    #[test]
    fn a_model_is_found_by_name_and_must_let_others_read_it() {
        let d = std::env::temp_dir().join(format!("writ-models-{}", std::process::id()));
        let models = models_dir(&d);
        fs::create_dir_all(&models).unwrap();
        assert!(only_model(&models).unwrap_err().contains("no model"));
        let a = models.join("a.gguf");
        fs::write(&a, "x").unwrap();
        fs::write(models.join("a.gguf.sum"), "x").unwrap();
        assert_eq!(only_model(&models).unwrap(), a);
        fs::set_permissions(&a, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(resolve_model(&d, Path::new("a.gguf")).unwrap(), fs::canonicalize(&a).unwrap());
        assert_eq!(resolve_model(&d, &a).unwrap(), fs::canonicalize(&a).unwrap());
        assert!(resolve_model(&d, Path::new("none.gguf")).is_err());
        fs::set_permissions(&a, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(resolve_model(&d, &a).unwrap_err().contains("chmod o+r"));
        fs::set_permissions(&a, fs::Permissions::from_mode(0o666)).unwrap();
        assert!(resolve_model(&d, &a).unwrap_err().contains("chmod o-w"));
        fs::write(models.join("b.gguf"), "x").unwrap();
        assert!(only_model(&models).unwrap_err().contains("2 models"));
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_program_under_home_is_not_one_work_can_run() {
        let exe = std::env::current_exe().unwrap();
        assert!(program_for_work(exe.parent().unwrap()).unwrap_err().contains("under your home"));
        assert_eq!(program_for_work(Path::new("/nowhere")).unwrap(), exe);
    }

    #[test]
    fn the_relay_carries_both_ways_and_an_end_closing_ends_it() {
        let d = std::env::temp_dir().join(format!("writ-relay-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let (outer, inner) = (d.join("outer.sock"), d.join("inner.sock"));
        let server = UnixListener::bind(&inner).unwrap();
        relay(UnixListener::bind(&outer).unwrap(), inner.clone());

        let mut client = UnixStream::connect(&outer).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let (mut far, _) = server.accept().unwrap();
        far.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut buf = [0u8; 5];
        client.write_all(b"hello").unwrap();
        far.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"hello");
        // A piece of a stream arrives when it is sent, not when a buffer fills.
        far.write_all(b"token").unwrap();
        client.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"token");
        // The client gone is the server told, which is how a dropped request stops running.
        drop(client);
        assert_eq!(far.read(&mut buf).unwrap(), 0);

        let mut client = UnixStream::connect(&outer).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let (far, _) = server.accept().unwrap();
        drop(far);
        assert_eq!(client.read(&mut buf).unwrap(), 0);

        // No server: the connection closes, and nothing stays waiting.
        drop(server);
        fs::remove_file(&inner).unwrap();
        let mut client = UnixStream::connect(&outer).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        assert_eq!(client.read(&mut buf).unwrap(), 0);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn the_server_opens_no_port_and_no_web_ui() {
        let a: Vec<String> = server_args(3, Some(4), 8192, false, &[]).iter().map(|s| s.to_string_lossy().into_owned()).collect();
        let host = a.iter().position(|x| x == "--host").unwrap();
        assert!(a[host + 1].ends_with(".sock"));
        assert!(a.iter().any(|x| x == "--no-webui"));
        let cors = a.iter().position(|x| x == "--cors-origins").unwrap();
        assert_eq!(a[cors + 1], "localhost");
        for bad in ["--port", "--tools", "--mcp-servers-config", "--ui-mcp-proxy", "--rpc", "--media-path"] {
            assert!(!a.iter().any(|x| x == bad), "{bad}");
        }
    }

    #[test]
    fn a_server_takes_a_gpu_only_when_asked_and_all_of_a_model_or_none() {
        let layers = |gpu| {
            let a: Vec<String> = server_args(3, None, 8192, gpu, &[]).iter().map(|s| s.to_string_lossy().into_owned()).collect();
            let i = a.iter().position(|x| x == "--n-gpu-layers").unwrap();
            assert_eq!(a[a.iter().position(|x| x == "--fit").unwrap() + 1], "off");
            a[i + 1].clone()
        };
        assert_eq!(layers(false), "0");
        assert_eq!(layers(true), "all");
    }

    #[test]
    fn a_gpu_brings_its_compute_nodes_and_nothing_of_the_display() {
        let d = std::env::temp_dir().join(format!("writ-gpu-{}", std::process::id()));
        fs::create_dir_all(d.join("dri")).unwrap();
        fs::create_dir_all(d.join("nvidia-caps")).unwrap();
        for name in ["nvidia0", "nvidia1", "nvidiactl", "nvidia-modeset", "nvidia-uvm", "nvidia-uvm-tools", "nvidiax", "kvm"] {
            fs::write(d.join(name), "").unwrap();
        }
        let names: Vec<String> = gpu_nodes(&d).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["nvidia0", "nvidia1", "nvidiactl"]);
        fs::remove_dir_all(&d).unwrap();
        assert!(gpu_nodes(&d).is_empty());

        let s = text_with(&[PathBuf::from("/dev/nvidiactl"), PathBuf::from("/dev/nvidia0")]);
        assert!(s.contains("--dev-bind /dev/nvidiactl /dev/nvidiactl"));
        assert!(s.contains("--dev-bind /dev/nvidia0 /dev/nvidia0"));
        assert_eq!(s.matches("--dev-bind").count(), 2);
        for bad in ["modeset", "/dev/dri", "nvidia-caps", "uvm", "--dev-bind /dev /dev"] {
            assert!(!s.contains(bad), "{bad} in {s}");
        }
        let plain = text();
        assert!(!plain.contains("--dev-bind") && !plain.contains("VK_LOADER"));
    }

    /// A real child: a shell under a shell, as bwrap is under writ.
    #[test]
    fn the_deepest_process_is_found_under_a_root() {
        let mut c = Command::new("/bin/sh").args(["-c", "/bin/sleep 30 & wait"]).spawn().unwrap();
        let found = (0..100).find_map(|_| {
            std::thread::sleep(Duration::from_millis(20));
            deepest_under(c.id()).filter(|pid| fs::read_to_string(format!("/proc/{pid}/comm")).is_ok_and(|n| n.trim() == "sleep"))
        });
        assert!(found.is_some());
        unsafe { sys::kill(found.unwrap() as i32, sys::SIGKILL) };
        c.wait().unwrap();
        assert_eq!(deepest_under(c.id()), None);
    }

    #[test]
    fn a_leftover_is_only_cleared_if_it_is_a_dead_socket() {
        let d = std::env::temp_dir().join(format!("writ-claim-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let f = d.join("file");
        fs::write(&f, "x").unwrap();
        assert!(claim_socket(&f).is_err());
        assert!(f.exists());
        let s = d.join("s.sock");
        let l = UnixListener::bind(&s).unwrap();
        assert!(claim_socket(&s).is_err());
        drop(l);
        assert!(claim_socket(&s).is_ok());
        assert!(!s.exists());
        assert!(claim_socket(&s).is_ok());
        fs::remove_dir_all(&d).unwrap();
    }
}
