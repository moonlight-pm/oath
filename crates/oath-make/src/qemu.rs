use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde_json::json;

use crate::tools::Tools;
use crate::util::{kvm, sha256_file, utc_rfc3339, utc_stamp, write_pretty};

pub struct Image {
    pub kernel: PathBuf,
    pub initrd: PathBuf,
    pub backing: PathBuf,
}

pub fn load_image(out: &Path) -> Result<Image> {
    let kernel =
        std::env::var_os("OATH_KERNEL").map(PathBuf::from).unwrap_or_else(|| out.join("bzImage"));
    let backing =
        std::env::var_os("OATH_IMAGE").map(PathBuf::from).unwrap_or_else(|| out.join("oath.qcow2"));
    let initrd = out.join("initrd.gz");
    for p in [&kernel, &backing, &initrd] {
        if !p.is_file() {
            bail!("missing {} — run: cargo make build", p.display());
        }
    }
    Ok(Image { kernel, initrd, backing })
}

pub fn new_run(out: &Path, label: &str) -> Result<PathBuf> {
    let dir = out.join("runs").join(format!("{}-{label}", utc_stamp()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn overlay_disk(tools: &Tools, run: &Path, backing: &Path) -> Result<PathBuf> {
    let overlay = run.join("disk.qcow2");
    if !overlay.is_file() {
        let backing = fs::canonicalize(backing)?;
        crate::util::run(Command::new(&tools.qemu_img).args([
            "create",
            "-f",
            "qcow2",
            "-F",
            "qcow2",
            "-b",
            backing.to_str().unwrap(),
            overlay.to_str().unwrap(),
        ]))?;
    }
    Ok(overlay)
}

pub fn write_meta(
    run: &Path,
    tools: &Tools,
    img: &Image,
    overlay: &Path,
    kind: &str,
) -> Result<()> {
    write_pretty(
        &run.join("meta.json"),
        &json!({
            "started": utc_rfc3339(),
            "kind": kind,
            "qemu": tools.qemu.display().to_string(),
            "kvm": kvm(),
            "kernel": img.kernel.display().to_string(),
            "initrd": img.initrd.display().to_string(),
            "backing": img.backing.display().to_string(),
            "overlay": overlay.display().to_string(),
            "sha256": {
                "kernel": sha256_file(&img.kernel).ok(),
                "initrd": sha256_file(&img.initrd).ok(),
                "backing": sha256_file(&img.backing).ok(),
            }
        }),
    )
}

#[derive(Clone, Copy)]
pub enum SerialMode {
    /// Guest serial on this process stdio (interactive / probe).
    Stdio,
    /// Guest serial only in `serial_log` (headless; Ctrl-C / stop kill QEMU).
    File,
}

pub fn qemu_args(
    tools: &Tools,
    img: &Image,
    overlay: &Path,
    serial_log: &Path,
    qemu_log: &Path,
    serial: SerialMode,
    inject_authorized: Option<&Path>,
    window: bool,
) -> Vec<String> {
    let mut a = vec![tools.qemu.display().to_string(), "-machine".into(), "q35".into()];
    if kvm() {
        a.push("-enable-kvm".into());
    }
    a.extend([
        "-m".into(),
        "2048".into(),
        "-monitor".into(),
        "none".into(),
        "-vga".into(),
        "none".into(),
    ]);
    if window {
        a.extend(["-display".into(), "gtk".into()]);
    } else {
        a.extend(["-display".into(), "none".into()]);
    }
    match serial {
        SerialMode::Stdio => {
            a.extend([
                "-chardev".into(),
                format!("stdio,id=cons,logfile={},signal=off", serial_log.display()),
                "-serial".into(),
                "chardev:cons".into(),
            ]);
        }
        SerialMode::File => {
            a.extend(["-serial".into(), format!("file:{}", serial_log.display())]);
        }
    }
    a.extend([
        "-kernel".into(),
        img.kernel.display().to_string(),
        "-initrd".into(),
        img.initrd.display().to_string(),
        "-append".into(),
        "console=tty0 console=ttyS0 quiet loglevel=4 panic=10".into(),
        "-netdev".into(),
        netdev(),
        "-device".into(),
        "virtio-net-pci,netdev=n0".into(),
        "-device".into(),
        "virtio-rng-pci".into(),
        "-device".into(),
        "virtio-gpu-pci".into(),
        "-device".into(),
        "virtio-keyboard-pci".into(),
        "-device".into(),
        "virtio-mouse-pci".into(),
        "-drive".into(),
        format!("file={},if=virtio,format=qcow2,cache=writeback", overlay.display()),
        "-d".into(),
        "guest_errors".into(),
        "-D".into(),
        qemu_log.display().to_string(),
        "-no-reboot".into(),
    ]);
    if let Some(p) = inject_authorized {
        a.extend(["-fw_cfg".into(), format!("name=opt/oath/authorized,file={}", p.display())]);
    }
    a
}

fn host_wants_window() -> bool {
    match std::env::var("OATH_DISPLAY") {
        Ok(v) if v == "none" || v == "0" => false,
        Ok(_) => true,
        Err(_) => std::env::var_os("DISPLAY").is_some(),
    }
}

pub fn ssh_port() -> u16 {
    std::env::var("OATH_SSH_PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(2222)
}

/// Host public keys for QEMU fw_cfg inject. Derives .pub from default
/// private keys when missing (this host has `id_rsa` but no `id_rsa.pub`).
fn write_host_authorized(dest: &Path) -> Option<PathBuf> {
    let mut keys: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |line: String| {
        let line = line.trim().to_string();
        if !(line.starts_with("ssh-") || line.starts_with("ecdsa-")) {
            return;
        }
        let blob = line.split_whitespace().nth(1).unwrap_or("").to_string();
        if blob.is_empty() || !seen.insert(blob) {
            return;
        }
        keys.push(line);
    };
    if let Ok(p) = std::env::var("OATH_SSH_PUBKEY") {
        if let Ok(s) = fs::read_to_string(&p) {
            for l in s.lines() {
                push(l.to_string());
            }
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let ssh = PathBuf::from(home).join(".ssh");
        if let Ok(rd) = fs::read_dir(&ssh) {
            for e in rd.flatten() {
                let name = e.file_name();
                let n = name.to_string_lossy();
                if n.ends_with(".pub") {
                    if let Ok(s) = fs::read_to_string(e.path()) {
                        for l in s.lines() {
                            push(l.to_string());
                        }
                    }
                }
            }
        }
        for name in ["id_ed25519", "id_ecdsa", "id_ecdsa_sk", "id_ed25519_sk", "id_rsa", "id_dsa"] {
            let privk = ssh.join(name);
            if privk.is_file() && !ssh.join(format!("{name}.pub")).is_file() {
                if let Ok(o) = Command::new("ssh-keygen")
                    .args(["-y", "-f", privk.to_str().unwrap()])
                    .stdin(Stdio::null())
                    .output()
                {
                    if o.status.success() {
                        if let Ok(s) = String::from_utf8(o.stdout) {
                            push(s);
                        }
                    }
                }
            }
        }
    }
    if let Ok(o) = Command::new("ssh-add").args(["-L"]).stdin(Stdio::null()).output() {
        if o.status.success() {
            if let Ok(s) = String::from_utf8(o.stdout) {
                for l in s.lines() {
                    push(l.to_string());
                }
            }
        }
    }
    if keys.is_empty() {
        eprintln!(
            "no host SSH public keys found (need ~/.ssh/*.pub, default id_rsa, ssh-agent, or OATH_SSH_PUBKEY)"
        );
        return None;
    }
    let mut body = keys.join("\n");
    body.push('\n');
    if fs::write(dest, &body).is_err() {
        return None;
    }
    eprintln!("injecting {} SSH public key(s) into the guest", keys.len());
    dest.canonicalize().ok().or_else(|| Some(dest.to_path_buf()))
}

/// OpenSSH to the QEMU user-net hostfwd. Extra args are passed to ssh(1).
pub fn ssh(out: &Path, extra: &[String]) -> Result<i32> {
    if running_pid(out)?.is_none() {
        eprintln!("hint: no vm.pid — cargo make start  (or cargo make up in another terminal)");
    }
    let port = ssh_port();
    let mut cmd = Command::new("ssh");
    cmd.args([
        "-p",
        &port.to_string(),
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "UserKnownHostsFile=/dev/null",
        "-o",
        "GlobalKnownHostsFile=/dev/null",
        "root@127.0.0.1",
    ])
    .args(extra)
    .stdin(Stdio::inherit())
    .stdout(Stdio::inherit())
    .stderr(Stdio::inherit());
    let status = cmd.status().context("ssh")?;
    Ok(status.code().unwrap_or(1))
}

fn netdev() -> String {
    match std::env::var("OATH_BRIDGE") {
        Ok(br) if !br.is_empty() => format!("bridge,id=n0,br={br}"),
        _ => format!("user,id=n0,hostfwd=tcp:127.0.0.1:{}-:22", ssh_port()),
    }
}

pub fn run_interactive(root: &Path, out: &Path) -> Result<i32> {
    let tools = crate::tools::load(root)?;
    let img = load_image(out)?;
    let run = new_run(out, "int")?;
    let overlay = overlay_disk(&tools, &run, &img.backing)?;
    write_meta(&run, &tools, &img, &overlay, "int")?;
    let inject = write_host_authorized(&run.join("host.authorized"));
    let args = qemu_args(
        &tools,
        &img,
        &overlay,
        &run.join("serial.log"),
        &run.join("qemu.log"),
        SerialMode::Stdio,
        inject.as_deref(),
        host_wants_window(),
    );
    fs::write(run.join("qemu.cmd"), args.join(" ") + "\n")?;
    eprintln!("run: {}", run.display());
    eprintln!("serial log: {}", run.join("serial.log").display());
    if host_wants_window() {
        eprintln!("display: gtk window (OATH_DISPLAY=none to hide)");
    }
    if std::env::var("OATH_BRIDGE").map(|s| s.is_empty()).unwrap_or(true) {
        eprintln!("ssh: cargo make ssh   (port {})", ssh_port());
    } else {
        eprintln!(
            "ssh: guest is on bridge {} (DHCP: oath set net:net0 ipv4=dhcp)",
            std::env::var("OATH_BRIDGE").unwrap()
        );
    }
    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..]).stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    let status = cmd.status().context("qemu")?;
    let rc = status.code().unwrap_or(1);
    if let Ok(mut meta) =
        serde_json::from_str::<serde_json::Value>(&fs::read_to_string(run.join("meta.json"))?)
    {
        meta["ended"] = json!(utc_rfc3339());
        meta["qemu_exit"] = json!(rc);
        write_pretty(&run.join("meta.json"), &meta)?;
    }
    eprintln!("qemu exit {rc}  (logs in {})", run.display());
    Ok(rc)
}

fn pid_file(out: &Path) -> PathBuf {
    out.join("vm.pid")
}

fn run_file(out: &Path) -> PathBuf {
    out.join("vm.run")
}

fn pid_alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

fn running_pid(out: &Path) -> Result<Option<i32>> {
    let p = pid_file(out);
    if !p.is_file() {
        return Ok(None);
    }
    let s = fs::read_to_string(&p).unwrap_or_default();
    let Ok(pid) = s.trim().parse::<i32>() else {
        let _ = fs::remove_file(&p);
        return Ok(None);
    };
    if pid_alive(pid) {
        Ok(Some(pid))
    } else {
        let _ = fs::remove_file(&p);
        let _ = fs::remove_file(run_file(out));
        Ok(None)
    }
}

fn print_reachability(run: &Path) {
    eprintln!("run: {}", run.display());
    eprintln!("serial log: {}", run.join("serial.log").display());
    if host_wants_window() {
        eprintln!("display: gtk window (OATH_DISPLAY=none to hide)");
    }
    if std::env::var("OATH_BRIDGE").map(|s| s.is_empty()).unwrap_or(true) {
        eprintln!("ssh: cargo make ssh   (port {})", ssh_port());
    } else {
        eprintln!(
            "ssh: guest is on bridge {} (DHCP: oath set net:net0 ipv4=dhcp)",
            std::env::var("OATH_BRIDGE").unwrap()
        );
    }
}

fn prepare_run(root: &Path, out: &Path, label: &str) -> Result<(Tools, Vec<String>, PathBuf)> {
    let tools = crate::tools::load(root)?;
    let img = load_image(out)?;
    let run = new_run(out, label)?;
    let overlay = overlay_disk(&tools, &run, &img.backing)?;
    write_meta(&run, &tools, &img, &overlay, label)?;
    let inject = write_host_authorized(&run.join("host.authorized"));
    let args = qemu_args(
        &tools,
        &img,
        &overlay,
        &run.join("serial.log"),
        &run.join("qemu.log"),
        SerialMode::File,
        inject.as_deref(),
        host_wants_window(),
    );
    fs::write(run.join("qemu.cmd"), args.join(" ") + "\n")?;
    Ok((tools, args, run))
}

/// Foreground, serial in a file. Ctrl-C kills QEMU.
pub fn run_up(root: &Path, out: &Path) -> Result<i32> {
    if let Some(pid) = running_pid(out)? {
        bail!("already running pid {pid} — cargo make stop");
    }
    let (_tools, args, run) = prepare_run(root, out, "up")?;
    print_reachability(&run);
    eprintln!("Ctrl-C stops the VM");
    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..]).stdin(Stdio::null()).stdout(Stdio::inherit()).stderr(Stdio::inherit());
    let mut child = cmd.spawn().context("qemu")?;
    let pid = child.id() as i32;
    fs::write(pid_file(out), format!("{pid}\n"))?;
    fs::write(run_file(out), format!("{}\n", run.display()))?;
    let status = child.wait().context("qemu")?;
    let _ = fs::remove_file(pid_file(out));
    let _ = fs::remove_file(run_file(out));
    let rc = status.code().unwrap_or(1);
    eprintln!("qemu exit {rc}  (logs in {})", run.display());
    Ok(rc)
}

/// Background QEMU. Serial in the run dir. `cargo make stop` kills it.
pub fn start(root: &Path, out: &Path) -> Result<()> {
    if let Some(pid) = running_pid(out)? {
        bail!("already running pid {pid} — cargo make stop");
    }
    let (_tools, args, run) = prepare_run(root, out, "vm")?;
    let log = fs::File::create(run.join("qemu.out"))?;
    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let child = cmd.spawn().context("qemu")?;
    let pid = child.id() as i32;
    fs::write(pid_file(out), format!("{pid}\n"))?;
    fs::write(run_file(out), format!("{}\n", run.display()))?;
    print_reachability(&run);
    eprintln!("pid {pid}");
    eprintln!("stop: cargo make stop");
    Ok(())
}

pub fn stop(out: &Path) -> Result<()> {
    let Some(pid) = running_pid(out)? else {
        eprintln!("not running");
        return Ok(());
    };
    unsafe {
        libc::kill(pid, libc::SIGTERM);
    }
    for _ in 0..20 {
        if !pid_alive(pid) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    if pid_alive(pid) {
        unsafe {
            libc::kill(pid, libc::SIGKILL);
        }
    }
    let _ = fs::remove_file(pid_file(out));
    let _ = fs::remove_file(run_file(out));
    eprintln!("stopped pid {pid}");
    Ok(())
}
