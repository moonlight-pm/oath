//! `cargo make install --qemu --oath-install`: rehearse `oath install`
//! (R43) through OVMF, as a person does it from a stick.
//!
//! 1. Install the output onto a stick (a qcow2), as `--qemu` does.
//! 2. Boot the stick beside a blank target disk and another disk (btrfs
//!    "storage") whose bytes must not change, with no `bootindex`, so the
//!    firmware follows its own boot list as metal does. Add a stand-in for
//!    another system's entry (`Linux Boot Manager`, loading a file from a
//!    disk).
//! 3. `oath install` lists the disks; `oath install <serial>` previews (it
//!    must offer to remove the stand-in); it refuses the stick it runs
//!    from; `--confirm` installs. Power off.
//! 4. The other disk's sha256 is what it was.
//! 5. Boot again with the same firmware variables: the firmware must start
//!    the target first (its new host key answers, its root UUID is on the
//!    command line), with `@home` at /home, generation 0, the catalog in
//!    sync, `authorized_keys` written, the stand-in entry gone, and
//!    `/oath/output.json` naming the output (R45).

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use oath_core::efi::{self, LoadOption};

use crate::install::{
    dump_serial, install_in_qemu, pin_host_key, power_off, ssh_out, stop_qemu, wait_oath_ssh, Opts,
    Payload,
};
use crate::qemu;
use crate::tools::Tools;
use crate::util::{kvm, new_uuid, run, sha256_file};

const TARGET_SERIAL: &str = "OATHTARGET01";
const OTHER_SERIAL: &str = "OATHOTHER01";
const STAND_IN: &str = "Linux Boot Manager";

