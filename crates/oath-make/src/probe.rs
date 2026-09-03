use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use regex::Regex;
use serde_json::{json, Value};

use crate::qemu::{self, Image};
use crate::tools::Tools;
use crate::util::{utc_rfc3339, write_pretty};

pub struct SerialVm {
    pub child: Child,
    pub buf: String,
}

impl SerialVm {
    fn spawn(
        tools: &Tools,
        img: &Image,
        overlay: &Path,
        serial: &Path,
        qlog: &Path,
        cmdfile: &Path,
    ) -> Result<Self> {
        let args = qemu::qemu_args(
            tools,
            img,
            overlay,
            serial,
            qlog,
            qemu::SerialMode::Stdio,
            None,
            false,
        );
        fs::write(cmdfile, args.join(" ") + "\n")?;
        let child = qemu::qemu_command(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("qemu")?;
        Ok(Self { child, buf: String::new() })
    }

    fn read_some(&mut self, timeout: Duration) -> Result<()> {
        let Some(out) = self.child.stdout.as_mut() else {
            return Ok(());
        };
        let fd = out.as_raw_fd();
        let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        let n = unsafe { libc::poll(&mut pfd, 1, ms) };
        if n <= 0 {
            return Ok(());
        }
        let mut buf = [0u8; 4096];
        match out.read(&mut buf) {
            Ok(0) => {}
            Ok(n) => self.buf.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    fn wait_for(&mut self, pat: &str, timeout: Duration) -> Result<bool> {
        let re = Regex::new(pat)?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if re.is_match(&self.buf) {
                return Ok(true);
            }
            if self.child.try_wait()?.is_some() {
                return Ok(re.is_match(&self.buf));
            }
            self.read_some(Duration::from_millis(150))?;
        }
        Ok(re.is_match(&self.buf))
    }

    fn send(&mut self, line: &str) -> Result<()> {
        if let Some(stdin) = self.child.stdin.as_mut() {
            stdin.write_all(line.as_bytes())?;
            stdin.write_all(b"\n")?;
            stdin.flush()?;
        }
        Ok(())
    }

    fn wait_exit(&mut self, timeout: Duration) -> Result<Option<i32>> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if let Some(st) = self.child.try_wait()? {
                return Ok(st.code());
            }
            self.read_some(Duration::from_millis(100))?;
        }
        Ok(None)
    }

