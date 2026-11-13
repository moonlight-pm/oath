//! `cargo make image` (R86): an output as a disk image anyone can download
//! and write to a USB stick, which boots the release and installs it with
//! `oath install`. The installer ramdisk writes it in QEMU as `install
//! --local` writes a stick, with no SSH key and no host key, so every
//! machine that boots it makes its own. A copy boots once through OVMF
//! and its serial console (root's, `svc:serial`) must answer `oath ls`;
//! the image itself is never booted. Then it is compressed with xz,
//! hashed and signed (`oath-image`), in the output's directory. `--publish`
//! puts the three files on the default origin under `image/`.

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::install::{self, dump_serial, stop_qemu};
use crate::output::{self, Output};
use crate::tools::{self, Tools};
use crate::util::{copy_file, kvm, run, sha256_file};

/// What `ssh-keygen -Y verify -n` names to check an image's signature.
pub const NAMESPACE: &str = "oath-image";

/// The disk the image holds, on a stick of 16 GB or more. At 6 GiB the
/// `desk` system had 485 MB free: btrfs keeps two copies of the metadata
/// of its many small files.
const SIZE: u64 = 8 << 30;

pub struct Opts {
    pub output: Option<String>,
    pub publish: bool,
}

/// The image's three files, beside each other in the output's directory.
struct Files {
    xz: PathBuf,
    sha256: PathBuf,
    sig: PathBuf,
}

impl Files {
    fn of(o: &Output, profile: &str) -> Self {
        let xz = o.file(&format!("oath-{profile}-{}.img.xz", o.id));
        let with = |ext: &str| PathBuf::from(format!("{}.{ext}", xz.display()));
        Self { sha256: with("sha256"), sig: with("sig"), xz }
    }

    fn all(&self) -> [&Path; 3] {
        [&self.xz, &self.sha256, &self.sig]
    }
}

pub fn run_image(root: &Path, out: &Path, opts: Opts) -> Result<()> {
    let o = output::pick(out, opts.output.as_deref(), true)?;
    crate::check::require_checked(&o.dir)?;
    let profile = crate::check::output_profile(&o.dir)?;
    let files = Files::of(&o, &profile);
    // A machine installed from the image updates from its release, so the
    // release goes out first.
    let origin = if opts.publish {
        let origin = crate::origin::Origin::from_env()?;
        let release = oath_core::update::release_key(&o.id);
        if !origin.keys_under(&release)?.contains(&release) {
            bail!(
                "output {} is not published, so a machine installed from its image could not \
                 update: cargo make publish --output {} first",
                o.id,
                o.id
            );
        }
        Some(origin)
    } else {
        None
    };
    if files.all().iter().all(|f| f.is_file()) {
        println!("image {} is made", files.xz.display());
    } else {
        make(root, out, &o, &files)?;
    }
    let Some(origin) = origin else { return Ok(()) };
    let have = origin.keys_under("image/")?;
    for f in files.all() {
        let key = format!("image/{}", f.file_name().and_then(|n| n.to_str()).context("name")?);
        if have.contains(&key) {
            bail!("{key} is already on the origin: an image is published once");
        }
    }
    for f in files.all() {
        let key = format!("image/{}", f.file_name().and_then(|n| n.to_str()).context("name")?);
        origin.put(&key, f)?;
        println!("put {key}");
    }
    Ok(())
}

fn make(root: &Path, out: &Path, o: &Output, files: &Files) -> Result<()> {
    let tools = tools::load(root)?;
    tools.require_qemu()?;
    let run_dir = install::new_run_dir(out, "image")?;
    eprintln!("image of output {} in {}", o.id, run_dir.display());
    let p = install::payload(o, &run_dir)?;
    let img = run_dir.join("oath.img");
    fs::File::create(&img)?.set_len(SIZE)?;
    let opts = install::Opts {
        target: None,
        disk: "/dev/vda".into(),
        confirm: true,
        qemu: false,
        usb: false,
        local: false,
        hostname: None,
        overlay: None,
        unchecked: false,
        output: Some(o.id.clone()),
        oath_install: false,
        image: true,
    };
    let drive = format!("file={},if=none,id=hd0,format=raw", img.display());
    install::write_through_ramdisk(&run_dir, &tools, &p, &opts, &drive, crate::qemu::ssh_port())?;
    boot_check(&run_dir, &tools, &img)?;

    eprintln!("xz {}", files.xz.display());
    let part = run_dir.join("oath.img.xz");
    run(Command::new("xz")
        .args(["-T0", "-6", "-c"])
        .arg(&img)
        .stdout(Stdio::from(fs::File::create(&part)?)))?;
    fs::remove_file(&img)?;
    let name = files.xz.file_name().and_then(|n| n.to_str()).context("name")?;
    fs::write(&files.sha256, format!("{}  {name}\n", sha256_file(&part)?))?;
    fs::rename(&part, &files.xz)?;
    let sig = oath_core::origin::sign(&files.xz, NAMESPACE).map_err(|e| anyhow::anyhow!("{e}"))?;
    if sig != files.sig {
        bail!("ssh-keygen wrote {}, not {}", sig.display(), files.sig.display());
    }
    println!(
        "image {} ({:.2} GB), signed by {}",
        files.xz.display(),
        fs::metadata(&files.xz)?.len() as f64 / 1e9,
        oath_core::origin::public_key().map_err(|e| anyhow::anyhow!("{e}"))?
    );
    Ok(())
}

