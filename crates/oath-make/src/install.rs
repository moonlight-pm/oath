//! SSH+kexec install (nixos-anywhere shape). Wipes `--disk` on `--target`.

use std::fs;
use std::io;
use std::io::Write;
use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::pack;
use crate::qemu;
use crate::tools::{self, Tools};
use crate::util::{copy_file, kvm, run, sudo, utc_stamp};

pub struct Opts {
    pub target: Option<String>,
    pub disk: String,
    pub confirm: bool,
    pub qemu: bool,
    pub usb: bool,
    pub hostname: Option<String>,
    /// Catalog overlay merged into the installed catalog (`oath seed --overlay`).
    pub overlay: Option<PathBuf>,
    /// Install an image that has not passed `cargo make check`.
    pub unchecked: bool,
}

pub struct EspOpts {
    pub esp: String,
    pub confirm: bool,
    pub root_dev: String,
}

#[derive(Clone)]
pub struct Remote {
    pub user: String,
    pub host: String,
    pub port: u16,
    sudo: bool,
    /// Pinned host keys. `None` takes any key: the installer ramdisk and
    /// the system being replaced have nothing worth pinning.
    known_hosts: Option<PathBuf>,
}

/// Host keys of the machines this tree installed, written by `install`
/// and read by `check --target`.
pub fn known_hosts(out: &Path) -> PathBuf {
    out.join("known_hosts")
}

pub fn run_install(root: &Path, out: &Path, opts: Opts) -> Result<()> {
    if !opts.confirm {
        bail!("refusing to wipe a disk without --confirm");
    }
    if opts.disk.is_empty() || !opts.disk.starts_with("/dev/") {
        bail!("--disk must be a /dev/ node (got {:?})", opts.disk);
    }
    let tools = tools::load(root)?;
    if !out.join("stage/oath/INDEX.md").is_file() {
        pack::build(root, out, &tools, oath_core::DEFAULT_PROFILE)?;
    }
    if !out.join("initramfs-install/init").is_file() {
        pack::build(root, out, &tools, oath_core::DEFAULT_PROFILE)?;
    }
    if !opts.qemu {
        if opts.unchecked {
            eprintln!("warning: installing an image that `cargo make check` has not passed");
        } else {
            crate::check::require_checked(out)
                .context("pass --unchecked to install anyway (Joshua's call)")?;
        }
    }
    let keys =
        qemu::host_pubkeys_body().context("need host SSH public keys to log into the installer")?;
    pack::bake_install_keys(out, &keys)?;
    if let Some(p) = &opts.overlay {
        read_overlay(p)?;
    }
    if opts.usb {
        return write_usb_installer(root, out, &tools, &opts);
    }
    if opts.qemu {
        return install_qemu(root, out, &tools, &opts);
    }
    let target = opts.target.as_deref().context("--target user@host (or --qemu or --usb)")?;
    let remote = parse_target(target)?;
    install_remote(out, &tools, &opts, &remote)
}

pub fn parse_target(s: &str) -> Result<Remote> {
    let (userhost, port) = match s.rsplit_once(':') {
        Some((uh, p)) if p.chars().all(|c| c.is_ascii_digit()) => {
            (uh, p.parse::<u16>().unwrap_or(22))
        }
        _ => (s, 22u16),
    };
    let (user, host) = match userhost.split_once('@') {
        Some((u, h)) => (u.to_string(), h.to_string()),
        None => ("root".into(), userhost.to_string()),
    };
    Ok(Remote { sudo: user != "root", user, host, port, known_hosts: None })
}

fn parts(disk: &str) -> (String, String) {
    let p1 = if disk.chars().last().is_some_and(|c| c.is_ascii_digit()) {
        format!("{disk}p1")
    } else {
        format!("{disk}1")
    };
    let p2 = if disk.chars().last().is_some_and(|c| c.is_ascii_digit()) {
        format!("{disk}p2")
    } else {
        format!("{disk}2")
    };
    (p1, p2)
}

