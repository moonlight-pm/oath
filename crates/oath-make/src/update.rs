//! `cargo make update --qemu <older>`: rehearse a machine's own update
//! (R38: machines pull, `oath update`) in QEMU.
//!
//! 1. `install --qemu` the older output and boot it through OVMF.
//! 2. Make the newer output's release, signed with this host's key, and
//!    serve it, with every pack the older output lacks, from a local
//!    origin (`http://10.0.2.2:<port>` from inside QEMU).
//! 3. Point the machine at that origin and trust this host's key, as a
//!    person does on a real machine, and record the older output's release
//!    as the one it runs, as a pull of it would (the one this host
//!    published, else one made here). One older than `oath
//!    update` first takes the newer `pkg:oath` from the origin
//!    (docs/manual/install.md).
//!    `--build plan:<name>` first builds that recipe on the machine and
//!    makes its package present, as its owner does (canto's Steam).
//! 4. `sudo oath update --confirm --reboot`, and require that the update
//!    rebooted, with no service restarted on its new program before (when
//!    the output's `oath` ran it), and then the output's PID 1 with the
//!    catalog in sync, each pack pinned from elsewhere at the newer
//!    output's pin, each recipe built at the output's pin, every service
//!    that ran before running again, and the old boot archived on the ESP.
//! 5. A machine from before R36 then moves `svc:sshd` to OpenSSH with a
//!    second `oath update --confirm`, kept from a new SSH session.
//!
//! Real machines are never written from here: each runs `sudo oath
//! update` itself.

use std::collections::BTreeSet;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use oath_core::update as up;
use serde_json::{json, Value};

use crate::install::{self, Remote};
use crate::output::{self, Output};
use crate::tools::{self, Tools};
use crate::util::{copy_file, kvm, sha256_file};

pub struct Opts {
    /// Rehearse from this (older) output.
    pub qemu: String,
    pub output: Option<String>,
    pub unchecked: bool,
    /// Move `pkg:oath` first, so the output's own `oath` plans the update.
    pub oath_first: bool,
    /// Recipes (`plan:<name>`) to build on the older machine first.
    pub build: Vec<String>,
}

pub fn run_update(root: &Path, out: &Path, opts: Opts) -> Result<()> {
    let o = output::pick(out, opts.output.as_deref(), !opts.unchecked)
        .context("pass --unchecked to rehearse the latest output anyway")?;
    if !opts.unchecked {
        crate::check::require_checked(&o.dir).context("pass --unchecked to rehearse it anyway")?;
    }
    eprintln!("output {}", o.dir.display());
    rehearse(root, out, &o, &opts)
}

/// A QEMU machine booted from an installed disk through OVMF.
struct Vm {
    tools: Tools,
    dir: PathBuf,
    port: u16,
    boots: u32,
    child: Option<std::process::Child>,
}

impl Vm {
    fn boot(&mut self) -> Result<()> {
        self.boots += 1;
        let code = self.tools.ovmf_code.as_ref().context("OVMF_CODE.fd missing")?;
        let serial = self.dir.join(format!("serial-update-{}.log", self.boots));
        let netdev = format!("user,id=n0,hostfwd=tcp:127.0.0.1:{}-:22", self.port);
        let drive =
            format!("file={},if=none,id=hd0,format=qcow2", self.dir.join("disk.qcow2").display());
        let mut c = Command::new(&self.tools.qemu);
        if kvm() {
            c.arg("-enable-kvm");
        }
        c.args([
            "-drive",
            &format!("if=pflash,format=raw,readonly=on,file={}", code.display()),
            "-drive",
            &format!("if=pflash,format=raw,file={}", self.dir.join("OVMF_VARS.fd").display()),
        ]);
        install::qemu_headless(&mut c, &serial, &netdev, &drive);
        c.stdout(Stdio::null()).stderr(Stdio::null());
        eprintln!(">> OVMF boot {} ({})", self.boots, serial.display());
        self.child = Some(c.spawn().context("qemu")?);
        Ok(())
    }

