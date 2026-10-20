//! `cargo make update`: move an installed machine to a build output
//! without wiping it, from the build host, over SSH as `home` (sudo), with
//! the host key `install` pinned.
//!
//! 1. Refuse unless the target is Oath, not this machine, in sync and not
//!    on probation, and the output passed `cargo make check`.
//! 2. Mount its ESP, plan the boot rotation (as `cargo make esp`), and
//!    snapshot `@` as `@boot-<id>` before anything changes: the archived
//!    boot is the old kernel with the old userspace.
//! 3. Ship the output's store packs the machine lacks (`.part`, renamed
//!    into place).
//! 4. Set every package the machine has to the output's pin and needs
//!    (and a recipe's plan, `url` and `file`, R23), make each package the
//!    output fixes that the machine lacks (`/etc/passwd` names `pkg:zsh`'s
//!    shell), and `oath apply` them: one generation, undone by `oath undo`
//!    (which keeps a fixed package), and apply re-hashes each tree before
//!    linking it. PID 1 restarts each service whose program moved; when
//!    sshd's did, the apply is on probation and a new SSH login keeps it.
//!    A recipe the owner built is reported instead.
//! 5. Replace the image's own files outside the catalog (`/lib/oath`, the
//!    static `/etc` files, `/sbin/init`, `/bin/sudo`).
//! 6. Archive the ESP's kernel and initrd as boot `<id>`, write the new
//!    kernel, initrd and oath-efi, keep five.
//! 7. With `--reboot`: reboot through PID 1, wait for a new boot, and
//!    require the output's PID 1 and an in-sync catalog.
//!
//! `--qemu <output>` rehearses it: `install --qemu` of that older output,
//! boot it through OVMF, update it to the new one, reboot, check.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::boot;
use crate::install::{self, Remote};
use crate::output::{self, Output};
use crate::tools::{self, Tools};
use crate::util::{copy_file, kvm, sha256_file};

pub struct Opts {
    pub target: Option<String>,
    /// Rehearse in QEMU from this (older) output.
    pub qemu: Option<String>,
    pub output: Option<String>,
    pub unchecked: bool,
    pub confirm: bool,
    pub reboot: bool,
    /// The target's ESP node (default: partition 1 of the root disk).
    pub esp: Option<String>,
}

/// The image's files outside `/oath` that an update replaces. The rest of
/// the root tree is the catalog's (`/bin` links, `/etc/hosts`) or the
/// machine's own.
const ROOT_FILES: &[&str] = &[
    "lib/oath",
    "etc/passwd",
    "etc/group",
    "etc/shadow",
    "etc/shells",
    "etc/nsswitch.conf",
    "etc/ssh/ssh_config",
    "sbin/init",
    "bin/sudo",
];
/// Where an update stages its files on the target (inside `@`).
const WORK: &str = "/oath/run/update";
const ESP_MNT: &str = "/oath/run/update-esp";

pub fn run_update(root: &Path, out: &Path, opts: Opts) -> Result<()> {
    if !opts.confirm {
        bail!("refusing to change a machine's boot files without --confirm");
    }
    let o = match &opts.output {
        Some(id) => output::open(out, id)?,
        None => output::pick(out, None, !opts.unchecked)
            .context("pass --unchecked to update to the latest output anyway (Joshua's call)")?,
    };
    if !opts.unchecked {
        crate::check::require_checked(&o.dir)
            .context("pass --unchecked to update to it anyway (Joshua's call)")?;
    }
    eprintln!("output {}", o.dir.display());
    if let Some(from) = &opts.qemu {
        return rehearse(root, out, &o, from);
    }
    let target = opts.target.as_deref().context("--target home@host (or --qemu <output>)")?;
    let mut r = install::parse_target(target)?;
    r.known_hosts = Some(install::known_hosts(out));
    update(&r, &o, opts.esp.as_deref(), false)?;
    if opts.reboot {
        reboot_and_verify(&r, &o, None)?;
    } else {
        eprintln!("updated; reboot to run the new kernel, initrd and PID 1");
    }
    Ok(())
}