fn write_usb_installer(root: &Path, out: &Path, tools: &Tools, opts: &Opts) -> Result<()> {
    assert_usb_stick(&opts.disk)?;
    let boot = crate::pack::build_efi(root)?;
    let mkfat = tools.bin("dosfstools", "mkfs.fat")?;
    let sfdisk = tools.bin("util-linux", "sfdisk")?;
    let (p1, _) = parts(&opts.disk);
    eprintln!("USB installer -> {} (wipe GPT, ESP, installer ramdisk)", opts.disk);
    let _ = sudo(&["umount", &p1]);
    let _ = sudo(&["umount", &opts.disk]);
    let mut part = Command::new("sudo");
    part.arg(&sfdisk).args(["--wipe", "always", "--quiet", &opts.disk]).stdin(Stdio::piped());
    let mut child = part.spawn().context("sudo sfdisk")?;
    child
        .stdin
        .take()
        .context("sfdisk stdin")?
        .write_all(b"label: gpt\nsize=1GiB, type=uefi, name=OATHUSB\n")?;
    if !child.wait()?.success() {
        bail!("sfdisk {} failed", opts.disk);
    }
    let _ = sudo(&["blockdev", "--rereadpt", &opts.disk]);
    let _ = sudo(&["partprobe", &opts.disk]);
    for _ in 0..20 {
        if Path::new(&p1).exists() {
            break;
        }
        thread::sleep(Duration::from_millis(200));
    }
    if !Path::new(&p1).exists() {
        bail!("partition {p1} did not appear");
    }
    sudo(&[mkfat.to_str().unwrap(), "-F", "32", "-n", "OATHUSB", &p1])?;
    let mnt = out.join("usb-esp");
    let _ = fs::remove_dir_all(&mnt);
    fs::create_dir_all(&mnt)?;
    sudo(&["mount", &p1, mnt.to_str().unwrap()])?;
    let result = (|| -> Result<()> {
        for d in ["EFI/BOOT", "loader/entries", "oath-install"] {
            sudo(&["mkdir", "-p", mnt.join(d).to_str().unwrap()])?;
        }
        sudo(&["cp", boot.to_str().unwrap(), mnt.join("EFI/BOOT/BOOTX64.EFI").to_str().unwrap()])?;
        sudo(&[
            "cp",
            tools.kernel.to_str().unwrap(),
            mnt.join("oath-install/vmlinuz").to_str().unwrap(),
        ])?;
        sudo(&[
            "cp",
            out.join("initrd-install.gz").to_str().unwrap(),
            mnt.join("oath-install/initrd.gz").to_str().unwrap(),
        ])?;
        let loader = "default oath-install.conf\ntimeout 5\n";
        let entry = "\
title Oath installer
linux /oath-install/vmlinuz
initrd /oath-install/initrd.gz
options console=ttyS0,115200 console=tty0 nomodeset net.ifnames=0 random.trust_cpu=on oath.install=1
";
        let tmp = out.join("usb-loader.conf");
        fs::write(&tmp, loader)?;
        sudo(&["cp", tmp.to_str().unwrap(), mnt.join("loader/loader.conf").to_str().unwrap()])?;
        fs::write(&tmp, entry)?;
        sudo(&[
            "cp",
            tmp.to_str().unwrap(),
            mnt.join("loader/entries/oath-install.conf").to_str().unwrap(),
        ])?;
        let _ = fs::remove_file(&tmp);
        sudo(&["sync"])?;
        Ok(())
    })();
    let _ = sudo(&["umount", mnt.to_str().unwrap()]);
    result?;
    eprintln!(
        "USB installer ready on {}. Plug into canto, hold Option, boot EFI.\n\
         tty0 should get a shell; DHCP + dropbear on the live NIC.\n\
         Then: cargo make install --target root@canto --disk /dev/sda --confirm --hostname canto",
        opts.disk
    );
    Ok(())
}

fn assert_usb_stick(dev: &str) -> Result<()> {
    let name = dev.trim_start_matches("/dev/");
    if name.is_empty() || name.contains('/') {
        bail!("--disk {dev} looks wrong for USB");
    }
    let rem = fs::read_to_string(format!("/sys/block/{name}/removable"))
        .with_context(|| format!("not a block device: {dev}"))?;
    if rem.trim() != "1" {
        bail!("{dev} is not removable; refusing to write USB installer");
    }
    let sys = format!("/sys/block/{name}");
    let is_usb =
        fs::canonicalize(&sys).map(|p| p.to_string_lossy().contains("/usb")).unwrap_or(false);
    if !is_usb {
        bail!("{dev} is removable but not USB; refusing");
    }
    let mounts = fs::read_to_string("/proc/mounts").unwrap_or_default();
    for line in mounts.lines() {
        let src = line.split_whitespace().next().unwrap_or("");
        let tgt = line.split_whitespace().nth(1).unwrap_or("");
        if src == dev || src.starts_with(&format!("{dev}p")) || src.starts_with(&format!("{name}"))
        {
            if tgt == "/" || tgt == "/boot" || tgt == "/home" || tgt.starts_with("/nix") {
                bail!("{dev} is mounted at {tgt}; refusing");
            }
        }
    }
    Ok(())
}

fn ssh_base(r: &Remote) -> Command {
    let mut c = Command::new("ssh");
    match &r.known_hosts {
        Some(f) => c
            .args(["-o", "StrictHostKeyChecking=yes", "-o"])
            .arg(format!("UserKnownHostsFile={}", f.display())),
        // The installer ramdisk makes a new host key every boot, so there is
        // nothing to pin. Only the keys baked from ~/.ssh can log in.
        None => c.args(["-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null"]),
    };
    c.args([
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=8",
        "-o",
        "LogLevel=ERROR",
        "-T",
        "-p",
        &r.port.to_string(),
        &format!("{}@{}", r.user, r.host),
    ]);
    c
}

/// `script` runs in the target's `/bin/sh`, fed on stdin, so the login
/// shell never parses it (canto's old image logs `home` into Thoxa). With
/// a non-root user the whole script runs under `sudo -n`.
fn sh_command(r: &Remote) -> Command {
    let mut c = ssh_base(r);
    sh_tail(&mut c, r);
    c
}

fn sh_tail(c: &mut Command, r: &Remote) {
    c.arg(if r.sudo { "sudo -n /bin/sh -s" } else { "/bin/sh -s" });
    c.stdin(Stdio::piped());
}

fn feed(child: &mut std::process::Child, script: &str) -> Result<()> {
    let mut stdin = child.stdin.take().context("ssh stdin")?;
    stdin.write_all(script.as_bytes()).context("ssh stdin")
}

fn ssh_run(r: &Remote, script: &str) -> Result<()> {
    let mut child = sh_command(r).spawn().context("ssh")?;
    feed(&mut child, script)?;
    let st = child.wait()?;
    if !st.success() {
        bail!("ssh `{script}` failed: {st}");
    }
    Ok(())
}