    /// `-no-reboot`: a reboot ends the QEMU process.
    fn wait_exit(&mut self, wait: Duration) -> Result<()> {
        let Some(child) = self.child.as_mut() else { return Ok(()) };
        let deadline = Instant::now() + wait;
        while Instant::now() < deadline {
            if child.try_wait()?.is_some() {
                self.child = None;
                return Ok(());
            }
            thread::sleep(Duration::from_millis(500));
        }
        bail!("QEMU did not exit on reboot")
    }
}

impl Drop for Vm {
    fn drop(&mut self) {
        if let Some(c) = self.child.as_mut() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn rehearse(root: &Path, out: &Path, o: &Output, opts: &Opts) -> Result<()> {
    let (from, oath_first) = (opts.qemu.as_str(), opts.oath_first);
    let old = output::open(out, from)?;
    if old.id == o.id {
        bail!("--qemu {from} is the output being rehearsed; name an older one");
    }
    let dir = install::run_install(
        root,
        out,
        install::Opts {
            target: None,
            disk: "/dev/vda".into(),
            confirm: true,
            qemu: true,
            usb: false,
            local: false,
            hostname: None,
            overlay: None,
            unchecked: true,
            output: Some(old.id.clone()),
        },
    )?;

    // 2. The local origin. The older output's release first, so the newer
    // one is not older than the release the machine records.
    let origin = dir.join("origin");
    let published = out.join("release").join(&old.id).join("release.json");
    let ran = if published.is_file() {
        published
    } else {
        crate::release::make(&old, &dir.join("release-old"))?.release
    };
    let record = fs::read_to_string(&ran)?;
    let ran: up::Release = serde_json::from_str(&record)?;
    let made = crate::release::make(o, &dir.join("release"))?;
    for (key, file) in made.files() {
        let to = origin.join(&key);
        fs::create_dir_all(to.parent().context("parent")?)?;
        fs::copy(&file, &to)?;
    }
    let mut packs = 0;
    for key in crate::release::pack_keys(o)? {
        let (name, hash) = key
            .strip_prefix("pkg/")
            .and_then(|k| k.strip_suffix(".tar"))
            .and_then(|k| k.split_once('/'))
            .context("pack key")?;
        if old.file("stage/oath/store/pkg").join(name).join(hash).is_dir() {
            continue;
        }
        let to = origin.join(&key);
        fs::create_dir_all(to.parent().context("parent")?)?;
        crate::util::run(
            Command::new("tar")
                .arg("-C")
                .arg(o.file("stage/oath/store/pkg").join(name).join(hash))
                .arg("-cf")
                .arg(&to)
                .arg("."),
        )?;
        packs += 1;
    }
    let port = serve(&origin)?;
    eprintln!(
        ">> origin {} on 127.0.0.1:{port} ({packs} packs the older output lacks)",
        origin.display()
    );

    let mut vm = Vm {
        tools: tools::load(root)?,
        dir: dir.clone(),
        port: crate::qemu::ssh_port(),
        boots: 0,
        child: None,
    };
    vm.boot()?;
    let r = install::wait_oath_ssh(
        "127.0.0.1",
        vm.port,
        &dir.join("known_hosts"),
        Duration::from_secs(180),
    )?;
    let r = Remote { sudo: true, ..r };

    // 3. As a person sets up a machine to pull.
    let set = |id: &str, v: Value| -> Result<()> {
        let q = install::sh_quote(&v.to_string());
        install::ssh_run(&r, &format!("set -e\noath set {id} --from-json {q}\noath apply {id}\n"))
    };
    set("host:local", json!({ "origins": [format!("http://10.0.2.2:{port}")] }))?;
    let no_update = install::ssh_out(&r, "oath update --help >/dev/null 2>&1 && echo yes; true")?
        != "yes";
    if no_update || oath_first {
        let doc: Value = oath_core::read_json(&o.file("stage/oath/objects/pkg/oath/desired.json"))
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let why = if no_update { "this machine's oath has no update" } else { "--oath-first" };
        eprintln!(">> pkg:oath first: {why}");
        let needs = doc.get("needs").cloned().unwrap_or_else(|| json!([]));
        set("pkg:oath", json!({ "hash": doc["hash"], "needs": needs }))?;
    }
    let public = crate::release::public_key()?;
    set("host:local", json!({ "profile": made.profile, "publishers": [public] }))?;
    install::ssh_run(
        &r,
        &format!("printf '%s' {} > /oath/{}\n", install::sh_quote(&record), up::RECORD),
    )?;
    let mut recipes = Vec::new();
    for plan in &opts.build {
        let name = plan.strip_prefix("plan:").unwrap_or(plan);
        eprintln!(">> build plan:{name} on the older machine, as its owner does");
        let built = install::ssh_out(
            &r,
            &format!(
                "set -e\noath --json build plan:{name}\n\
                 oath set pkg:{name} present=true >&2\noath apply pkg:{name} >&2\n"
            ),
        )?;
        let v: Value = serde_json::from_str(&built).context("oath --json build")?;
        eprintln!("pkg:{name} {} present", v["product"].as_str().unwrap_or("?"));
        recipes.push(name.to_string());
    }

    // 4. The update, as the manual has it: --reboot.
    let ran_before = steady_services(&r)?;
    let followed = follows(&r)?;
    eprintln!("{}", install::ssh_out(&r, "oath update 2>&1")?);
    let boot = install::ssh_out(&r, "cat /proc/sys/kernel/random/boot_id")?;
    let mut moving = Command::new("ssh");
    moving.args(["-o", "ServerAliveInterval=2", "-o", "ServerAliveCountMax=3"]);
    moving.args(install::ssh_base(&r).get_args());
    moving.arg("sudo -n oath update --confirm --reboot 2>&1");
    let moved = moving.output().context("ssh oath update")?;
    let moved = String::from_utf8_lossy(&moved.stdout).into_owned();
    eprintln!("{moved}");
    if !moved.contains("; rebooting") {
        bail!("oath update --confirm --reboot did not reboot (above)");
    }
    come_back_and_verify(&r, o, &mut vm, &boot)?;
    let after = follows(&r)?;
    if no_update || oath_first {
        if after != followed {
            bail!("services restarted on their new programs before the reboot (oath logs)");
        }
        eprintln!("no service restarted before the reboot");
    }
    pinned_moved(&r, &old, o, &ran)?;
    for name in &recipes {
        let doc = o.file(&format!("stage/oath/objects/pkg/{name}/desired.json"));
        let want: Value = oath_core::read_json(&doc).map_err(|e| anyhow::anyhow!("{e}"))?;
        let got = install::ssh_out(&r, &format!("oath --json get pkg:{name} --actual"))?;
        let got: Value = serde_json::from_str(&got).unwrap_or(Value::Null);
        if got["hash"] != want["hash"] {
            bail!("pkg:{name} is {}, the output pins {}", got["hash"], want["hash"]);
        }
        eprintln!("pkg:{name} was built at the output's pin {}", want["hash"]);
    }
    services_back(&r, &ran_before)?;

    // 5. dropbear to OpenSSH, kept from a new session.
    let sshd = install::ssh_out(&r, "oath --json get svc:sshd --desired")?;
    if sshd.contains("/bin/dropbear") {
        let mut moving = Command::new("ssh");
        moving.args(install::ssh_base(&r).get_args());
        moving.arg("sudo -n oath update --confirm");
        let mut child = moving.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            if Instant::now() > deadline {
                let _ = child.kill();
                bail!("the sshd move never went on probation");
            }
            thread::sleep(Duration::from_secs(3));
            if install::ssh_out(&r, "test -f /oath/probation.json && echo open; true")
                .is_ok_and(|s| s == "open")
            {
                break;
            }
        }
        eprintln!("{}", install::ssh_out(&r, "oath keep")?);
        let done = child.wait_with_output()?;
        eprintln!("{}", String::from_utf8_lossy(&done.stderr));
        if !done.status.success() {
            bail!("oath update (the sshd move) failed: {}", done.status);
        }
        let sshd = install::ssh_out(&r, "oath --json get svc:sshd --desired")?;
        if sshd.contains("/bin/dropbear") {
            bail!("svc:sshd still runs dropbear");
        }
        eprintln!("svc:sshd runs OpenSSH, kept");
    }
    let mnt = "/oath/run/update-esp";
    let entries = install::ssh_out(
        &r,
        &format!(
            "mkdir -p {mnt} && mount -t vfat /dev/vda1 {mnt} && cat {mnt}/loader/oath-boots; umount {mnt}; true"
        ),
    )?;
    eprintln!("boot menu after the update:\n{entries}");
    if !entries.contains("oath-") {
        bail!("the ESP has no archived boot after the update");
    }
    eprintln!("{}", install::ssh_out(&r, "oath update 2>&1")?);
    eprintln!("QEMU update rehearsal ok: {} -> {}  {}", old.id, o.id, dir.display());
    copy_file(&dir.join("known_hosts"), &dir.join("known_hosts.kept")).ok();
    Ok(())
}

/// The services running now and ten seconds from now: not one that comes
/// and goes (in QEMU, a desk program with no GPU).
fn steady_services(r: &Remote) -> Result<BTreeSet<String>> {
    let now = running_services(r)?;
    thread::sleep(Duration::from_secs(10));
    let later = running_services(r)?;
    Ok(now.intersection(&later).cloned().collect())
}

fn running_services(r: &Remote) -> Result<BTreeSet<String>> {
    let names = install::ssh_out(
        r,
        "for f in /oath/status/svc/*.json; do n=${f##*/}; \
         grep -q '\"state\": *\"running\"' \"$f\" && echo \"svc:${n%.json}\"; done; true",
    )?;
    Ok(names.lines().map(str::to_string).collect())
}

/// Every service that ran steadily before the update runs after it (each
/// seen running within two minutes of the boot).
fn services_back(r: &Remote, before: &BTreeSet<String>) -> Result<()> {
    let mut missing = before.clone();
    let deadline = Instant::now() + Duration::from_secs(120);
    while !missing.is_empty() && Instant::now() < deadline {
        let now = running_services(r)?;
        missing.retain(|s| !now.contains(s));
        if !missing.is_empty() {
            thread::sleep(Duration::from_secs(3));
        }
    }
    if !missing.is_empty() {
        let list: Vec<&str> = missing.iter().map(String::as_str).collect();
        bail!("services that ran before the update do not run after it: {}", list.join(" "));
    }
    eprintln!("every service that ran before the update runs again ({})", before.len());
    Ok(())
}

/// Each pack the newer output pins from elsewhere (R28) that the machine had
/// at the pin its release lists is at the newer output's: the owner never
/// moved it, so it moves with the output. A release from before it listed
/// them leaves them where they were.
fn pinned_moved(r: &Remote, old: &Output, o: &Output, ran: &up::Release) -> Result<()> {
    let manifest = |o: &Output| -> Result<Value> {
        Ok(serde_json::from_slice(&fs::read(o.file("manifest.json"))?)?)
    };
    let (was, now) = (manifest(old)?, manifest(o)?);
    let mut off = Vec::new();
    for name in now["pinned"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        let (from, to) = (&was["packs"][name]["hash"], &now["packs"][name]["hash"]);
        if from.is_null() || from == to {
            continue;
        }
        let doc = install::ssh_out(r, &format!("oath --json get pkg:{name} --desired"))?;
        let got: Value = serde_json::from_str(&doc).unwrap_or(Value::Null);
        let got = got.get("hash").unwrap_or(&Value::Null);
        if got == to {
            eprintln!("pkg:{name} moved with the output: {from} -> {to}");
        } else if ran.pinned.get(name).is_some_and(|p| from == p) {
            off.push(format!("pkg:{name} is {got}, the output pins {to}"));
        } else {
            eprintln!("pkg:{name} keeps {got}: the release it ran lists no pin for it");
        }
    }
    if !off.is_empty() {
        bail!("packs pinned from elsewhere did not move with the output:\n{}", off.join("\n"));
    }
    Ok(())
}

/// How many times PID 1 has restarted a service on its package's new
/// program, by its logs.
fn follows(r: &Remote) -> Result<usize> {
    let n = install::ssh_out(
        r,
        "cat /oath/log/init.log.1 /oath/log/init.log 2>/dev/null \
         | grep -c 'is now pkg:.*; restarting'; true",
    )?;
    Ok(n.lines().next().unwrap_or("0").trim().parse().unwrap_or(0))
}

/// After a reboot from boot `before` (its boot id), wait for another boot,
/// and require the output's PID 1 and nothing pending.
fn come_back_and_verify(r: &Remote, o: &Output, vm: &mut Vm, before: &str) -> Result<()> {
    vm.wait_exit(Duration::from_secs(180))?;
    vm.boot()?;
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if Instant::now() > deadline {
            bail!("the machine did not come back from the reboot");
        }
        thread::sleep(Duration::from_secs(3));
        match install::ssh_out(r, "cat /proc/sys/kernel/random/boot_id") {
            Ok(id) if !id.is_empty() && id != before => break,
            _ => {}
        }
    }
    let want = sha256_file(&o.file("stage/lib/oath/init"))?;
    let got = install::ssh_out(r, "sha256sum /proc/1/exe")?;
    if !got.starts_with(&want) {
        bail!("PID 1 is not the output's ({got}, want {want})");
    }
    let diff = install::ssh_out(r, "oath diff")?;
    if diff.trim() != "in-sync" {
        bail!("after the update the catalog is not in sync:\n{diff}");
    }
    let off = unconverged(r)?;
    if !off.is_empty() {
        bail!("after the update packages are not at their pins:\n{}", off.join("\n"));
    }
    eprintln!(
        "the machine runs output {}: PID 1 {}, catalog in sync, every package at its pin",
        o.id,
        &want[..12]
    );
    Ok(())
}