/// Steps 1–6 on a reachable machine.
fn update(r: &Remote, o: &Output, esp: Option<&str>, qemu: bool) -> Result<()> {
    let stage = o.file("stage");
    let manifest: Value = serde_json::from_slice(&fs::read(o.file("manifest.json"))?)?;
    crate::pinned::warn_stale(&manifest);
    let pinned = crate::pinned::in_manifest(&manifest);
    preflight(r)?;
    let root_dev = cmdline_val(&install::ssh_out(r, "cat /proc/cmdline")?, "oath.root")
        .context("the running kernel has no oath.root= (not booted from an Oath ESP)")?;
    let esp = match esp {
        Some(e) => e.to_string(),
        None => esp_of(&root_dev).with_context(|| format!("no ESP for {root_dev}: pass --esp"))?,
    };

    // 2. The boot rotation, planned before anything changes.
    install::ssh_run(
        r,
        &format!("set -e\nmkdir -p {ESP_MNT} {WORK}\nmountpoint -q {ESP_MNT} || mount -t vfat {esp} {ESP_MNT}\n"),
    )?;
    let result = (|| -> Result<()> {
        let listing = install::ssh_out(
            r,
            &format!("ls {ESP_MNT}/loader/entries {ESP_MNT}/oath/boot 2>/dev/null; true"),
        )?;
        let ids = archive_ids(&listing);
        let plan = boot::plan_rotate(&ids);
        let current = install::ssh_out(
            r,
            &format!("cat {ESP_MNT}/loader/entries/oath.conf 2>/dev/null; true"),
        )?;
        let extra = extra_options(&boot::parse_bls(&current).options);
        let snap = oath_core::boot_subvol_name(plan.new_id);
        eprintln!(">> snapshot @ as {snap} (boot {}'s userspace)", plan.new_id);
        install::ssh_run(
            r,
            &format!(
                "set -e\ntest -e /oath/run/fs/{snap} || btrfs subvolume snapshot /oath/run/fs/@ /oath/run/fs/{snap} >/dev/null\n"
            ),
        )?;

        ship_packs(r, &stage)?;
        set_pins(r, &stage, &pinned)?;
        replace_root_files(r, &stage)?;
        write_esp(r, o, &plan, &root_dev, &extra, qemu)?;
        Ok(())
    })();
    let _ =
        install::ssh_run(r, &format!("sync; umount {ESP_MNT} 2>/dev/null; rm -rf {WORK}; true"));
    result
}

/// Oath, not this machine, in sync, and nothing on probation.
fn preflight(r: &Remote) -> Result<()> {
    let is_oath = install::ssh_out(r, "test -f /oath/INDEX.md && echo OATH; true")?;
    if !is_oath.contains("OATH") {
        bail!("{} is not an Oath machine (no /oath/INDEX.md); install it instead", r.host);
    }
    install::refuse_build_host(r)?;
    let diff = install::ssh_out(r, "oath diff")?;
    if diff.trim() != "in-sync" {
        bail!("{} has pending changes; apply or undo them first:\n{diff}", r.host);
    }
    if install::ssh_out(r, "test -f /oath/probation.json && echo OPEN; true")?.contains("OPEN") {
        bail!("{} has an apply on probation; keep or undo it first", r.host);
    }
    Ok(())
}

