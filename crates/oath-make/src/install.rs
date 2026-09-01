//! SSH+kexec install (nixos-anywhere shape). Wipes `--disk` on `--target`.

use std::fs;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
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
}

struct Remote {
    user: String,
    host: String,
    port: u16,
    sudo: bool,
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
        pack::build(root, out, &tools)?;
    }
    if !out.join("initramfs-install/init").is_file() {
        pack::build(root, out, &tools)?;
    }
    let keys =
        qemu::host_pubkeys_body().context("need host SSH public keys to log into the installer")?;
    pack::bake_install_keys(out, &keys)?;
    if opts.usb {
        return write_usb_installer(out, &tools, &opts);
    }
    if opts.qemu {
        return install_qemu(root, out, &tools, &opts);
    }
    let target = opts.target.as_deref().context("--target user@host (or --qemu or --usb)")?;
    let remote = parse_target(target)?;
    install_remote(out, &tools, &opts, &remote)
}

fn parse_target(s: &str) -> Result<Remote> {
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
    Ok(Remote { sudo: user != "root", user, host, port })
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

fn write_usb_installer(out: &Path, tools: &Tools, opts: &Opts) -> Result<()> {
    assert_usb_stick(&opts.disk)?;
    let boot = tools.systemd_boot.as_ref().context("systemd-bootx64.efi missing")?;
    let mkfat = tools.mkfs_fat.as_ref().context("mkfs.fat missing")?;
    let sgdisk = tools.sgdisk.as_ref().context("sgdisk missing")?;
    let (p1, _) = parts(&opts.disk);
    eprintln!("USB installer -> {} (wipe GPT, ESP, installer ramdisk)", opts.disk);
    let _ = sudo(&["umount", &p1]);
    let _ = sudo(&["umount", &opts.disk]);
    let _ = sudo(&["wipefs", "-a", &opts.disk]);
    let _ = sudo(&[sgdisk.to_str().unwrap(), "-Z", &opts.disk]);
    sudo(&[
        sgdisk.to_str().unwrap(),
        "-n",
        "1:0:+1G",
        "-t",
        "1:ef00",
        "-c",
        "1:OATHUSB",
        &opts.disk,
    ])?;
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
        for d in ["EFI/BOOT", "EFI/systemd", "loader/entries", "oath-install"] {
            sudo(&["mkdir", "-p", mnt.join(d).to_str().unwrap()])?;
        }
        sudo(&["cp", boot.to_str().unwrap(), mnt.join("EFI/BOOT/BOOTX64.EFI").to_str().unwrap()])?;
        sudo(&[
            "cp",
            boot.to_str().unwrap(),
            mnt.join("EFI/systemd/systemd-bootx64.efi").to_str().unwrap(),
        ])?;
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
    let is_usb = fs::canonicalize(&sys)
        .map(|p| p.to_string_lossy().contains("/usb"))
        .unwrap_or(false);
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
    c.args([
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "UserKnownHostsFile=/dev/null",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=8",
        "-T",
        "-p",
        &r.port.to_string(),
        &format!("{}@{}", r.user, r.host),
    ]);
    c
}

fn ssh_run(r: &Remote, remote_cmd: &str) -> Result<()> {
    let cmd = if r.sudo { format!("sudo -n {remote_cmd}") } else { remote_cmd.to_string() };
    let st = ssh_base(r).arg(&cmd).status().context("ssh")?;
    if !st.success() {
        bail!("ssh {cmd} failed: {st}");
    }
    Ok(())
}

fn ssh_out(r: &Remote, remote_cmd: &str) -> Result<String> {
    let cmd = if r.sudo { format!("sudo -n {remote_cmd}") } else { remote_cmd.to_string() };
    let o = ssh_base(r).arg(&cmd).output().context("ssh")?;
    if !o.status.success() {
        bail!("ssh {cmd} failed: {}\n{}", o.status, String::from_utf8_lossy(&o.stderr));
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn put_text(r: &Remote, dest: &str, body: &str) -> Result<()> {
    let tmp = std::env::temp_dir().join(format!(
        "oath-put-{}",
        dest.rsplit('/').next().unwrap_or("file")
    ));
    fs::write(&tmp, body)?;
    let res = scp_root(r, &tmp, dest);
    let _ = fs::remove_file(&tmp);
    res
}

fn scp_root(r: &Remote, src: &Path, dest: &str) -> Result<()> {
    // Dropbear ramdisk has no sftp-server and no scp applet. Stream bytes
    // over ssh. `sudo tee` because `sudo cat > dest` redirects as the user.
    let remote = if r.sudo {
        format!("sudo -n tee {dest} >/dev/null")
    } else {
        format!("/bin/busybox cat > {dest}")
    };
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
    if ssh_out(r, "test -d /sys/firmware/efi && test -d /boot/loader/entries && echo YES")
        .map(|s| s.contains("YES"))
        .unwrap_or(false)
    {
        return efi_oneshot_installer(out, tools, opts, r);
    }
    kexec_into_installer(out, tools, opts, r)
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
    put_text(r, "/boot/loader/entries/oath-install.conf", &entry)?;
    let oneshot = ssh_run(r, "bootctl set-oneshot oath-install.conf");
    if oneshot.is_err() {
        eprintln!("bootctl set-oneshot failed; writing loader.conf default");
        put_text(r, "/boot/loader/loader.conf", "default oath-install.conf\ntimeout 1\n")?;
    }
    eprintln!("rebooting into installer (firmware re-inits PCI)");
    let mut c = ssh_base(r);
    c.args(["-o", "ServerAliveInterval=2", "-o", "ServerAliveCountMax=2"]);
    c.arg(if r.sudo { "sudo -n systemctl reboot" } else { "systemctl reboot" });
    if let Ok(mut child) = c.spawn() {
        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(8) {
                let _ = child.kill();
                break;
            }
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => thread::sleep(Duration::from_millis(200)),
                Err(_) => break,
            }
        }
    }
    Ok(())
}

fn kexec_into_installer(out: &Path, tools: &Tools, _opts: &Opts, r: &Remote) -> Result<()> {
    let kexec = tools.kexec.as_ref().context("kexec tool missing from tools.nix")?;
    ssh_run(r, "mkdir -p /tmp/oath-install")?;
    scp_root(r, kexec, "/tmp/oath-install/kexec")?;
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
    // Time it out so we can wait for the installer ramdisk.
    let mut c = ssh_base(r);
    c.args(["-o", "ServerAliveInterval=2", "-o", "ServerAliveCountMax=2"]);
    c.arg(if r.sudo {
        "sudo -n /tmp/oath-install/kexec --force --no-ifdown -e"
    } else {
        "/tmp/oath-install/kexec --force --no-ifdown -e"
    });
    if let Ok(mut child) = c.spawn() {
        let start = Instant::now();
        loop {
            if start.elapsed() > Duration::from_secs(8) {
                let _ = child.kill();
                break;
            }
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => thread::sleep(Duration::from_millis(200)),
                Err(_) => break,
            }
        }
    }
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

fn format_and_copy(out: &Path, _tools: &Tools, opts: &Opts, r: &Remote) -> Result<()> {
    let (p1, p2) = parts(&opts.disk);
    let hostname = opts.hostname.clone().unwrap_or_else(|| "oath".into());
    eprintln!("format {} -> ESP {p1} + btrfs {p2}", opts.disk);
    let script = format!(
        r#"
set -e
DISK={disk}
P1={p1}
P2={p2}
sgdisk -Z "$DISK" || true
sgdisk -n 1:0:+512M -t 1:ef00 -c 1:ESP "$DISK"
sgdisk -n 2:0:0 -t 2:8300 -c 2:oath "$DISK"
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
"#,
        disk = opts.disk,
        p1 = p1,
        p2 = p2,
    );
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
        ".",
    ]);
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
        "/bin/busybox chown -R 0:0 /mnt && /bin/busybox chmod 755 /mnt && /bin/busybox chmod 700 /mnt/root && /bin/busybox mkdir -p /mnt/root/.ssh && /bin/busybox chmod 700 /mnt/root/.ssh",
    )?;

    patch_catalog(out, opts, &hostname, r)?;

    ssh_run(r, "mkdir -p /esp/EFI/BOOT /esp/EFI/systemd /esp/loader/entries")?;
    ssh_run(
        r,
        "test -f /opt/oath-install/BOOTX64.EFI && test -f /opt/oath-install/vmlinuz && test -f /opt/oath-install/initrd.gz",
    )?;
    ssh_run(
        r,
        "cp /opt/oath-install/BOOTX64.EFI /esp/EFI/BOOT/BOOTX64.EFI && cp /opt/oath-install/BOOTX64.EFI /esp/EFI/systemd/systemd-bootx64.efi && cp /opt/oath-install/vmlinuz /esp/vmlinuz && cp /opt/oath-install/initrd.gz /esp/initrd.gz && sync",
    )?;
    let entry = format!(
        "title Oath\nlinux /vmlinuz\ninitrd /initrd.gz\noptions console=ttyS0,115200 console=tty0 amdgpu.si_support=1 radeon.si_support=0 oath.root={p2}\n"
    );
    ssh_run(
        r,
        &format!(
            "printf '%s' {q} > /esp/loader/loader.conf && printf '%s' {e} > /esp/loader/entries/oath.conf",
            q = sh_quote("default oath.conf\ntimeout 5\n"),
            e = sh_quote(&entry),
        ),
    )?;
    ssh_run(r, "umount /mnt/boot /mnt /esp || true")?;
    eprintln!("disk written; reboot");
    Ok(())
}