    fn close(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Step {
    name: String,
    ok: bool,
    detail: String,
}

fn record(steps: &mut Vec<Step>, name: &str, ok: bool, detail: &str) {
    eprintln!(
        "  [{}] {name}{}",
        if ok { "ok" } else { "FAIL" },
        if !ok && !detail.is_empty() {
            format!(" — {}", detail.chars().take(120).collect::<String>())
        } else {
            String::new()
        }
    );
    steps.push(Step { name: name.into(), ok, detail: detail.chars().take(2000).collect() });
}

fn cmd(
    vm: &mut SerialVm,
    steps: &mut Vec<Step>,
    line: &str,
    expect: Option<&str>,
    name: &str,
    timeout: Duration,
) -> Result<String> {
    let before = vm.buf.len();
    vm.send(line)?;
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        vm.read_some(Duration::from_millis(150))?;
        let chunk = &vm.buf[before..];
        let rest = chunk.get(line.len()..).unwrap_or(chunk);
        if rest.contains("~ #") {
            break;
        }
        if vm.child.try_wait()?.is_some() {
            break;
        }
    }
    let chunk = vm.buf[before..].to_string();
    let mut ok = true;
    let mut detail = String::new();
    if let Some(exp) = expect {
        // Skip the echoed command so `echo FOO` does not count as FOO.
        let body = chunk.get(line.len()..).unwrap_or(chunk.as_str());
        if !body.contains(exp) {
            ok = false;
            let tail: String =
                body.chars().rev().take(800).collect::<String>().chars().rev().collect();
            detail = format!("expected {exp:?} in:\n{tail}");
        }
    }
    record(steps, name, ok, &detail);
    Ok(chunk)
}

fn spawn_fetch_server(root: &Path) -> Option<std::thread::JoinHandle<()>> {
    let listener = TcpListener::bind(("0.0.0.0", 18765)).ok()?;
    let _ = listener.set_nonblocking(false);
    let body = fs::read(root.join("apps/fetchme/bin/fetchme")).ok()?;
    Some(std::thread::spawn(move || {
        for mut s in listener.incoming().flatten() {
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf);
            let head = format!(
                "HTTP/1.0 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(head.as_bytes());
            let _ = s.write_all(&body);
        }
    }))
}

fn host_ssh(key: &Path, want_ok: bool) -> (bool, String) {
    host_ssh_as(key, "home", "echo SSH_OK", want_ok)
}

fn host_ssh_as(key: &Path, user: &str, remote: &str, want_ok: bool) -> (bool, String) {
    let port = std::env::var("OATH_SSH_PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(2222u16);
    let tries = if want_ok { 10 } else { 2 };
    let mut last = String::new();
    for _ in 0..tries {
        let o = Command::new("ssh")
            .args([
                "-p",
                &port.to_string(),
                "-i",
                &key.display().to_string(),
                "-o",
                "StrictHostKeyChecking=no",
                "-o",
                "UserKnownHostsFile=/dev/null",
                "-o",
                "GlobalKnownHostsFile=/dev/null",
                "-o",
                "ConnectTimeout=4",
                "-o",
                "BatchMode=yes",
                &format!("{user}@127.0.0.1"),
                remote,
            ])
            .output();
        match o {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let stderr = String::from_utf8_lossy(&o.stderr);
                last = format!(
                    "status={} stdout={:?} stderr={:?}",
                    o.status.code().unwrap_or(-1),
                    stdout.trim(),
                    stderr.trim()
                );
                let ok = o.status.success() && stdout.contains("SSH_OK");
                if ok == want_ok {
                    return (true, last);
                }
            }
            Err(e) => last = e.to_string(),
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    (false, last)
}

fn extract_events(serial: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for line in serial.lines() {
        if let Some(p) = line.split("oath-tel ").nth(1) {
            match serde_json::from_str::<Value>(p.trim()) {
                Ok(v) => out.push(v),
                Err(_) => out.push(json!({ "raw": p.trim() })),
            }
        }
    }
    out
}

pub fn probe(root: &Path, out: &Path) -> Result<i32> {
    let tools = crate::tools::load(root)?;
    let img = qemu::load_image(out)?;
    let run = qemu::new_run(out, "probe")?;
    let overlay = qemu::overlay_disk(&tools, &run, &img.backing)?;
    qemu::write_meta(&run, &tools, &img, &overlay, "probe")?;
    eprintln!("run: {}", run.display());
    // Don't fight a `cargo make up` that already owns 2222.
    if std::env::var_os("OATH_SSH_PORT").is_none() {
        std::env::set_var("OATH_SSH_PORT", "13222");
    }
    let _fetch = spawn_fetch_server(root);

    let key_path = run.join("id_ed25519");
    let _ = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-f", key_path.to_str().unwrap(), "-N", ""])
        .status();
    let pubkey = fs::read_to_string(run.join("id_ed25519.pub")).unwrap_or_default();
    // Drop comment so the serial line stays short; keep type + blob.
    let pubkey = pubkey.split_whitespace().take(2).collect::<Vec<_>>().join(" ");

    let mut steps = Vec::new();

    let boot = |label: &str, steps: &mut Vec<Step>| -> Result<SerialVm> {
        let mut vm = SerialVm::spawn(
            &tools,
            &img,
            &overlay,
            &run.join(format!("serial-{label}.log")),
            &run.join(format!("qemu-{label}.log")),
            &run.join("qemu.cmd"),
        )?;
        let ready = vm.wait_for(r"oath-init: ready|oath-tel .*ready", Duration::from_secs(45))?;
        record(
            steps,
            &format!("{label}.ready"),
            ready,
            if ready { "" } else { "did not see init ready" },
        );
        let prompt = vm.wait_for(r"(~|/) #", Duration::from_secs(15))?;
        record(
            steps,
            &format!("{label}.prompt"),
            prompt,
            if prompt { "" } else { "no serial prompt" },
        );
        Ok(vm)
    };

    let mut vm = boot("boot1", &mut steps)?;
    cmd(&mut vm, &mut steps, "stty cols 512", None, "stty.cols", Duration::from_secs(5))?;
    cmd(&mut vm, &mut steps, "ls /oath/run/fs", Some("@"), "gens.top", Duration::from_secs(8))?;
    cmd(&mut vm, &mut steps, "oath ls", Some("host:local"), "ls.host", Duration::from_secs(8))?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get host:local",
        Some("hostname: \"oath\""),
        "get.initial",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "grep '^home:' /etc/passwd && test -d /home && echo SEAT_HOME",
        Some("SEAT_HOME"),
        "seat.passwd",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -x /lib/oath/init && test ! -e /usr/lib/oath/init && echo LIB_OATH",
        Some("LIB_OATH"),
        "seat.lib_oath",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "grep GROK_DISABLE_AUTOUPDATER /etc/profile && echo ENV_GROK",
        Some("ENV_GROK"),
        "seat.env",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get svc:hold --actual",
        Some("\"state\": \"running\""),
        "hold.running",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "grep ' /tmp ' /proc/mounts",
        Some("tmpfs"),
        "floor.tmp",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -d /dev/shm && echo SHM_OK",
        Some("SHM_OK"),
        "floor.shm",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "grep ' /run ' /proc/mounts",
        Some("tmpfs"),
        "floor.run",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -f /sys/fs/cgroup/cgroup.controllers && echo CGROUP_OK",
        Some("CGROUP_OK"),
        "floor.cgroup",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath ls --kind dev",
        Some("dev:vda"),
        "dev.ls_vda",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath ls --kind dev",
        Some("dev:net0"),
        "dev.ls_net0",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath ls --kind dev",
        Some("dev:ttyS0"),
        "dev.ls_tty",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get dev:vda --actual",
        Some("\"present\": true"),
        "dev.vda",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -c /dev/dri/card0 && echo DRM_OK",
        Some("DRM_OK"),
        "drm.card0",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "stat -c %g /dev/dri/card0 | grep -x 1 && echo DRM_GID",
        Some("DRM_GID"),
        "drm.gid_home",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "stat -c %g /dev/input/event0 | grep -x 1 && echo INPUT_GID",
        Some("INPUT_GID"),
        "dev.input_gid_home",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "stat -c %a /dev/ptmx | grep 666 && echo PTMX_OK",
        Some("PTMX_OK"),
        "dev.ptmx",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get dev:card0 --actual",
        Some("\"present\": true"),
        "dev.card0",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -c /dev/input/event0 && echo EVDEV_OK",
        Some("EVDEV_OK"),
        "dev.evdev",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get dev:kbd0 --actual",
        Some("/dev/input/event"),
        "dev.kbd0",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get dev:mouse0 --actual",
        Some("/dev/input/event"),
        "dev.mouse0",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get pkg:glibc --actual",
        Some("\"present\": true"),
        "pkg.glibc_present",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set pkg:glibc present=false",
        Some("not removable"),
        "pkg.glibc_refuse",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get pkg:river --actual",
        Some("\"present\": true"),
        "pkg.river_present",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "sleep 2; p1=$(pidof river); sleep 2; p2=$(pidof river); test -n \"$p1\" -a \"$p1\" = \"$p2\" && echo RIVER_STABLE",
        Some("RIVER_STABLE"),
        "river.running",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -S /run/seatd.sock -o -S /var/run/seatd.sock && echo SEATD_SOCK",
        Some("SEATD_SOCK"),
        "seatd.sock",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -S /run/user/1/wayland-0 -o -S /run/user/1/wayland-1 && echo WAYLAND_UP",
        Some("WAYLAND_UP"),
        "river.wayland",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "grep -i keyboard /oath/log/river.log && echo LI_KBD",
        Some("LI_KBD"),
        "river.keyboard",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "grep -iE 'mouse|pointer' /oath/log/river.log && echo LI_PTR",
        Some("LI_PTR"),
        "river.pointer",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get pkg:sola --actual",
        Some("\"present\": true"),
        "pkg.sola_present",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "sleep 5; pidof sola-bus >/dev/null && echo SOLA_BUS",
        Some("SOLA_BUS"),
        "sola.bus",
        Duration::from_secs(16),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "pidof sola-call >/dev/null && echo SOLA_CALL",
        Some("SOLA_CALL"),
        "sola.call",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "pidof sola-river >/dev/null && echo SOLA_BRIDGE",
        Some("SOLA_BRIDGE"),
        "sola.bridge",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -S /run/user/1/sola-bus && test -S /run/user/1/sola-call && echo SOLA_SOCK",
        Some("SOLA_SOCK"),
        "sola.sockets",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "sleep 8; pidof sola-shell >/dev/null && echo SOLA_SHELL",
        Some("SOLA_SHELL"),
        "sola.shell",
        Duration::from_secs(24),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "pidof sola-session >/dev/null && echo SOLA_SESSION",
        Some("SOLA_SESSION"),
        "sola.session",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "awk '/^Uid:/{print $2}' /proc/$(pidof sola-session)/status | grep -x 1 && echo SEAT_UID",
        Some("SEAT_UID"),
        "seat.session_uid",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -z \"$(pidof sola)\" && echo SOLA_NO_PM",
        Some("SOLA_NO_PM"),
        "sola.no_pm",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -x /bin/sola-terminal && test -x /bin/tmux && echo TERM_BINS",
        Some("TERM_BINS"),
        "sola.terminal_bins",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "ls /oath/store/pkg/sola/share/fonts | grep -q SF-Pro-Text && echo UI_FONT",
        Some("UI_FONT"),
        "sola.ui_font",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "ls /oath/store/pkg/sola/share/fonts | grep -q IosevkaTermSlab && echo MONO_FONT",
        Some("MONO_FONT"),
        "sola.mono_font",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "tmux -V && echo TMUX_OK",
        Some("TMUX_OK"),
        "sola.tmux",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "tmux -L oathprobe new-session -d -s t /bin/sleep 2 && tmux -L oathprobe ls && echo TMUX_SESS",
        Some("TMUX_SESS"),
        "sola.tmux_session",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "sola-terminal >/dev/null 2>&1 & sleep 8; pidof sola-terminal >/dev/null && echo TERM_UP",
        Some("TERM_UP"),
        "sola.terminal",
        Duration::from_secs(24),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -x /bin/sola-browser && echo BROWSER_BIN",
        Some("BROWSER_BIN"),
        "sola.browser_bin",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -f /oath/store/pkg/sola/cef/Release/libcef.so && echo CEF",
        Some("CEF"),
        "sola.cef",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "sola-browser >/dev/null 2>&1 & sleep 20; pidof sola-browser >/dev/null && echo BROWSER_UP",
        Some("BROWSER_UP"),
        "sola.browser",
        Duration::from_secs(40),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set dev:vda present=false",
        Some("not removable"),
        "dev.refuse",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath ls --kind net",
        Some("net:net0"),
        "net.ls",
        Duration::from_secs(8),
    )?;
    cmd(&mut vm, &mut steps, "ip -o link", Some("net0"), "net.link", Duration::from_secs(8))?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get net:net0 --actual",
        Some("\"up\": true"),
        "net.up",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "ping -c 1 -w 5 10.0.2.2 && echo NET_UP",
        Some("NET_UP"),
        "net.ping",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set net:net0 up=false",
        None,
        "net.set_down",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath apply net:net0",
        Some("applied generation"),
        "net.apply_down",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "ping -c 1 -w 3 10.0.2.2 || echo NET_DOWN",
        Some("NET_DOWN"),
        "net.down",
        Duration::from_secs(10),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath undo",
        Some("undid to generation"),
        "net.undo",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "ping -c 1 -w 5 10.0.2.2 && echo NET_UP",
        Some("NET_UP"),
        "net.undo_ping",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set pkg:fetchme present=true",
        None,
        "pkg.fetch_set",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath apply pkg:fetchme",
        Some("applied generation"),
        "pkg.fetch_apply",
        Duration::from_secs(20),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "[ \"$(fetchme)\" = fetched ] && echo PKG_FETCH_OK",
        Some("PKG_FETCH_OK"),
        "pkg.fetch_run",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "grep :0016 /proc/net/tcp && echo SSHD_LISTEN",
        Some("SSHD_LISTEN"),
        "ssh.listen",
        Duration::from_secs(8),
    )?;
    let set_json = serde_json::json!({ "authorized": [pubkey] }).to_string();
    cmd(
        &mut vm,
        &mut steps,
        &format!("oath set ssh:local --from-json '{set_json}'"),
        None,
        "ssh.set_key",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath apply ssh:local",
        Some("applied generation"),
        "ssh.apply_key",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "cat /home/.ssh/authorized_keys",
        Some("ssh-ed25519"),
        "ssh.keys_file",
        Duration::from_secs(8),
    )?;
    {
        let (ok, detail) = host_ssh(&key_path, true);
        record(&mut steps, "ssh.login", ok, &detail);
    }
    {
        let (ok, detail) = host_ssh_as(&key_path, "root", "echo SSH_OK", false);
        record(&mut steps, "ssh.root_denied", ok, &detail);
    }
    {
        let (ok, detail) = host_ssh_as(&key_path, "home", "sudo id -u && echo SSH_OK", true);
        record(&mut steps, "ssh.sudo", ok, &detail);
    }
    cmd(
        &mut vm,
        &mut steps,
        "oath set ssh:local --from-json '{\"authorized\":[]}'",
        None,
        "ssh.set_empty",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath apply ssh:local",
        Some("applied generation"),
        "ssh.apply_empty",
        Duration::from_secs(12),
    )?;
    {
        let (ok, detail) = host_ssh(&key_path, false);
        record(&mut steps, "ssh.denied", ok, &detail);
    }
    cmd(
        &mut vm,
        &mut steps,
        "oath undo",
        Some("undid to generation"),
        "ssh.undo",
        Duration::from_secs(12),
    )?;
    {
        let (ok, detail) = host_ssh(&key_path, true);
        record(&mut steps, "ssh.undo_login", ok, &detail);
    }
    cmd(
        &mut vm,
        &mut steps,
        "oath ls --kind pkg",
        Some("pkg:hello"),
        "pkg.ls",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath ls --kind pkg",
        Some("pkg:busybox"),
        "pkg.ls_busybox",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "readlink /bin/oath",
        Some("/oath/store/pkg/oath/bin/oath"),
        "pkg.oath_symlink",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "readlink /bin/busybox",
        Some("/oath/store/pkg/busybox/bin/busybox"),
        "pkg.busybox_symlink",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "readlink /bin/btrfs",
        Some("/oath/store/pkg/btrfs/bin/btrfs"),
        "pkg.btrfs_symlink",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get pkg:busybox --actual",
        Some("\"removable\": false"),
        "pkg.busybox_sealed",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set pkg:busybox present=false",
        Some("not removable"),
        "pkg.busybox_refuse",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set pkg:oath present=false",
        Some("not removable"),
        "pkg.oath_refuse",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set pkg:btrfs present=false",
        Some("not removable"),
        "pkg.btrfs_refuse",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get pkg:hello --actual",
        Some("\"present\": false"),
        "pkg.absent",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "test -x /oath/store/pkg/hello/bin/hello && echo PKG_STORE_OK",
        Some("PKG_STORE_OK"),
        "pkg.store",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set pkg:hello present=true",
        None,
        "pkg.set_present",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath apply pkg:hello",
        Some("applied generation"),
        "pkg.apply_present",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "readlink /bin/hello",
        Some("/oath/store/pkg/hello/bin/hello"),
        "pkg.symlink",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "[ \"$(hello)\" = hello ] && echo PKG_HELLO_OK",
        Some("PKG_HELLO_OK"),
        "pkg.run",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set host:local hostname=atlas",
        None,
        "set.atlas",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath apply",
        Some("applied generation"),
        "apply.atlas",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "ls /oath/run/fs",
        Some("@gen-"),
        "gens.sibling",
        Duration::from_secs(8),
    )?;
    cmd(&mut vm, &mut steps, "hostname", Some("atlas"), "hostname.atlas", Duration::from_secs(8))?;
    cmd(
        &mut vm,
        &mut steps,
        "oath undo",
        Some("undid to generation"),
        "undo",
        Duration::from_secs(12),
    )?;
    cmd(&mut vm, &mut steps, "hostname", Some("oath"), "hostname.undone", Duration::from_secs(8))?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set svc:hold enabled=false",
        None,
        "hold.disable",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath apply svc:hold",
        Some("applied generation"),
        "hold.apply_stop",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get svc:hold --actual",
        Some("\"state\": \"stopped\""),
        "hold.stopped",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath undo",
        Some("undid to generation"),
        "hold.undo",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath get svc:hold --actual",
        Some("\"state\": \"running\""),
        "hold.undone",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set svc:hold enabled=false",
        None,
        "hold.disable2",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath apply svc:hold",
        Some("applied generation"),
        "hold.apply_stop2",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set host:local hostname=atlas",
        None,
        "set.atlas2",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath apply",
        Some("applied generation"),
        "apply.atlas2",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath set host:local power=reboot",
        None,
        "set.reboot",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm,
        &mut steps,
        "oath apply",
        Some("confirm required"),
        "apply.reboot.noconfirm",
        Duration::from_secs(8),
    )?;
    let before = vm.buf.len();
    vm.send("oath apply --confirm")?;
    let rc = vm.wait_exit(Duration::from_secs(20))?;
    let _ = vm.read_some(Duration::from_millis(200));
    let tail: String =
        vm.buf[before..].chars().rev().take(400).collect::<String>().chars().rev().collect();
    record(
        &mut steps,
        "apply.reboot.confirm_exits",
        rc.is_some(),
        &format!("qemu still running rc={rc:?} tail={tail}"),
    );
    if rc.is_none() {
        vm.close();
    }

