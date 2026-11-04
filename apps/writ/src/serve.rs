//! A local model, served from a box (R65). A model file is untrusted input to a large C++
//! parser, and this keeps a bad one from mattering: the server sees its model read-only and one
//! directory for its socket. It has no network, no home and no way back through `sudo`, and
//! nothing but `home`'s own programs can reach the socket, since no port is open.
//!
//! A GPU is the one thing `--gpu` adds, and it is a real cost: the driver's ioctls are a large
//! surface in the kernel, and the box does not narrow them.

use crate::{inner, landlock, sandbox, sys, term};
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const SERVER: &str = "/bin/llama-server";
/// Where the model and the socket are in the box.
const MODEL_IN_BOX: &str = "/model.gguf";
const SOCKET_DIR_IN_BOX: &str = "/sock";
const SOCKET_NAME: &str = "llama.sock";
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

/// Where the socket is on the host. Only `home` can reach this directory.
pub fn socket_dir(home: &Path) -> PathBuf {
    home.join(".local/share/writ/llama")
}

pub fn socket(home: &Path) -> PathBuf {
    socket_dir(home).join(SOCKET_NAME)
}

/// bwrap's arguments for a server. Not the project's box: nothing here is writable but the
/// socket's directory and a small /tmp, so there is nothing to hold or apply.
fn args(model: &Path, sock_dir: &Path, exe: &Path, gpu: &[PathBuf]) -> Vec<OsString> {
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
    push(&[s("--ro-bind"), model.as_os_str(), s(MODEL_IN_BOX)]);
    push(&[s("--bind"), sock_dir.as_os_str(), s(SOCKET_DIR_IN_BOX)]);
    push(&[s("--ro-bind"), exe.as_os_str(), s(sandbox::INNER)]);
    push(&[s("--remount-ro"), s("/"), s("--chdir"), s("/tmp")]);
    a
}