/// 3. Every store pack in the output that the machine lacks, as a tar
/// stream into `<slot>/.<hash>.part`, renamed into place whole.
fn ship_packs(r: &Remote, stage: &Path) -> Result<()> {
    let have = install::ssh_out(r, "ls -d /oath/store/pkg/*/sha256-* 2>/dev/null; true")?;
    let have: Vec<&str> = have.lines().map(str::trim).collect();
    for (name, hash) in stage_packs(stage)? {
        let dest = format!("/oath/store/pkg/{name}/{hash}");
        if have.contains(&dest.as_str()) {
            continue;
        }
        eprintln!(">> pkg:{name} {hash}");
        let part = format!("/oath/store/pkg/{name}/.{hash}.part");
        let script = format!(
            "set -e\nrm -rf {part}\nmkdir -p {part}\ntar -x -f - -C {part}\nmv {part}/{hash} {dest}\nrmdir {part}\n"
        );
        let mut tar = Command::new("tar")
            .args(["-C"])
            .arg(stage.join("oath/store/pkg").join(&name))
            .args(["--owner=0", "--group=0", "--numeric-owner", "-cf", "-", &hash])
            .stdout(Stdio::piped())
            .spawn()
            .context("tar")?;
        let mut ssh = install::ssh_base(r);
        let run = format!("sh -c {}", sh_quote(&script));
        ssh.arg(if r.sudo { format!("sudo -n {run}") } else { run });
        ssh.stdin(tar.stdout.take().context("tar stdout")?);
        let st = ssh.status().context("ssh")?;
        let tst = tar.wait()?;
        if !st.success() || !tst.success() {
            bail!("shipping pkg:{name} {hash} failed (ssh {st}, tar {tst})");
        }
    }
    Ok(())
}

/// 4. The output's pins for what the machine has, applied in one
/// generation: each package's `hash` and `needs` (a package whose own
/// tree is unchanged still moves its `needs` when what it runs with was
/// rebuilt), a recipe's `url` and `file`, and its plan (R23). A recipe
/// the owner built stays at the pin it runs, since the output carries no
/// tree for it; it is reported, to rebuild on the machine. A package
/// pinned from elsewhere (`pinned`, R28) keeps the machine's own pin, its
/// owner's to move, and only its needs follow the output, except a need
/// on another such pack the machine keeps, which stays at that pin. A
/// package the output fixes is made when the machine lacks it, since the
/// image's own files need it; other objects the output's seed has and the
/// machine does not are reported, not created: the catalog is the owner's.
fn set_pins(r: &Remote, stage: &Path, pinned: &std::collections::BTreeSet<String>) -> Result<()> {
    let ids = install::ssh_out(r, "oath ls")?;
    let have: Vec<&str> = ids.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let mut missing: Vec<String> =
        stage_ids(stage)?.into_iter().filter(|id| !have.contains(&id.as_str())).collect();
    set_plans(r, stage, &have)?;
    let mut changed = Vec::new();
    let mut rebuild = Vec::new();
    let pins = stage_pins(stage)?;
    let desired = |id: &str| -> Result<Value> {
        let cur = install::ssh_out(r, &format!("oath --json get {id} --desired"))?;
        serde_json::from_str(&cur).with_context(|| format!("{id}: {cur}"))
    };
    // The machine's own pins of the packs pinned from elsewhere: one it
    // keeps is what its other such packs need of it.
    let mut kept = std::collections::BTreeMap::new();
    for (name, pin, _) in pins.iter().filter(|(n, _, _)| pinned.contains(n)) {
        if have.contains(&format!("pkg:{name}").as_str()) {
            let mine = desired(&format!("pkg:{name}"))?["hash"].as_str().unwrap_or("").to_string();
            if mine != *pin {
                kept.insert(name.clone(), mine);
            }
        }
    }
    let ships = crate::pinned::needed_at(&kept, |n: &str| {
        pins.iter().find(|(p, _, _)| p == n).map(|(_, h, _)| h.clone())
    });
    for (name, pin, doc) in pins.iter().cloned() {
        let id = format!("pkg:{name}");
        if !have.contains(&id.as_str()) {
            if stage_fixed(stage, &name)? {
                install::ssh_run(
                    r,
                    &format!("oath new {id} --from-json {}", sh_quote(&doc.to_string())),
                )?;
                eprintln!(">> {id} {pin} (the output fixes it)");
                missing.retain(|m| *m != id);
                changed.push(id);
            }
            continue;
        }
        let cur = desired(&id)?;
        let mine = cur.get("hash").and_then(Value::as_str).unwrap_or("").to_string();
        if pinned.contains(&name) && mine != pin {
            eprintln!(
                ">> {id} keeps the machine's pin {mine} (built elsewhere; the output pins {pin})"
            );
            if let Some((set, moved)) = crate::pinned::keep_machine_pin(&cur, &ships) {
                eprintln!(
                    "   its needs move with the output ({}); its owner should build it again \
                     against them",
                    moved.join(", ")
                );
                install::ssh_run(
                    r,
                    &format!("oath set {id} --from-json {}", sh_quote(&set.to_string())),
                )?;
                changed.push(id);
            }
            continue;
        }
        let mut set = serde_json::json!({
            "hash": pin,
            "needs": doc.get("needs").cloned().unwrap_or_else(|| Value::Array(vec![])),
        });
        for k in ["url", "file"] {
            if let Some(v) = doc.get(k) {
                set[k] = v.clone();
            }
        }
        let none = Value::Array(vec![]);
        let differs = set.as_object().into_iter().flatten().any(|(k, v)| {
            cur.get(k).unwrap_or(if k == "needs" { &none } else { &Value::Null }) != v
        });
        if !differs {
            continue;
        }
        let shipped = stage.join("oath/store/pkg").join(&name).join(&pin).is_dir();
        if !shipped && cur.get("present") == Some(&Value::Bool(true)) {
            rebuild.push(format!(
                "{id}: the output's recipe builds {pin}; on the machine run \
                 `sudo oath build plan:{name}`, `sudo oath set {id} hash={pin}` and \
                 `sudo oath apply {id}`"
            ));
            continue;
        }
        install::ssh_run(r, &format!("oath set {id} --from-json {}", sh_quote(&set.to_string())))?;
        changed.push(id);
    }
    if !missing.is_empty() {
        eprintln!(
            "in the output's seed, not on the machine, left to its owner (oath new): {}",
            missing.join(" ")
        );
    }
    for line in &rebuild {
        eprintln!("{line}");
    }
    if changed.is_empty() {
        eprintln!("every package is already at the output's pin");
        return Ok(());
    }
    // `--confirm` is the update's own: a package that moves sshd's program
    // restarts sshd, so its apply goes on probation, and a new login keeps it.
    let applied = install::ssh_out(r, &format!("oath apply --confirm {}", changed.join(" ")))?;
    eprintln!("{applied}");
    if applied.contains("on probation") {
        eprintln!("{}", install::ssh_out(r, "oath keep")?);
    }
    Ok(())
}