    let mut vm2 = boot("boot2", &mut steps)?;
    cmd(
        &mut vm2,
        &mut steps,
        "hostname",
        Some("atlas"),
        "reboot.hostname",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "oath get host:local",
        Some("hostname: \"atlas\""),
        "reboot.get",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "oath get svc:hold --actual",
        Some("\"state\": \"stopped\""),
        "reboot.hold_stopped",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "ping -c 1 -w 5 10.0.2.2 && echo NET_UP",
        Some("NET_UP"),
        "reboot.net_ping",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "oath ls --kind dev",
        Some("dev:vda"),
        "reboot.dev_vda",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "oath get dev:kbd0 --actual",
        Some("/dev/input/event"),
        "reboot.kbd0",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "test -f /sys/fs/cgroup/cgroup.controllers && echo CGROUP_OK",
        Some("CGROUP_OK"),
        "reboot.cgroup",
        Duration::from_secs(8),
    )?;
    {
        let (ok, detail) = host_ssh(&key_path, true);
        record(&mut steps, "reboot.ssh_login", ok, &detail);
    }
    cmd(
        &mut vm2,
        &mut steps,
        "[ \"$(hello)\" = hello ] && echo PKG_HELLO_OK",
        Some("PKG_HELLO_OK"),
        "reboot.pkg_hello",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "oath get pkg:hello --actual",
        Some("\"present\": true"),
        "reboot.pkg_present",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "oath set pkg:hello present=false",
        None,
        "pkg.set_absent",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "oath apply pkg:hello",
        Some("applied generation"),
        "pkg.apply_absent",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "test ! -e /bin/hello && echo PKG_HELLO_GONE",
        Some("PKG_HELLO_GONE"),
        "pkg.unlinked",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "test -x /oath/store/pkg/hello/bin/hello && echo PKG_STORE_OK",
        Some("PKG_STORE_OK"),
        "pkg.store_kept",
        Duration::from_secs(8),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "oath undo",
        Some("undid to generation"),
        "pkg.undo",
        Duration::from_secs(12),
    )?;
    cmd(
        &mut vm2,
        &mut steps,
        "[ \"$(hello)\" = hello ] && echo PKG_HELLO_OK",
        Some("PKG_HELLO_OK"),
        "pkg.undo_hello",
        Duration::from_secs(8),
    )?;
    vm2.close();