fn ssh_out(r: &Remote, script: &str) -> Result<String> {
    let mut child =
        sh_command(r).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("ssh")?;
    feed(&mut child, script)?;
    let o = child.wait_with_output()?;
    if !o.status.success() {
        bail!("ssh `{script}` failed: {}\n{}", o.status, String::from_utf8_lossy(&o.stderr));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Start `script` (a reboot or kexec) and give up on the session after a
/// few seconds: the connection dies with the kernel and never returns cleanly.
fn ssh_fire(r: &Remote, script: &str) {
    let mut c = Command::new("ssh");
    c.args(["-o", "ServerAliveInterval=2", "-o", "ServerAliveCountMax=2"]);
    c.args(ssh_base(r).get_args());
    sh_tail(&mut c, r);
    let Ok(mut child) = c.spawn() else { return };
    let _ = feed(&mut child, script);
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => break,
            Ok(None) if start.elapsed() > Duration::from_secs(8) => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(200)),
        }
    }
}

fn ssh_reboot(r: &Remote) -> Result<()> {
    ssh_fire(r, "/bin/busybox reboot -f");
    Ok(())
}

fn put_text(r: &Remote, dest: &str, body: &str) -> Result<()> {
    let tmp = std::env::temp_dir()
        .join(format!("oath-put-{}", dest.rsplit('/').next().unwrap_or("file")));
    fs::write(&tmp, body)?;
    let res = scp_root(r, &tmp, dest);
    let _ = fs::remove_file(&tmp);
    res
}

fn scp_root(r: &Remote, src: &Path, dest: &str) -> Result<()> {
    // Stream bytes over ssh so the first hop does not depend on guest
    // scp/sftp. `dd of=` needs no redirect, so any login shell can run it.
    let dd = format!("dd of={dest} bs=65536 status=none");
    let remote = if r.sudo { format!("sudo -n {dd}") } else { dd };
    let mut ssh = ssh_base(r);
    ssh.arg(&remote).stdin(Stdio::piped());
    let mut child = ssh.spawn().context("ssh put")?;
    {
        let mut stdin = child.stdin.take().context("ssh stdin")?;
        let mut f = fs::File::open(src).with_context(|| format!("open {}", src.display()))?;
        io::copy(&mut f, &mut stdin).context("put file")?;
    }
    let st = child.wait()?;
    if !st.success() {
        bail!("put {} -> {dest} failed: {st}", src.display());
    }
    Ok(())
}

fn already_installer(r: &Remote) -> bool {
    ssh_out(r, "test -f /run/oath-install/ready && echo YES")
        .map(|s| s.contains("YES"))
        .unwrap_or(false)
}

fn enter_installer(out: &Path, tools: &Tools, opts: &Opts, r: &Remote) -> Result<()> {
    // kexec on this Apple box jumps and never brings tg3 up. Firmware reboot
    // re-inits PCI; QEMU rehearsal does not use this path.
    if ssh_out(r, "test -d /sys/firmware/efi && echo YES")
        .map(|s| s.contains("YES"))
        .unwrap_or(false)
    {
        if let Err(e) = mount_esp(r, opts) {
            eprintln!("mount ESP: {e}");
        }
        if ssh_out(r, "test -d /boot/loader/entries && echo YES")
            .map(|s| s.contains("YES"))
            .unwrap_or(false)
        {
            return efi_oneshot_installer(out, tools, opts, r);
        }
        eprintln!("EFI but no loader entries; kexec fallback");
    }
    kexec_into_installer(out, tools, opts, r)
}

fn mount_esp(r: &Remote, opts: &Opts) -> Result<()> {
    if ssh_out(r, "test -d /boot/loader/entries && echo YES")
        .map(|s| s.contains("YES"))
        .unwrap_or(false)
    {
        return Ok(());
    }
    let (p1, _) = parts(&opts.disk);
    let dev = ssh_out(r, "blkid -L OATHESP").ok().filter(|s| s.starts_with("/dev/")).unwrap_or(p1);
    let _ = ssh_run(r, "modprobe vfat || true");
    ssh_run(r, "mkdir -p /boot")?;
    ssh_run(r, &format!("mount -t vfat {dev} /boot"))?;
    if !ssh_out(r, "test -d /boot/loader/entries && echo YES")
        .map(|s| s.contains("YES"))
        .unwrap_or(false)
    {
        bail!("mounted {dev} on /boot but loader/entries is missing");
    }
    eprintln!("ESP {dev} mounted on /boot");
    Ok(())
}

fn installer_cmdline(r: &Remote) -> Result<String> {
    let nic = discover_nic(r)?;
    let mac = discover_mac(r, &nic)?;
    if let Ok((ip, gw)) = discover_ip_gw(r, &nic) {
        eprintln!("live {nic} {mac} {ip} via {gw}; installer uses dhcp on that MAC");
    }
    Ok(format!(
        "console=tty0 console=ttyS0,115200 nomodeset net.ifnames=0 random.trust_cpu=on oath.install=1 oath.mac={mac}"
    ))
}

fn discover_mac(r: &Remote, nic: &str) -> Result<String> {
    let s = ssh_out(r, &format!("cat /sys/class/net/{nic}/address"))?;
    if s.is_empty() {
        bail!("no mac for {nic}");
    }
    Ok(s.to_lowercase())
}