/// A recipe's plan (R23), filed again on the machine when the output's
/// pin differs: the plan file goes over SSH, `oath set --from-file` lints
/// and pins it.
fn set_plans(r: &Remote, stage: &Path, have: &[&str]) -> Result<()> {
    let Ok(dir) = fs::read_dir(stage.join("oath/objects/plan")) else { return Ok(()) };
    for e in dir {
        let name = e?.file_name().to_string_lossy().into_owned();
        let id = format!("plan:{name}");
        if !have.contains(&id.as_str()) {
            continue;
        }
        let doc = stage.join("oath/objects/plan").join(&name).join("desired.json");
        let v: Value = oath_core::read_json(&doc).map_err(|err| anyhow::anyhow!("{err}"))?;
        let Some(pin) = v.get("hash").and_then(Value::as_str).filter(|h| !h.is_empty()) else {
            continue;
        };
        let cur = install::ssh_out(r, &format!("oath --json get {id} --desired"))?;
        let cur: Value = serde_json::from_str(&cur).with_context(|| format!("{id}: {cur}"))?;
        if cur.get("hash").and_then(Value::as_str) == Some(pin) {
            continue;
        }
        let path = stage.join("oath/store/plan").join(&name).join(pin).join("plan.plan");
        let text = fs::read_to_string(&path).with_context(|| path.display().to_string())?;
        if !text.ends_with('\n') || text.lines().any(|l| l == "OATH_PLAN_EOF") {
            bail!("{}: cannot send it as a here-document", path.display());
        }
        let f = format!("{WORK}/{name}.plan");
        install::ssh_run(
            r,
            &format!(
                "set -e\nmkdir -p {WORK}\ncat > {f} <<'OATH_PLAN_EOF'\n{text}OATH_PLAN_EOF\n\
                 oath set {id} --from-file {f}\nrm -f {f}\n"
            ),
        )?;
        eprintln!(">> {id} {pin}");
    }
    Ok(())
}