/// Boot a copy of `img` (a qcow2 over it) through OVMF, as a machine boots
/// the stick: the firmware finds `oath-efi` on the ESP, which boots the
/// kernel and the root the image names. Its serial console must answer
/// `oath ls`. The copy is thrown away.
fn boot_check(run_dir: &Path, tools: &Tools, img: &Path) -> Result<()> {
    let ovmf_code = tools.ovmf_code.as_ref().context("OVMF_CODE.fd missing")?;
    let ovmf_vars = tools.ovmf_vars.as_ref().context("OVMF_VARS.fd missing")?;
    let copy = run_dir.join("boot.qcow2");
    run(Command::new(&tools.qemu_img)
        .args(["create", "-q", "-f", "qcow2", "-F", "raw", "-b"])
        .arg(img)
        .arg(&copy))?;
    let vars = run_dir.join("OVMF_VARS.fd");
    copy_file(ovmf_vars, &vars)?;
    fs::set_permissions(&vars, fs::Permissions::from_mode(0o644))?;
    let sock = run_dir.join("serial.sock");
    let log = run_dir.join("serial-boot.log");
    let mut qemu = Command::new(&tools.qemu);
    if kvm() {
        qemu.arg("-enable-kvm");
    }
    qemu.args(["-machine", "q35", "-m", "4096", "-display", "none", "-monitor", "none"])
        .arg("-drive")
        .arg(format!("if=pflash,format=raw,readonly=on,file={}", ovmf_code.display()))
        .arg("-drive")
        .arg(format!("if=pflash,format=raw,file={}", vars.display()))
        .arg("-chardev")
        .arg(format!(
            "socket,id=s0,path={},server=on,wait=off,logfile={}",
            sock.display(),
            log.display()
        ))
        .args(["-serial", "chardev:s0"])
        .args(["-netdev", "user,id=n0", "-device", "virtio-net-pci,netdev=n0"])
        .args(["-device", "virtio-rng-pci"])
        .arg("-drive")
        .arg(format!("file={},if=none,id=hd0,format=qcow2", copy.display()))
        .args(["-device", "virtio-blk-pci,drive=hd0,bootindex=1", "-no-reboot"])
        .stdout(Stdio::null())
        .stderr(Stdio::from(fs::File::create(run_dir.join("qemu-boot.log"))?));
    eprintln!(">> OVMF boot of a copy of the image");
    let mut child = qemu.spawn().context("qemu")?;
    let answer = ask_serial(&sock, Duration::from_secs(240));
    stop_qemu(&mut child);
    let _ = fs::remove_file(&copy);
    match answer {
        Ok(text) => {
            if !text.contains("host:local") {
                dump_serial(&log);
                bail!("the image booted, but `oath ls` named no host:local");
            }
            eprintln!("the image boots: its serial console answered `oath ls`");
            Ok(())
        }
        Err(e) => {
            dump_serial(&log);
            Err(e)
        }
    }
}

/// Wait for the serial console's shell, run `oath ls` in it, and return
/// what it printed.
fn ask_serial(sock: &Path, timeout: Duration) -> Result<String> {
    let deadline = Instant::now() + timeout;
    let mut s = loop {
        match UnixStream::connect(sock) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(200)),
            Err(e) => bail!("QEMU's serial socket {}: {e}", sock.display()),
        }
    };
    s.set_read_timeout(Some(Duration::from_secs(1)))?;
    let mut seen = String::new();
    let mut asked: Option<usize> = None;
    let mut buf = [0u8; 4096];
    while Instant::now() < deadline {
        match s.read(&mut buf) {
            Ok(0) => bail!("QEMU closed the serial console"),
            Ok(n) => seen.push_str(&String::from_utf8_lossy(&buf[..n])),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(e) => return Err(e.into()),
        }
        match asked {
            None if seen.contains(SHELL_UP) => {
                s.write_all(b"oath ls; echo \"image-check $?\"\n")?;
                asked = Some(seen.len());
            }
            Some(from) => match status(&seen[from..]) {
                Some(0) => return Ok(seen[from..].to_string()),
                Some(rc) => bail!("`oath ls` on the image's serial console exited {rc}"),
                None => {}
            },
            None => {}
        }
    }
    bail!(
        "timed out: {}",
        if asked.is_some() { "`oath ls` did not finish" } else { "no shell on the serial console" }
    )
}

/// What `serial-login` prints when its root shell starts.
const SHELL_UP: &str = "Root on serial";

/// The exit status after `image-check `, once the shell printed it (the
/// echoed command reads `image-check $?`).
fn status(text: &str) -> Option<i32> {
    text.match_indices("image-check ").find_map(|(i, m)| {
        let rest = &text[i + m.len()..];
        let n = rest.find(|c: char| !c.is_ascii_digit())?;
        if n == 0 || !rest[n..].starts_with(['\r', '\n']) {
            return None;
        }
        rest[..n].parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_waits_for_the_shell_not_the_echo() {
        assert_eq!(status("oath ls; echo \"image-check $?\"\r\n"), None);
        let answer = "oath ls; echo \"image-check $?\"\r\nhost:local\r\nimage-check 0\r\n";
        assert_eq!(status(answer), Some(0));
        assert_eq!(status("image-check 1\r\n/root # "), Some(1));
        assert_eq!(status("image-check 12"), None, "not the whole number yet");
    }

    #[test]
    fn an_images_files_sit_in_its_output() {
        let o = Output { id: "42a8028ae7bf".into(), dir: PathBuf::from("/b/out/42a8028ae7bf") };
        let f = Files::of(&o, "desk");
        assert_eq!(f.xz, Path::new("/b/out/42a8028ae7bf/oath-desk-42a8028ae7bf.img.xz"));
        assert_eq!(f.sha256, Path::new("/b/out/42a8028ae7bf/oath-desk-42a8028ae7bf.img.xz.sha256"));
        assert_eq!(f.sig, Path::new("/b/out/42a8028ae7bf/oath-desk-42a8028ae7bf.img.xz.sig"));
    }
}