fn efi_oneshot_installer(out: &Path, tools: &Tools, _opts: &Opts, r: &Remote) -> Result<()> {
    let cmdline = installer_cmdline(r)?;
    eprintln!("EFI oneshot installer cmdline: {cmdline}");
    ssh_run(r, "mkdir -p /boot/oath-install /boot/loader/entries")?;
    scp_root(r, &tools.kernel, "/boot/oath-install/vmlinuz")?;
    scp_root(r, &out.join("initrd-install.gz"), "/boot/oath-install/initrd.gz")?;
    let entry = format!(
        "title Oath installer\nlinux /oath-install/vmlinuz\ninitrd /oath-install/initrd.gz\noptions {cmdline}\n"
    );
    // oath-efi (BOOTX64) boots the entries `loader/oath-boots` names, else
    // loader.conf's default; oath.conf covers an ESP that has oath-boots.
    put_text(r, "/boot/loader/entries/oath.conf", &entry)?;
    put_text(r, "/boot/loader/entries/oath-install.conf", &entry)?;
    put_text(r, "/boot/loader/loader.conf", "default oath-install.conf\ntimeout 1\n")?;
    let _ = ssh_run(r, "sync");
    eprintln!("rebooting into installer (firmware re-inits PCI)");
    ssh_fire(r, REBOOT_SCRIPT);
    Ok(())
}

fn kexec_into_installer(out: &Path, tools: &Tools, _opts: &Opts, r: &Remote) -> Result<()> {
    let kexec = tools.bin("kexec", "kexec")?;
    ssh_run(r, "mkdir -p /tmp/oath-install")?;
    scp_root(r, &kexec, "/tmp/oath-install/kexec")?;
    scp_root(r, &tools.kernel, "/tmp/oath-install/vmlinuz")?;
    scp_root(r, &out.join("initrd-install.gz"), "/tmp/oath-install/initrd.gz")?;
    ssh_run(r, "chmod +x /tmp/oath-install/kexec")?;
    let cmdline = installer_cmdline(r)?;
    eprintln!("kexec cmdline: {cmdline}");
    ssh_run(
        r,
        &format!(
            "/tmp/oath-install/kexec -c -l /tmp/oath-install/vmlinuz --initrd=/tmp/oath-install/initrd.gz --command-line={}",
            sh_quote(&cmdline)
        ),
    )?;
    eprintln!("kexec -e (SSH will drop)");
    // kexec replaces the kernel: the SSH session never returns cleanly.
    ssh_fire(r, "/tmp/oath-install/kexec --force --no-ifdown -e");
    Ok(())
}

fn discover_nic(r: &Remote) -> Result<String> {
    let s = ssh_out(r, "ip -o route show default | awk '{print $5; exit}'")?;
    if s.is_empty() {
        bail!("no default route nic on target");
    }
    Ok(s)
}

fn discover_ip_gw(r: &Remote, nic: &str) -> Result<(String, String)> {
    let ip = ssh_out(r, &format!("ip -o -4 addr show dev {nic} | awk '{{print $4; exit}}'"))?;
    let gw = ssh_out(r, "ip -o route show default | awk '{print $3; exit}'")?;
    if ip.is_empty() || gw.is_empty() {
        bail!("could not read ip/gw on {nic}");
    }
    Ok((ip, gw))
}

fn wait_ssh(r: &Remote, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let addr = format!("{}:{}", r.host, r.port);
    eprintln!("waiting for installer SSH at {addr}");
    while Instant::now() < deadline {
        let sock = addr.to_socket_addrs().ok().and_then(|mut i| i.next());
        let ok_tcp = sock
            .map(|a| TcpStream::connect_timeout(&a, Duration::from_secs(2)).is_ok())
            .unwrap_or(false);
        if ok_tcp {
            if ssh_out(r, "test -f /run/oath-install/ready && echo YES")
                .map(|s| s.contains("YES"))
                .unwrap_or(false)
            {
                eprintln!("installer ramdisk is up");
                return Ok(());
            }
        }
        thread::sleep(Duration::from_secs(3));
    }
    bail!("timed out waiting for installer SSH")
}