/// 5. `/lib/oath`, the static `/etc` files, `/sbin/init`, `/bin/sudo`:
/// each written beside itself and renamed over, so a running program
/// keeps its old file and nothing is ever half there.
fn replace_root_files(r: &Remote, stage: &Path) -> Result<()> {
    eprintln!(">> /lib/oath and the image's /etc files");
    let mut tar = Command::new("tar")
        .arg("-C")
        .arg(stage)
        .args(["--owner=0", "--group=0", "--numeric-owner", "-cf", "-"])
        .args(ROOT_FILES)
        .stdout(Stdio::piped())
        .spawn()
        .context("tar")?;
    let script = format!(
        "set -e\nrm -rf {WORK}/root\nmkdir -p {WORK}/root\ntar -x -f - -C {WORK}/root\n\
         cd {WORK}/root\n\
         find . ! -type d | while read -r f; do\n\
         \tt=/${{f#./}}\n\
         \tmkdir -p \"$(dirname \"$t\")\"\n\
         \tcp -a \"$f\" \"$t.oath-new\"\n\
         \tmv -f \"$t.oath-new\" \"$t\"\n\
         done\n\
         chown 0:0 /lib/oath/sudo\nchmod 4755 /lib/oath/sudo\n"
    );
    let mut ssh = install::ssh_base(r);
    let run = format!("sh -c {}", sh_quote(&script));
    ssh.arg(if r.sudo { format!("sudo -n {run}") } else { run });
    ssh.stdin(tar.stdout.take().context("tar stdout")?);
    let st = ssh.status().context("ssh")?;
    let tst = tar.wait()?;
    if !st.success() || !tst.success() {
        bail!("replacing the root files failed (ssh {st}, tar {tst})");
    }
    Ok(())
}

/// 6. The ESP, as `boot::apply_rotate_files` writes a local one: archive
/// the current kernel and initrd as boot `<id>` (whose userspace is the
/// snapshot step 2 took), drop the oldest beyond five, write the new
/// files and entries.
fn write_esp(
    r: &Remote,
    o: &Output,
    plan: &boot::RotatePlan,
    root_dev: &str,
    extra: &str,
    qemu: bool,
) -> Result<()> {
    eprintln!(">> ESP: archive boot {}, write the new kernel, initrd and oath-efi", plan.new_id);
    for (src, name) in [
        (o.file("bzImage"), "vmlinuz"),
        (o.file("initrd.gz"), "initrd.gz"),
        (o.file("BOOTX64.EFI"), "BOOTX64.EFI"),
    ] {
        install::scp_root(r, &src, &format!("{WORK}/{name}"))?;
    }
    let id = plan.new_id;
    let m = ESP_MNT;
    let mut s = String::from("set -e\n");
    s.push_str(&format!(
        "if [ -f {m}/vmlinuz ] && [ -f {m}/initrd.gz ]; then\n\
         \tmkdir -p {m}/oath/boot/{id} {m}/loader/entries\n\
         \tcp {m}/vmlinuz {m}/oath/boot/{id}/vmlinuz\n\
         \tcp {m}/initrd.gz {m}/oath/boot/{id}/initrd.gz\n\
         \tcat > {m}/loader/entries/{conf} <<'OATH_EOF'\n{bls}OATH_EOF\n\
         fi\n",
        conf = boot::archive_conf_name(id),
        bls = boot::archive_bls(id, root_dev, extra),
    ));
    for old in &plan.prune {
        s.push_str(&format!(
            "rm -rf {m}/oath/boot/{old}\nrm -f {m}/loader/entries/oath-{old}.conf {m}/loader/entries/OATH-{old}.CON\n\
             if [ -e /oath/run/fs/{sub} ]; then btrfs subvolume delete /oath/run/fs/{sub} >/dev/null; fi\n",
            sub = oath_core::boot_subvol_name(*old),
        ));
    }
    let loader = if qemu { boot::LOADER_CONF_QEMU } else { boot::LOADER_CONF_METAL };
    s.push_str(&format!(
        "mkdir -p {m}/EFI/BOOT {m}/loader/entries\n\
         cp {WORK}/vmlinuz {m}/vmlinuz\ncp {WORK}/initrd.gz {m}/initrd.gz\n\
         cp {WORK}/BOOTX64.EFI {m}/EFI/BOOT/BOOTX64.EFI\n\
         cat > {m}/loader/entries/oath.conf <<'OATH_EOF'\n{cur}OATH_EOF\n\
         cat > {m}/loader/loader.conf <<'OATH_EOF'\n{loader}OATH_EOF\n\
         cat > {m}/loader/oath-boots <<'OATH_EOF'\n{boots}OATH_EOF\n\
         sync\n",
        cur = boot::current_bls(root_dev, extra),
        boots = boot::format_oath_boots(&plan.boots),
    ));
    install::ssh_run(r, &s)
}