/// The server's arguments. No port: `.sock` makes it a Unix socket. `--jinja` runs the model's
/// own template, which is how tool calls work.
fn server_args(threads: Option<u32>, context: u32, gpu: bool, extra: &[OsString]) -> Vec<OsString> {
    let mut a: Vec<OsString> = vec![SERVER.into(), "--model".into(), MODEL_IN_BOX.into()];
    a.push("--host".into());
    a.push(format!("{SOCKET_DIR_IN_BOX}/{SOCKET_NAME}").into());
    a.extend(["--no-webui".into(), "--jinja".into(), "--parallel".into(), "1".into()]);
    // Its default echoes any origin back as allowed. No browser reaches a socket; still.
    a.extend(["--cors-origins".into(), "localhost".into()]);
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

/// bwrap with the box's second stage, ready for the program to run in it.
fn command(model: &Path, dir: &Path, gpu: &[PathBuf]) -> Result<Command, String> {
    sandbox::trusted_bwrap()?;
    let exe = std::env::current_exe().map_err(|e| format!("own path: {e}"))?;
    let mut c = Command::new(sandbox::BWRAP);
    c.env_clear();
    c.args(args(model, dir, &exe, gpu));
    c.arg(sandbox::INNER).arg("inner");
    for w in [Path::new("/tmp"), Path::new(SOCKET_DIR_IN_BOX)] {
        c.arg("--write").arg(w);
    }
    c.arg("--");
    c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    Ok(c)
}

/// The attacks in a server's box, for `writ check`, with its own model and socket directory.
pub fn probe_command(home: &Path, model: &Path, dir: &Path, env_text: &str, gpu: &[PathBuf]) -> Result<Command, String> {
    let mut c = command(model, dir, gpu)?;
    c.arg(sandbox::INNER).arg("probe-serve").arg(home).arg(model).arg(env_text);
    c.arg(std::process::id().to_string());
    c.args(gpu);
    Ok(c)
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

pub fn main(args_in: &[OsString]) -> Result<i32, String> {
    let mut model: Option<PathBuf> = None;
    let mut threads: Option<u32> = None;
    let mut context: u32 = 16384;
    let mut gpu = false;
    let mut share: u32 = 100;
    let mut extra: Vec<OsString> = Vec::new();
    let mut it = args_in.iter();
    let number = |v: Option<&OsString>, what: &str| -> Result<u32, String> {
        v.and_then(|s| s.to_str()).and_then(|s| s.parse().ok()).ok_or(format!("{what} takes a number"))
    };
    while let Some(a) = it.next() {
        match a.to_str() {
            Some("--model") => model = it.next().map(PathBuf::from),
            Some("--threads") => threads = Some(number(it.next(), "--threads")?),
            Some("--context") => context = number(it.next(), "--context")?,
            Some("--gpu") => gpu = true,
            Some("--gpu-share") => {
                share = number(it.next(), "--gpu-share")?;
                if !(1..=100).contains(&share) {
                    return Err("--gpu-share is a percentage, 1 to 100".into());
                }
                gpu = true;
            }
            Some("--") => {
                extra = it.by_ref().cloned().collect();
                break;
            }
            _ => return Err(format!("unknown argument {}", a.to_string_lossy())),
        }
    }
    let model = model.ok_or("writ serve needs --model FILE")?;
    let model = fs::canonicalize(&model).map_err(|e| format!("{}: {e}", model.display()))?;
    if !fs::metadata(&model).map_err(|e| e.to_string())?.is_file() {
        return Err(format!("{} is not a file", model.display()));
    }
    fs::metadata(SERVER).map_err(|_| format!("{SERVER} is not here: this needs pkg:llama-cpp"))?;
    if !cpu_can_run_it() {
        return Err("pkg:llama-cpp is built for AVX2, FMA, F16C and BMI2, and this CPU lacks one".into());
    }
    let nodes = if gpu { this_machines_gpu() } else { Vec::new() };
    if gpu && nodes.is_empty() {
        return Err("--gpu needs NVIDIA's /dev/nvidiactl and /dev/nvidia0, and this machine has neither".into());
    }
    let home = sandbox::home()?;
    let dir = socket_dir(&home);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(|e| format!("{}: {e}", dir.display()))?;
    let sock = dir.join(SOCKET_NAME);
    claim_socket(&sock)?;
    if landlock::abi() == 0 {
        eprintln!("writ: this kernel has no Landlock; the box rests on namespaces and seccomp");
    }

    let mut c = command(&model, &dir, &nodes)?;
    c.args(server_args(threads, context, gpu, &extra));
    let mut child = c.spawn().map_err(|e| format!("bwrap: {e}"))?;
    eprintln!("writ: serving {} on {}", model.display(), sock.display());
    if gpu {
        // Not silent: an escape here has the driver's whole surface to try.
        eprintln!("writ: the box has the GPU: {}", nodes.iter().map(|n| n.display().to_string()).collect::<Vec<_>>().join(" "));
    }
    let done = Arc::new(AtomicBool::new(false));
    let holding = (share < 100).then(|| hold_to_share(child.id(), share, done.clone()));

    let clean = |to_err: bool| {
        let mut clean = term::Clean::default();
        move |b: &[u8]| {
            let text = if b.is_empty() { clean.finish() } else { clean.push(b) };
            if to_err {
                let _ = std::io::stderr().write_all(text.as_bytes());
            } else {
                let mut o = std::io::stdout();
                let _ = o.write_all(text.as_bytes());
                let _ = o.flush();
            }
        }
    };
    let t1 = sandbox::drain(child.stdout.take().ok_or("no stdout")?, clean(false));
    let t2 = sandbox::drain(child.stderr.take().ok_or("no stderr")?, clean(true));
    let (status, _) = sandbox::wait(&mut child, None)?;
    done.store(true, Ordering::Relaxed);
    if let Some(h) = holding {
        let _ = h.join();
    }
    let _ = t1.join();
    let _ = t2.join();
    let _ = fs::remove_file(&sock);
    use std::os::unix::process::ExitStatusExt;
    Ok(status.code().unwrap_or_else(|| 128 + status.signal().unwrap_or(0)))
}

/// Attacks from inside a server's box, printed for `writ check`, one `name state detail` a line.
pub fn probe(rest: &[OsString]) -> ! {
    use crate::probe::{broken, filter_refuses, leak, refused, works};
    let [home, model, env_text, host_pid, gpu @ ..] = rest else {
        eprintln!("probe-serve: home, model, a value from the host's environment, a host PID, GPU nodes");
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
        ("serve: the model cannot be changed", MODEL_IN_BOX),
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
    match fs::read(MODEL_IN_BOX) {
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
        let a = args(Path::new("/home/.local/share/writ/models/m.gguf"), Path::new("/home/.local/share/writ/llama"), Path::new("/oath/store/pkg/writ/h/bin/writ"), gpu);
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
        for bad in ["--share-net", "--dev-bind", "--cap-add", "/etc", "/work", "/run", "/oath/log", "/oath/ssh", "--bind / "] {
            assert!(!s.contains(bad), "{bad} in {s}");
        }
    }

    #[test]
    fn the_model_is_read_only_and_only_the_socket_directory_is_writable() {
        let s = text();
        assert!(s.contains("--ro-bind /home/.local/share/writ/models/m.gguf /model.gguf"));
        assert_eq!(s.matches("--bind ").count(), 1);
        assert!(s.contains("--bind /home/.local/share/writ/llama /sock"));
    }

    #[test]
    fn the_server_opens_no_port_and_no_web_ui() {
        let a: Vec<String> = server_args(Some(4), 8192, false, &[]).iter().map(|s| s.to_string_lossy().into_owned()).collect();
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
            let a: Vec<String> = server_args(None, 8192, gpu, &[]).iter().map(|s| s.to_string_lossy().into_owned()).collect();
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