/// Partition, copy the packed root, write the catalog and the ESP. Returns
/// the installed system's SSH host key (`ssh-ed25519 AAAA…`), made here so
/// the first login to the new system can be checked against it.
fn format_and_copy(out: &Path, _tools: &Tools, opts: &Opts, r: &Remote) -> Result<String> {
    let (p1, p2) = parts(&opts.disk);
    let hostname = opts.hostname.clone().unwrap_or_else(|| "oath".into());
    eprintln!("format {} -> ESP {p1} + btrfs {p2}", opts.disk);
    let script = format_disk_script(&opts.disk, &p1, &p2);
    ssh_run(r, &format!("PATH=/bin sh -c {}", sh_quote(&script)))?;

    eprintln!("copy packed root");
    let stage = out.join("stage");
    let mut tar = Command::new("tar");
    tar.args([
        "-C",
        stage.to_str().unwrap(),
        "--format=gnu",
        "--numeric-owner",
        "--owner=0",
        "--group=0",
        "-cf",
        "-",
    ]);
    tar.arg(".");
    let mut ssh = ssh_base(r);
    let remote = if r.sudo { "sudo -n tar -C /mnt -xf -" } else { "tar -C /mnt -xf -" };
    ssh.arg(remote).stdin(Stdio::piped());
    let mut child = ssh.spawn().context("ssh tar")?;
    {
        let mut stdin = child.stdin.take().context("ssh stdin")?;
        let mut src = tar.stdout(Stdio::piped()).spawn().context("tar")?;
        let mut outp = src.stdout.take().context("tar stdout")?;
        std::io::copy(&mut outp, &mut stdin).context("pipe tar")?;
        let _ = src.wait();
    }
    let st = child.wait()?;
    if !st.success() {
        bail!("remote tar extract failed");
    }
    ssh_run(
        r,
        &format!(
            "/bin/busybox chown -R 0:0 /mnt && /bin/busybox chmod 755 /mnt && /bin/busybox chmod 700 /mnt/root && /bin/busybox mkdir -p /mnt/root/.ssh /mnt/home/.ssh && /bin/busybox chown -R {uid}:{gid} /mnt/home && /bin/busybox chmod 755 /mnt/home && /bin/busybox chmod 700 /mnt/home/.ssh && /bin/busybox chmod 4755 /mnt/lib/oath/sudo",
            uid = oath_core::seat::UID,
            gid = oath_core::seat::GID,
        ),
    )?;

    write_catalog_overlay(opts, &hostname, r)?;
    let host_key = make_host_key(r)?;

    ssh_run(r, "mkdir -p /esp/EFI/BOOT /esp/loader/entries")?;
    ssh_run(
        r,
        "test -f /opt/oath-install/BOOTX64.EFI && test -f /opt/oath-install/vmlinuz && test -f /opt/oath-install/initrd.gz",
    )?;
    ssh_run(
        r,
        "cp /opt/oath-install/BOOTX64.EFI /esp/EFI/BOOT/BOOTX64.EFI && cp /opt/oath-install/vmlinuz /esp/vmlinuz && cp /opt/oath-install/initrd.gz /esp/initrd.gz && sync",
    )?;
    let entry = crate::boot::current_bls(&p2, "");
    let loader =
        if opts.qemu { crate::boot::LOADER_CONF_QEMU } else { crate::boot::LOADER_CONF_METAL };
    let boots = crate::boot::format_oath_boots(&[String::from("oath.conf")]);
    ssh_run(
        r,
        &format!(
            "printf '%s' {q} > /esp/loader/loader.conf && printf '%s' {e} > /esp/loader/entries/oath.conf && printf '%s' {b} > /esp/loader/oath-boots",
            q = sh_quote(loader),
            e = sh_quote(&entry),
            b = sh_quote(&boots),
        ),
    )?;
    ssh_run(r, "sync; umount /mnt/boot /mnt /esp || true")?;
    eprintln!("disk written; reboot");
    Ok(host_key)
}

/// The installer's dropbearkey writes the key PID 1 would otherwise make
/// at first boot (`oath_core::SSH_HOST_KEY`).
fn make_host_key(r: &Remote) -> Result<String> {
    let key = format!("/mnt{}", oath_core::SSH_HOST_KEY);
    let dir = key.rsplit_once('/').map(|(d, _)| d).unwrap_or("/mnt/oath/ssh");
    let out = ssh_out(
        r,
        &format!(
            "mkdir -p {dir} && rm -f {key} && dropbearkey -t ed25519 -f {key} >/dev/null 2>&1 && dropbearkey -y -f {key}"
        ),
    )?;
    let line = out
        .lines()
        .find(|l| l.starts_with("ssh-ed25519 "))
        .context("dropbearkey printed no public key")?;
    // known_hosts wants `type blob`; drop dropbear's `root@host` comment.
    let mut words = line.split_whitespace();
    Ok(format!("{} {}", words.next().unwrap_or(""), words.next().unwrap_or("")))
}

/// Replace `host`'s line in a known_hosts file with `key`.
pub fn pin_host_key(file: &Path, host: &str, port: u16, key: &str) -> Result<()> {
    let name = if port == 22 { host.to_string() } else { format!("[{host}]:{port}") };
    let old = fs::read_to_string(file).unwrap_or_default();
    let mut body: String = old
        .lines()
        .filter(|l| l.split_whitespace().next() != Some(name.as_str()))
        .map(|l| format!("{l}\n"))
        .collect();
    body.push_str(&format!("{name} {key}\n"));
    fs::write(file, body).with_context(|| format!("write {}", file.display()))
}

fn format_disk_script(disk: &str, p1: &str, p2: &str) -> String {
    format!(
        r#"
set -e
DISK={disk}
P1={p1}
P2={p2}
wipefs -a "$DISK" || true
printf 'label: gpt\nsize=512MiB, type=uefi, name=ESP\ntype=linux, name=oath\n' | sfdisk --wipe always --quiet "$DISK"
blockdev --rereadpt "$DISK" || true
mdev -s || true
for i in 1 2 3 4 5 6 7 8 9 10; do
  [ -b "$P1" ] && [ -b "$P2" ] && break
  mdev -s || true
  sleep 1
done
[ -b "$P1" ] && [ -b "$P2" ]
mkfs.fat -F 32 -n OATHESP "$P1"
mkfs.btrfs -f -L oath "$P2"
mkdir -p /mnt /esp
mount "$P2" /mnt
btrfs subvolume create /mnt/@
umount /mnt
mount -o subvol=@ "$P2" /mnt
mount -t vfat "$P1" /esp
mkdir -p /mnt/boot
mount --bind /esp /mnt/boot
"#
    )
}