pub(crate) fn rehearse(run_dir: &Path, tools: &Tools, p: &Payload, opts: &Opts) -> Result<()> {
    tools.require_qemu()?;
    // 1. The stick.
    let stick = run_dir.join("disk.qcow2");
    run(Command::new(&tools.qemu_img)
        .args(["create", "-q", "-f", "qcow2"])
        .arg(&stick)
        .arg("16G"))?;
    let drive = format!("file={},if=none,id=hd0,format=qcow2", stick.display());
    install_in_qemu(run_dir, tools, p, opts, &drive, "/dev/vda", false)?;

    // 2. The target and the other disk.
    let target = run_dir.join("target.img");
    fs::File::create(&target)?.set_len(16 << 30)?;
    let other = run_dir.join("other.img");
    {
        let mut f = fs::File::create(&other)?;
        let mut rnd = fs::File::open("/dev/urandom")?;
        std::io::copy(&mut Read::by_ref(&mut rnd).take(256 << 20), &mut f)?;
    }
    run(Command::new("mkfs.btrfs").args(["-q", "-f", "-L", "storage"]).arg(&other))?;
    let other_sum = sha256_file(&other)?;
    let drives = [
        (stick.clone(), "qcow2", None),
        (target.clone(), "raw", Some(TARGET_SERIAL)),
        (other.clone(), "raw", Some(OTHER_SERIAL)),
    ];
    let port = qemu::ssh_port();
    let pins = run_dir.join("known_hosts");
    let mut vm = ovmf_boot(tools, run_dir, "stick", &drives)?;
    let live = match wait_oath_ssh("127.0.0.1", port, &pins, Duration::from_secs(180)) {
        Ok(l) => l,
        Err(e) => return fail(&mut vm, run_dir, "stick", e),
    };
    let result = (|| -> Result<(String, String)> {
        let entry = LoadOption {
            attributes: efi::LOAD_OPTION_ACTIVE,
            description: STAND_IN.into(),
            path: efi::disk_file_path(
                1,
                2048,
                1 << 20,
                &new_uuid()?,
                r"\EFI\systemd\systemd-bootx64.efi",
            )
            .context("stand-in path")?,
            optional: vec![],
        };
        let mut bytes = efi::BOOT_ATTRS.to_le_bytes().to_vec();
        bytes.extend(entry.encode());
        let esc: String = bytes.iter().map(|b| format!("\\x{b:02x}")).collect();
        let var = format!("{}/{}", efi::EFIVARS, efi::var_file("Boot0042"));
        ssh_out(
            &live,
            &format!(
                "sudo -n mount -t efivarfs efivarfs {dir} && printf '{esc}' | sudo -n dd \
                 of={var} bs=4096 iflag=fullblock conv=notrunc status=none; s=$?; \
                 sudo -n umount {dir}; exit $s",
                dir = efi::EFIVARS,
            ),
        )
        .context("write the stand-in boot entry")?;

        // 3. oath install.
        let list = ssh_out(&live, "oath install 2>&1")?;
        eprintln!("oath install:\n{list}");
        for want in [TARGET_SERIAL, OTHER_SERIAL, "this system runs from it"] {
            if !list.contains(want) {
                bail!("`oath install` does not show {want:?}");
            }
        }
        let preview = ssh_out(&live, &format!("sudo -n oath install {TARGET_SERIAL} 2>&1"))?;
        eprintln!("oath install {TARGET_SERIAL}:\n{preview}");
        for want in ["install on /dev/vdb", "adds", &format!("removes  Boot0042 {STAND_IN}")] {
            if !preview.contains(want) {
                bail!("the preview does not say {want:?}");
            }
        }
        let refused = ssh_out(&live, "sudo -n oath install /dev/vda --confirm 2>&1; echo exit=$?")?;
        if !refused.contains("holds the system this machine runs") || refused.ends_with("exit=0") {
            bail!("`oath install` did not refuse the disk it runs from:\n{refused}");
        }
        let out = ssh_out(&live, &format!("sudo -n oath install {TARGET_SERIAL} --confirm 2>&1"))?;
        eprintln!("oath install --confirm:\n{out}");
        let uuid = out
            .split("Its root is UUID=")
            .nth(1)
            .and_then(|s| s.split(';').next())
            .context("the install printed no root UUID")?
            .to_string();
        let key = out
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("ssh-ed25519 "))
            .context("the install printed no host key")?
            .to_string();
        let after = ssh_out(&live, &format!("sudo -n oath install {OTHER_SERIAL} 2>&1"))?;
        let ours = after
            .lines()
            .any(|l| l.trim_start().starts_with("removes") && l.trim_end().ends_with(" Oath"));
        if after.contains(STAND_IN) || !ours {
            bail!("after the install the firmware's entries are not Oath's:\n{after}");
        }
        Ok((uuid, key))
    })();
    let (uuid, key) = match result {
        Ok(v) => v,
        Err(e) => return fail(&mut vm, run_dir, "stick", e),
    };
    power_off(&live, &mut vm);
    stop_qemu(&mut vm);

    // 4. The other disk.
    if sha256_file(&other)? != other_sum {
        bail!("{} changed", other.display());
    }
    eprintln!("the other disk is unchanged");

    // 5. The firmware's first entry.
    let pins = run_dir.join("known_hosts-target");
    pin_host_key(&pins, "127.0.0.1", port, &key)?;
    let mut vm = ovmf_boot(tools, run_dir, "target", &drives)?;
    let live = match wait_oath_ssh("127.0.0.1", port, &pins, Duration::from_secs(180)) {
        Ok(l) => l,
        Err(e) => return fail(&mut vm, run_dir, "target", e),
    };
    let checks = (|| -> Result<()> {
        let cmdline = ssh_out(&live, "cat /proc/cmdline")?;
        if !cmdline.contains(&format!("oath.root=UUID={uuid}")) {
            bail!("booted {cmdline:?}, not the target's root {uuid}");
        }
        let home = ssh_out(&live, "grep ' /home ' /proc/mounts")?;
        if !home.contains("subvol=/@home") {
            bail!("/home is not @home: {home:?}");
        }
        let diff = ssh_out(&live, "oath diff 2>/dev/null | tail -1")?;
        if diff != "in-sync" {
            bail!("the installed catalog is not in sync: {diff}");
        }
        let gen = ssh_out(&live, "cat /oath/objects/snap/current/applied.json")?;
        if !gen.contains("\"generation\": 0") {
            bail!("the install is not at generation 0: {gen}");
        }
        ssh_out(&live, "test -s /home/.ssh/authorized_keys && test ! -e /oath/log/init.log.1")?;
        // R45: the install's record, so `oath update` orders against it.
        let record = ssh_out(&live, "cat /oath/output.json")?;
        if !record.contains(&format!("\"output\": \"{}\"", p.output_id())) {
            bail!("the install's /oath/output.json does not name its output: {record}");
        }
        let svcs = ssh_out(&live, r#"cd /oath/status/svc && grep -o '"state": *"[a-z]*"' *.json"#)?;
        eprintln!("services on the target:\n{svcs}");
        Ok(())
    })();
    if let Err(e) = checks {
        return fail(&mut vm, run_dir, "target", e);
    }
    power_off(&live, &mut vm);
    stop_qemu(&mut vm);
    eprintln!("oath install rehearsal ok  {}", run_dir.display());
    Ok(())
}