fn patch_catalog(out: &Path, opts: &Opts, hostname: &str, r: &Remote) -> Result<()> {
    // Hostname + dhcp on the installed catalog. Keys already in desired via tar + extra write.
    let host_json = format!(r#"{{"hostname":"{hostname}","power":"run"}}"#);
    let net_json = r#"{"up":true,"ipv4":"dhcp","gateway":"","lease":null}"#;
    ssh_run(
        r,
        &format!(
            "printf '%s\\n' {h} > /mnt/oath/objects/host/local/desired.json && printf '%s\\n' {n} > /mnt/oath/objects/net/net0/desired.json && printf '%s\\n' {h} > /mnt/oath/objects/host/local/actual.json && printf '%s\\n' {n} > /mnt/oath/objects/net/net0/actual.json",
            h = sh_quote(&host_json),
            n = sh_quote(net_json),
        ),
    )?;
    let keys = qemu::host_pubkeys_body().unwrap_or_default();
    let arr: Vec<String> = keys
        .lines()
        .filter(|l| l.starts_with("ssh-") || l.starts_with("ecdsa-"))
        .map(|l| format!("\"{}\"", l.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    let ssh_json = format!(r#"{{"authorized":[{}]}}"#, arr.join(","));
    ssh_run(
        r,
        &format!(
            "printf '%s\\n' {s} > /mnt/oath/objects/ssh/local/desired.json",
            s = sh_quote(&ssh_json)
        ),
    )?;
    // Metal has no virtio-gpu + /dev/input yet. River/Sola crash-loop
    // floods the console. Courage is SSH. QEMU rehearsal keeps them on.
    if !opts.qemu {
        ssh_run(
            r,
            r#"w=/mnt/oath/store/pkg/sola/bin/sola-river
[ -f "$w" ] && sed -i 's/^export SOLA_OUTPUT_PICK=preferred/# export SOLA_OUTPUT_PICK=preferred/' "$w"
true"#,
        )?;
    }
    let _ = out;
    Ok(())
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn install_remote(out: &Path, tools: &Tools, opts: &Opts, r: &Remote) -> Result<()> {
    if !already_installer(r) {
        enter_installer(out, tools, opts, r)?;
    } else {
        eprintln!("already in installer ramdisk");
    }
    let inst = Remote { user: "root".into(), host: r.host.clone(), port: 22, sudo: false };
    wait_ssh(&inst, Duration::from_secs(300))?;
    format_and_copy(out, tools, opts, &inst)?;
    eprintln!("disk written; reboot");
    let _ = ssh_run(&inst, "/bin/busybox reboot -f");
    wait_oath_ssh(&inst, Duration::from_secs(300))?;
    let ls = ssh_out(&inst, "/bin/oath ls")?;
    eprintln!("oath ls:\n{ls}");
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
    let inst = Remote { user: "root".into(), host: "127.0.0.1".into(), port, sudo: false };
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
    };
    if let Err(e) = format_and_copy(out, tools, &qopts, &inst) {
        let _ = child.kill();
        dump_serial(&serial);
        return Err(e);
    }
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
    let wait = wait_oath_ssh(&inst, Duration::from_secs(180));
    let ok = wait.is_ok();
    if ok {
        let ls = ssh_out(&inst, "oath ls")?;
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
        "2048",
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

fn wait_oath_ssh(r: &Remote, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    eprintln!("waiting for installed SSH at {}:{}", r.host, r.port);
    while Instant::now() < deadline {
        if ssh_out(r, "test -f /oath/INDEX.md && echo YES")
            .map(|s| s.contains("YES"))
            .unwrap_or(false)
        {
            return Ok(());
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
    fn parse_target_user_host() {
        let r = parse_target("joshua@canto").unwrap();
        assert_eq!(r.user, "joshua");
        assert_eq!(r.host, "canto");
        assert_eq!(r.port, 22);
        assert!(r.sudo);
    }
}