/// 7. Reboot through PID 1 (no generation), wait for another boot, and
/// require the output's PID 1 and nothing pending.
fn reboot_and_verify(r: &Remote, o: &Output, vm: Option<&mut Vm>) -> Result<()> {
    let before = install::ssh_out(r, "cat /proc/sys/kernel/random/boot_id")?;
    eprintln!(">> reboot");
    install::ssh_fire(r, "reboot");
    if let Some(vm) = vm {
        vm.wait_exit(Duration::from_secs(120))?;
        vm.boot()?;
    }
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if Instant::now() > deadline {
            bail!("{} did not come back from the reboot", r.host);
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
    eprintln!("{} runs output {}: PID 1 {}, catalog in sync", r.host, o.id, &want[..12]);
    Ok(())
}

/// `(name, hash)` of every realization in the output's store.
fn stage_packs(stage: &Path) -> Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    let dir = stage.join("oath/store/pkg");
    for slot in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let slot = slot?;
        let name = slot.file_name().to_string_lossy().into_owned();
        for e in fs::read_dir(slot.path())? {
            let e = e?;
            let hash = e.file_name().to_string_lossy().into_owned();
            if oath_core::is_realization_id(&hash) && e.path().is_dir() {
                out.push((name.clone(), hash));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Every object the output's seed made, generations and devices aside.
fn stage_ids(stage: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let dir = stage.join("oath/objects");
    for kind in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let kind = kind?;
        let k = kind.file_name().to_string_lossy().into_owned();
        if !kind.path().is_dir() || k == "snap" || k == "dev" {
            continue;
        }
        for e in fs::read_dir(kind.path())? {
            let e = e?;
            if e.path().join("desired.json").is_file() {
                out.push(format!("{k}:{}", e.file_name().to_string_lossy()));
            }
        }
    }
    out.sort();
    Ok(out)
}

/// `(name, pin, desired doc)` of every package the output's seed pins.
fn stage_pins(stage: &Path) -> Result<Vec<(String, String, Value)>> {
    let mut out = Vec::new();
    let dir = stage.join("oath/objects/pkg");
    for e in fs::read_dir(&dir).with_context(|| format!("read {}", dir.display()))? {
        let e = e?;
        let v: Value = oath_core::read_json(&e.path().join("desired.json"))
            .map_err(|err| anyhow::anyhow!("{}: {err}", e.path().display()))?;
        if let Some(h) = v.get("hash").and_then(Value::as_str).filter(|h| !h.is_empty()) {
            out.push((e.file_name().to_string_lossy().into_owned(), h.to_string(), v.clone()));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Whether the output's seed fixes `pkg:<name>`.
fn stage_fixed(stage: &Path, name: &str) -> Result<bool> {
    let p = stage.join("oath/objects/pkg").join(name).join("meta.json");
    let meta: Value =
        oath_core::read_json(&p).map_err(|e| anyhow::anyhow!("{}: {e}", p.display()))?;
    Ok(meta.get("fixed") == Some(&Value::Bool(true)))
}

/// Archive ids in an ESP listing of `loader/entries` and `oath/boot`.
fn archive_ids(listing: &str) -> Vec<u64> {
    let mut ids: Vec<u64> = listing
        .split_whitespace()
        .filter_map(|n| {
            boot::parse_archive_conf_id(n)
                .or_else(|| n.bytes().all(|b| b.is_ascii_digit()).then(|| n.parse().ok()).flatten())
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// The kernel options of the current entry that `boot::kernel_options`
/// does not write itself.
fn extra_options(current: &str) -> String {
    let own = boot::kernel_options("", "", "");
    let own: Vec<&str> = own.split_whitespace().filter(|t| !t.starts_with("oath.")).collect();
    current
        .split_whitespace()
        .filter(|t| {
            !own.contains(t) && !t.starts_with("oath.root=") && !t.starts_with("oath.subvol=")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn cmdline_val(cmdline: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    cmdline.split_whitespace().find_map(|t| t.strip_prefix(&prefix).map(str::to_string))
}

/// Partition 1 of the disk that holds `root` (`/dev/sda2` → `/dev/sda1`,
/// `/dev/nvme0n1p2` → `/dev/nvme0n1p1`).
fn esp_of(root: &str) -> Option<String> {
    let disk = root.trim_end_matches(|c: char| c.is_ascii_digit());
    if disk == root || !disk.starts_with("/dev/") {
        return None;
    }
    Some(format!("{disk}1"))
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
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

/// `install --qemu` of output `from`, then update that machine to `o` and
/// reboot it into the new boot.
fn rehearse(root: &Path, out: &Path, o: &Output, from: &str) -> Result<()> {
    let old = output::open(out, from)?;
    if old.id == o.id {
        bail!("--qemu {from} is the output being installed; name an older one");
    }
    let before: Vec<PathBuf> = runs(out);
    install::run_install(
        root,
        out,
        install::Opts {
            target: None,
            disk: "/dev/vda".into(),
            confirm: true,
            qemu: true,
            usb: false,
            hostname: None,
            overlay: None,
            unchecked: true,
            output: Some(old.id.clone()),
        },
    )?;
    let dir = runs(out)
        .into_iter()
        .find(|d| !before.contains(d))
        .context("install --qemu left no run directory")?;
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
    update(&r, o, None, true)?;
    reboot_and_verify(&r, o, Some(&mut vm))?;
    let entries = install::ssh_out(&r, &format!(
        "mkdir -p {ESP_MNT} && sudo -n mount -t vfat /dev/vda1 {ESP_MNT} && cat {ESP_MNT}/loader/oath-boots; sudo -n umount {ESP_MNT}; true"
    ))?;
    eprintln!("boot menu after the update:\n{entries}");
    if !entries.contains("oath-") {
        bail!("the ESP has no archived boot after the update");
    }
    eprintln!("QEMU update rehearsal ok: {} -> {}  {}", old.id, o.id, dir.display());
    copy_file(&dir.join("known_hosts"), &dir.join("known_hosts.kept")).ok();
    Ok(())
}

fn runs(out: &Path) -> Vec<PathBuf> {
    fs::read_dir(out.join("runs"))
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.to_string_lossy().ends_with("-install"))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn esp_is_partition_one_of_the_root_disk() {
        assert_eq!(esp_of("/dev/sda2").as_deref(), Some("/dev/sda1"));
        assert_eq!(esp_of("/dev/vda2").as_deref(), Some("/dev/vda1"));
        assert_eq!(esp_of("/dev/nvme0n1p2").as_deref(), Some("/dev/nvme0n1p1"));
        assert_eq!(esp_of("/dev/sda"), None);
    }

    #[test]
    fn archive_ids_from_an_esp_listing() {
        let ls =
            "/x/loader/entries:\noath.conf\noath-3.conf\nOATH-4.CON\n\n/x/oath/boot:\n3\n4\n5\n";
        assert_eq!(archive_ids(ls), vec![3, 4, 5]);
    }

    #[test]
    fn extra_options_keep_only_what_the_entry_added() {
        let cur = boot::kernel_options("/dev/sda2", "@", "quiet amdgpu.dc=0");
        assert_eq!(extra_options(&cur), "quiet amdgpu.dc=0");
        assert_eq!(
            cmdline_val("a oath.root=/dev/vda2 b", "oath.root").as_deref(),
            Some("/dev/vda2")
        );
    }
}