/// Read and check `--overlay` before anything is wiped.
fn read_overlay(path: &Path) -> Result<serde_json::Value> {
    let text = fs::read_to_string(path).with_context(|| format!("overlay {}", path.display()))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("overlay {}", path.display()))?;
    let scratch = tempfile::tempdir()?;
    oath_core::seed(scratch.path()).map_err(|e| anyhow::anyhow!("{e}"))?;
    oath_core::apply_overlay(scratch.path(), &v)
        .map_err(|e| anyhow::anyhow!("overlay {}: {e}", path.display()))?;
    Ok(v)
}

/// What the installer writes into the new catalog: hostname, the host's
/// SSH public keys for `home`, then the user's overlay on top.
fn catalog_overlay(
    hostname: &str,
    keys: &str,
    user: Option<&serde_json::Value>,
) -> serde_json::Value {
    let authorized: Vec<&str> = keys
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("ssh-") || l.starts_with("ecdsa-"))
        .collect();
    let base = serde_json::json!({"objects": {
        "host:local": {"hostname": hostname},
        "ssh:local": {"authorized": authorized}
    }});
    match user {
        Some(u) => oath_core::merge_patch(base, u),
        None => base,
    }
}

fn write_catalog_overlay(opts: &Opts, hostname: &str, r: &Remote) -> Result<()> {
    let keys = qemu::host_pubkeys_body().unwrap_or_default();
    let user = match &opts.overlay {
        Some(p) => Some(read_overlay(p)?),
        None => None,
    };
    let overlay = catalog_overlay(hostname, &keys, user.as_ref());
    put_text(r, "/tmp/oath-overlay.json", &serde_json::to_string_pretty(&overlay)?)?;
    ssh_run(r, "/bin/oath --root /mnt/oath seed --overlay /tmp/oath-overlay.json")
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Non-destructive ESP write: archive the live kernel+initrd, copy the
/// packed image, keep five boots. Does not format.
/// `cargo make esp` mounts a local ESP and snapshots the local `@`, so it
/// only makes sense on the Oath machine itself, never on the build host.
fn require_oath_system(sys_root: &Path) -> Result<()> {
    if !sys_root.join("oath/INDEX.md").is_file() {
        bail!(
            "this machine is not Oath ({} missing); `esp` writes the boot partition of \
             the machine it runs on. Run it on the Oath host.",
            sys_root.join("oath/INDEX.md").display()
        );
    }
    Ok(())
}

pub fn update_esp(repo: &Path, out: &Path, opts: EspOpts) -> Result<()> {
    crate::boot::require_confirm(opts.confirm)?;
    require_oath_system(Path::new("/"))?;
    if !opts.esp.starts_with("/dev/") {
        bail!("--esp must be a /dev/ node (got {:?})", opts.esp);
    }
    let tools = tools::load(repo)?;
    pack::boot_image(repo, out, &tools)?;
    let efi = repo.join("target/x86_64-unknown-uefi/release/oath-efi.efi");
    let efi_alt = repo.join("target/x86_64-unknown-uefi/release/oath-efi");
    let efi = if efi.is_file() {
        efi
    } else if efi_alt.is_file() {
        efi_alt
    } else {
        bail!("oath-efi missing after boot_image");
    };
    let mnt = out.join("esp-mnt");
    let _ = fs::create_dir_all(&mnt);
    let _ = sudo(&["umount", mnt.to_str().unwrap()]);
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    let mnt_opts = format!("rw,uid={uid},gid={gid},umask=022");
    sudo(&["mount", "-t", "vfat", "-o", &mnt_opts, &opts.esp, mnt.to_str().unwrap()])?;
    let result = (|| -> Result<u64> {
        let (id, prune) = crate::boot::apply_rotate_files(
            &mnt,
            &tools.kernel,
            &out.join("initrd.gz"),
            efi.is_file().then_some(efi.as_path()),
            &opts.root_dev,
            "",
            crate::boot::LOADER_CONF_METAL,
        )?;
        snapshot_boot(id);
        for old in prune {
            delete_boot_snapshot(old);
        }
        Ok(id)
    })();
    let _ = sudo(&["sync"]);
    let _ = sudo(&["umount", mnt.to_str().unwrap()]);
    let id = result?;
    eprintln!("ESP updated; archived as boot {id}. Reboot and pick from the firmware menu if the new kernel fails.");
    Ok(())
}

fn snapshot_boot(id: u64) {
    let src = Path::new(oath_core::BTRFS_TOP).join("@");
    let dst = Path::new(oath_core::BTRFS_TOP).join(oath_core::boot_subvol_name(id));
    if !src.is_dir() || dst.exists() {
        return;
    }
    let _ = sudo(&["btrfs", "subvolume", "snapshot", src.to_str().unwrap(), dst.to_str().unwrap()]);
}

fn delete_boot_snapshot(id: u64) {
    let dst = Path::new(oath_core::BTRFS_TOP).join(oath_core::boot_subvol_name(id));
    if dst.exists() {
        let _ = sudo(&["btrfs", "subvolume", "delete", dst.to_str().unwrap()]);
    }
}

fn install_remote(out: &Path, tools: &Tools, opts: &Opts, r: &Remote) -> Result<()> {
    if !already_installer(r) {
        enter_installer(out, tools, opts, r)?;
    } else {
        eprintln!("already in installer ramdisk");
    }
    let inst = Remote {
        user: "root".into(),
        host: r.host.clone(),
        port: 22,
        sudo: false,
        known_hosts: None,
    };
    wait_ssh(&inst, Duration::from_secs(300))?;
    let key = format_and_copy(out, tools, opts, &inst)?;
    let pins = known_hosts(out);
    pin_host_key(&pins, &r.host, r.port, &key)?;
    eprintln!("host key for {} pinned in {}", r.host, pins.display());
    let _ = ssh_reboot(&inst);
    let live = wait_oath_ssh(&r.host, r.port, &pins, Duration::from_secs(300))?;
    let ls = ssh_out(&live, "/bin/oath ls")?;
    eprintln!("oath ls ({}@{}):\n{ls}", live.user, live.host);
    if !ls.contains("host:local") {
        bail!("installed system has no host:local");
    }
    Ok(())
}

fn install_qemu(root: &Path, out: &Path, tools: &Tools, opts: &Opts) -> Result<()> {
    let ovmf_code = tools.ovmf_code.as_ref().context("OVMF_CODE.fd missing")?;
    let ovmf_vars_src = tools.ovmf_vars.as_ref().context("OVMF_VARS.fd missing")?;
    let run_dir = out.join("runs").join(format!("{}-install", utc_stamp()));
    fs::create_dir_all(&run_dir)?;
    let disk = run_dir.join("disk.qcow2");
    run(Command::new(&tools.qemu_img).args([
        "create",
        "-f",
        "qcow2",
        disk.to_str().unwrap(),
        "8G",
    ]))?;
    let port = qemu::ssh_port();
    let serial = run_dir.join("serial.log");
    let netdev = format!("user,id=n0,hostfwd=tcp:127.0.0.1:{port}-:22");
    let drive = format!("file={},if=none,id=hd0,format=qcow2", disk.display());
    let mut qemu_cmd = Command::new(&tools.qemu);
    qemu_headless(&mut qemu_cmd, &serial, &netdev, &drive);
    if kvm() {
        qemu_cmd.arg("-enable-kvm");
    }
    qemu_cmd.args([
        "-kernel",
        tools.kernel.to_str().unwrap(),
        "-initrd",
        out.join("initrd-install.gz").to_str().unwrap(),
        "-append",
        "console=ttyS0 oath.install=1 oath.nic=eth0 oath.ip=10.0.2.15/24 oath.gw=10.0.2.2",
    ]);
    eprintln!(">> installer qemu {}", run_dir.display());
    let mut child = qemu_cmd.spawn().context("qemu installer")?;
    let inst = Remote {
        user: "root".into(),
        host: "127.0.0.1".into(),
        port,
        sudo: false,
        known_hosts: None,
    };
    if let Err(e) = wait_ssh(&inst, Duration::from_secs(90)) {
        let _ = child.kill();
        dump_serial(&serial);
        return Err(e);
    }
    let qopts = Opts {
        target: None,
        disk: opts.disk.clone(),
        confirm: true,
        qemu: true,
        usb: false,
        hostname: opts.hostname.clone().or_else(|| Some("oath".into())),
        overlay: opts.overlay.clone(),
        unchecked: true,
    };
    let key = match format_and_copy(out, tools, &qopts, &inst) {
        Ok(k) => k,
        Err(e) => {
            let _ = child.kill();
            dump_serial(&serial);
            return Err(e);
        }
    };
    // The rehearsal pins too, so the first login proves the key it wrote.
    let pins = run_dir.join("known_hosts");
    pin_host_key(&pins, "127.0.0.1", port, &key)?;
    let _ = child.kill();
    let _ = child.wait();

    let vars = run_dir.join("OVMF_VARS.fd");
    copy_file(ovmf_vars_src, &vars)?;
    fs::set_permissions(&vars, fs::Permissions::from_mode(0o644))?;
    let serial_boot = run_dir.join("serial-boot.log");
    let qemu_boot_log = run_dir.join("qemu-boot.log");
    let mut boot = Command::new(&tools.qemu);
    if kvm() {
        boot.arg("-enable-kvm");
    }
    boot.args([
        "-drive",
        &format!("if=pflash,format=raw,readonly=on,file={}", ovmf_code.display()),
        "-drive",
        &format!("if=pflash,format=raw,file={}", vars.display()),
    ]);
    qemu_headless(&mut boot, &serial_boot, &netdev, &drive);
    let boot_err = fs::File::create(&qemu_boot_log).context("qemu-boot.log")?;
    boot.stdout(Stdio::null()).stderr(Stdio::from(boot_err));
    eprintln!(">> OVMF boot {}", run_dir.display());
    let mut boot_child = boot.spawn().context("qemu ovmf")?;
    thread::sleep(Duration::from_millis(400));
    if let Ok(Some(st)) = boot_child.try_wait() {
        dump_serial(&qemu_boot_log);
        dump_serial(&serial_boot);
        bail!("OVMF qemu exited immediately: {st}");
    }
    let wait = wait_oath_ssh("127.0.0.1", port, &pins, Duration::from_secs(180));
    let ok = wait.is_ok();
    if ok {
        let live = wait.as_ref().expect("ok");
        let ls = ssh_out(live, "oath ls")?;
        eprintln!("oath ls:\n{ls}");
        if !ls.contains("host:local") {
            let _ = boot_child.kill();
            dump_serial(&qemu_boot_log);
            dump_serial(&serial_boot);
            bail!("installed system has no host:local");
        }
    }
    let _ = boot_child.kill();
    let _ = boot_child.wait();
    if !ok {
        dump_serial(&qemu_boot_log);
        dump_serial(&serial_boot);
        return wait.map(|_| ());
    }
    eprintln!("QEMU-EFI rehearsal ok  {}", run_dir.display());
    let _ = root;
    Ok(())
}

fn qemu_headless(cmd: &mut Command, serial: &Path, netdev: &str, drive: &str) {
    cmd.args([
        "-machine",
        "q35",
        "-m",
        "4096",
        "-display",
        "none",
        "-monitor",
        "none",
        "-serial",
        &format!("file:{}", serial.display()),
        "-netdev",
        netdev,
        "-device",
        "virtio-net-pci,netdev=n0",
        "-device",
        "virtio-rng-pci",
        "-drive",
        drive,
        "-device",
        "virtio-blk-pci,drive=hd0,bootindex=1",
        "-no-reboot",
    ]);
}

fn dump_serial(path: &Path) {
    match fs::read_to_string(path) {
        Ok(s) => {
            let lines: Vec<&str> = s.lines().collect();
            let tail = lines.iter().rev().take(60).rev().cloned().collect::<Vec<_>>().join("\n");
            eprintln!("serial tail {}:\n{tail}", path.display());
        }
        Err(e) => eprintln!("no serial log {}: {e}", path.display()),
    }
}

/// Reboot into the installer entry. The disk is about to be wiped, so a
/// clean shutdown buys nothing; flushing the ESP is what matters, and a
/// forced reboot does not depend on the old system's PID 1.
const REBOOT_SCRIPT: &str = "sync; umount /boot 2>/dev/null; sync; reboot -f\n";

fn wait_oath_ssh(host: &str, port: u16, pins: &Path, timeout: Duration) -> Result<Remote> {
    let home = Remote {
        user: "home".into(),
        host: host.into(),
        port,
        sudo: false,
        known_hosts: Some(pins.to_path_buf()),
    };
    let deadline = Instant::now() + timeout;
    eprintln!("waiting for installed SSH at home@{host}:{port}");
    while Instant::now() < deadline {
        if ssh_out(&home, "test -f /oath/INDEX.md && echo YES")
            .map(|s| s.contains("YES"))
            .unwrap_or(false)
        {
            return Ok(home);
        }
        thread::sleep(Duration::from_secs(3));
    }
    bail!("timed out waiting for installed Oath SSH")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_vda_and_nvme() {
        assert_eq!(parts("/dev/vda"), ("/dev/vda1".into(), "/dev/vda2".into()));
        assert_eq!(parts("/dev/sda"), ("/dev/sda1".into(), "/dev/sda2".into()));
        assert_eq!(parts("/dev/nvme0n1"), ("/dev/nvme0n1p1".into(), "/dev/nvme0n1p2".into()));
    }

    #[test]
    fn pin_replaces_only_that_host() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("known_hosts");
        fs::write(&f, "10.0.0.2 ssh-ed25519 OTHER\n10.0.0.3 ssh-ed25519 OLD\n").unwrap();
        pin_host_key(&f, "10.0.0.3", 22, "ssh-ed25519 NEW").unwrap();
        pin_host_key(&f, "127.0.0.1", 2222, "ssh-ed25519 VM").unwrap();
        let s = fs::read_to_string(&f).unwrap();
        assert_eq!(
            s,
            "10.0.0.2 ssh-ed25519 OTHER\n10.0.0.3 ssh-ed25519 NEW\n[127.0.0.1]:2222 ssh-ed25519 VM\n"
        );
    }

    #[test]
    fn parse_target_user_host() {
        let r = parse_target("joshua@canto").unwrap();
        assert_eq!(r.user, "joshua");
        assert_eq!(r.host, "canto");
        assert_eq!(r.port, 22);
        assert!(r.sudo);
    }

    #[test]
    fn esp_refuses_a_non_oath_host() {
        let d = tempfile::tempdir().unwrap();
        assert!(require_oath_system(d.path()).is_err());
        fs::create_dir_all(d.path().join("oath")).unwrap();
        fs::write(d.path().join("oath/INDEX.md"), "# Oath\n").unwrap();
        assert!(require_oath_system(d.path()).is_ok());
    }

    #[test]
    fn format_is_esp_plus_one_pool() {
        let script = format_disk_script("/dev/sda", "/dev/sda1", "/dev/sda2");
        assert!(script.contains("mkfs.btrfs -f -L oath"));
        assert!(script.contains("btrfs subvolume create /mnt/@\n"));
        assert!(!script.contains("preserve"));
    }

    #[test]
    fn overlay_has_hostname_keys_and_user_patch() {
        let user = serde_json::json!({"objects": {
            "host:local": {"timezone": "MST7MDT"},
            "svc:backup": {"enabled": false}
        }});
        let o = catalog_overlay("canto", "ssh-ed25519 AAAA me\nnot a key\n", Some(&user));
        assert_eq!(o["objects"]["host:local"]["hostname"], "canto");
        assert_eq!(o["objects"]["host:local"]["timezone"], "MST7MDT");
        assert_eq!(o["objects"]["ssh:local"]["authorized"][0], "ssh-ed25519 AAAA me");
        assert_eq!(o["objects"]["ssh:local"]["authorized"].as_array().unwrap().len(), 1);
        assert_eq!(o["objects"]["svc:backup"]["enabled"], false);
    }
}