fn fail(vm: &mut Child, run_dir: &Path, label: &str, e: anyhow::Error) -> Result<()> {
    stop_qemu(vm);
    dump_serial(&run_dir.join(format!("serial-{label}.log")));
    Err(e.context(format!("oath install rehearsal ({label} boot), {}", run_dir.display())))
}

/// OVMF with the run's firmware variables and `drives` as virtio disks in
/// order (/dev/vda, vdb, …), each with its serial. No `bootindex`: OVMF
/// would rewrite BootOrder from it at every boot, which no machine does.
fn ovmf_boot(
    tools: &Tools,
    run_dir: &Path,
    label: &str,
    drives: &[(PathBuf, &str, Option<&str>)],
) -> Result<Child> {
    let code = tools.ovmf_code.as_ref().context("OVMF_CODE.fd missing")?;
    let vars = run_dir.join("OVMF_VARS.fd");
    let serial = run_dir.join(format!("serial-{label}.log"));
    let mut c = Command::new(&tools.qemu);
    if kvm() {
        c.arg("-enable-kvm");
    }
    c.args([
        "-machine",
        "q35",
        "-smp",
        "4",
        "-m",
        "4096",
        "-display",
        "none",
        "-monitor",
        "none",
        "-serial",
        &format!("file:{}", serial.display()),
        "-netdev",
        &format!("user,id=n0,hostfwd=tcp:127.0.0.1:{}-:22", qemu::ssh_port()),
        "-device",
        "virtio-net-pci,netdev=n0",
        "-device",
        "virtio-rng-pci",
        "-drive",
        &format!("if=pflash,format=raw,readonly=on,file={}", code.display()),
        "-drive",
        &format!("if=pflash,format=raw,file={}", vars.display()),
        "-no-reboot",
    ]);
    for (i, (path, format, serial)) in drives.iter().enumerate() {
        c.args([
            "-drive",
            &format!("file={},if=none,id=hd{i},format={format},cache=writeback", path.display()),
        ]);
        let mut dev = format!("virtio-blk-pci,drive=hd{i}");
        if let Some(s) = serial {
            dev.push_str(&format!(",serial={s}"));
        }
        c.args(["-device", &dev]);
    }
    let log = fs::File::create(run_dir.join(format!("qemu-{label}.log")))?;
    c.stdout(Stdio::null()).stderr(Stdio::from(log));
    eprintln!(">> OVMF boot ({label}) {}", run_dir.display());
    let mut child = c.spawn().context("qemu")?;
    std::thread::sleep(Duration::from_millis(400));
    if let Ok(Some(st)) = child.try_wait() {
        let mut err = String::new();
        let _ = std::io::Read::read_to_string(
            &mut fs::File::open(run_dir.join(format!("qemu-{label}.log")))?,
            &mut err,
        );
        bail!("qemu exited at once ({st}): {err}");
    }
    let _ = std::io::stderr().flush();
    Ok(child)
}