/// Packages whose recorded `actual` hash is not their pin. `oath diff`
/// compares the spec with the one last applied, so it cannot see a pack
/// that was pinned but never linked (canto's `pkg:oath`, 2026-10-28).
fn unconverged(r: &Remote) -> Result<Vec<String>> {
    let rows = install::ssh_out(
        r,
        "cd /oath && for f in objects/pkg/*/desired.json; do n=${f#objects/pkg/}; \
         n=${n%/desired.json}; printf '%s\\t' \"$n\"; tr -d '\\n' < \"$f\"; printf '\\t'; \
         tr -d '\\n' < \"status/pkg/$n.json\" 2>/dev/null; echo; done",
    )?;
    let mut off = Vec::new();
    for row in rows.lines() {
        let mut cols = row.splitn(3, '\t');
        let (Some(name), Some(desired), actual) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        let desired: serde_json::Value = serde_json::from_str(desired)?;
        let actual: serde_json::Value =
            serde_json::from_str(actual.unwrap_or("")).unwrap_or(serde_json::Value::Null);
        let pin = desired["hash"].as_str().unwrap_or("");
        if desired["present"] == false || pin.is_empty() {
            continue;
        }
        let got = actual["hash"].as_str().unwrap_or("none");
        if got != pin {
            off.push(format!("pkg:{name} is {got}, pinned {pin}"));
        }
    }
    Ok(off)
}

/// Serve `dir` read-only over HTTP on a free local port, for the life of
/// this process: GET of a file, else 404.
fn serve(dir: &Path) -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let dir = dir.to_path_buf();
    thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let dir = dir.clone();
            thread::spawn(move || {
                let _ = answer(conn, &dir);
            });
        }
    });
    Ok(port)
}

fn answer(mut conn: TcpStream, dir: &Path) -> std::io::Result<()> {
    let mut reader = BufReader::new(conn.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
    }
    let path = line.split_whitespace().nth(1).unwrap_or("/");
    let path = path.split('?').next().unwrap_or("").trim_start_matches('/');
    let file = (!path.split('/').any(|p| p == ".." || p.is_empty()))
        .then(|| dir.join(path))
        .filter(|f| f.is_file());
    match file {
        Some(f) => {
            let len = fs::metadata(&f)?.len();
            write!(conn, "HTTP/1.1 200 OK\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n")?;
            std::io::copy(&mut fs::File::open(f)?, &mut conn)?;
        }
        None => write!(
            conn,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )?,
    }
    conn.flush()
}