    let mut serial = String::new();
    let mut logs: Vec<PathBuf> = fs::read_dir(&run)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("serial-") && n.ends_with(".log"))
        })
        .collect();
    logs.sort();
    for p in &logs {
        serial.push_str(&fs::read_to_string(p).unwrap_or_default());
        serial.push('\n');
    }
    fs::write(run.join("serial.log"), &serial)?;
    let events = extract_events(&serial);
    let mut evl = String::new();
    for e in &events {
        evl.push_str(&serde_json::to_string(e)?);
        evl.push('\n');
    }
    fs::write(run.join("events.jsonl"), evl)?;

    let failed: Vec<&str> = steps.iter().filter(|s| !s.ok).map(|s| s.name.as_str()).collect();
    let probe = json!({
        "run": run.display().to_string(),
        "ok": failed.is_empty(),
        "failed": failed,
        "steps": steps.iter().map(|s| json!({"name": s.name, "ok": s.ok, "detail": s.detail})).collect::<Vec<_>>(),
        "events": events.len(),
        "ended": utc_rfc3339(),
    });
    write_pretty(&run.join("probe.json"), &probe)?;

    let mut md = format!(
        "# Probe {}\n\n**ok:** {}\n**failed:** {}\n**events:** {} `oath-tel` lines\n\n| step | ok |\n|------|----|\n",
        run.file_name().unwrap().to_string_lossy(),
        failed.is_empty(),
        if failed.is_empty() { "none".into() } else { failed.join(", ") },
        events.len()
    );
    for s in &steps {
        md.push_str(&format!("| `{}` | {} |\n", s.name, if s.ok { "yes" } else { "NO" }));
    }
    if !failed.is_empty() {
        md.push_str("\n## Failures\n");
        for s in steps.iter().filter(|s| !s.ok) {
            md.push_str(&format!("### {}\n```\n{}\n```\n", s.name, s.detail));
        }
    }
    fs::write(run.join("REPORT.md"), md)?;
    eprintln!("report: {}", run.join("REPORT.md").display());
    println!(
        "{}",
        serde_json::to_string(
            &json!({"ok": failed.is_empty(), "failed": failed, "run": run.display().to_string()})
        )?
    );
    Ok(if failed.is_empty() { 0 } else { 1 })
}
