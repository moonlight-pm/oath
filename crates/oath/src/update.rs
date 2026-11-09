//! `oath update` (R38): move this machine to the newest output published
//! for its profile, from its origins, when a key it trusts signed it.
//!
//! 1. Fetch `out/<profile>.json` (or `out/<id>.json` for `--output`) and
//!    its signature from the first origin whose release a key in
//!    `host:local.publishers` signed. A release older than the one this
//!    machine runs (`/oath/output.json`, which an install writes too) is
//!    refused unless named, and so is one whose order is unknown
//!    (`oath_core::update::newer`).
//! 2. Fetch its bundle and check it against the release's hash.
//! 3. Say what moves. With `--confirm`, as root:
//!    The packages as the moves leave them must pass the apply's needs
//!    check, or the update stops before anything changes.
//! 4. Refuse unless in sync with nothing on probation. Fetch every pack the
//!    moves pin that the store lacks, and build each recipe the owner
//!    built (R23: the output carries no tree) from the output's plan to
//!    the output's pin, before anything changes, so a fetch or build that
//!    fails leaves the machine as it was. Mount the ESP, plan the boot
//!    rotation, and snapshot `@` as `@boot-<n>` first: the archived boot
//!    is the old kernel with the old userspace.
//! 5. Write the accounts, file the output's plans, set every package to
//!    the output's pin and needs, and apply them in one generation (apply
//!    fetches each pack from the origins and hashes it). PID 1 then
//!    restarts each running service whose package moved, except one with
//!    a `seat` (R42), which keeps its program until it next starts; with
//!    `--reboot` it restarts none, and the boot starts each on its new
//!    program. An apply that moves sshd's program goes on probation: sshd
//!    restarts, and the update waits here until `sudo oath keep` runs in a
//!    new SSH session.
//! 6. Replace the image's own files outside the catalog, archive the ESP's
//!    boot as `<n>`, write the new kernel, initrd and oath-efi, keep five,
//!    and record the release.
//! 7. `--reboot`: reboot through PID 1.
//!
//! A machine from before R36 runs dropbear as `svc:sshd`. Once it has
//! booted a PID 1 that wrote sshd's config and carried dropbear's host key
//! over, `oath update --confirm` moves `svc:sshd` to OpenSSH, on
//! probation: a new login keeps it, or PID 1 puts dropbear back.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use oath_core::esp;
use oath_core::head::{self, Head, Offer, Taken};
use oath_core::update::{self as up, Move, Release};
use oath_core::{find_bin, Actor, ApplyHooks, Catalog, Error, ObjectId, Result, KIND_PKG};
use serde_json::{json, Map, Value};

use crate::live::{self, Follow, Live};

pub struct Opts {
    pub confirm: bool,
    pub output: Option<String>,
    /// Packages that keep this machine's pin (`--keep`), by name.
    pub keep: Vec<String>,
    pub reboot: bool,
    pub esp: Option<String>,
    pub json: bool,
}

pub fn run(root: &Path, cat: &Catalog, actor: &Actor, opts: Opts) -> Result<i32> {
    let host = cat.get(&ObjectId::new("host", "local"))?.desired;
    let profile = host["profile"].as_str().unwrap_or("").to_string();
    if profile.is_empty() && opts.output.is_some() {
        return Err(Error::hint(
            "host:local names no profile, so it follows no output",
            "oath set host:local profile=<the profile it was installed from> (oath schema host)",
        ));
    }
    let (origins, trusted) = trust(cat)?;
    let is_root = unsafe { libc::geteuid() } == 0;
    if opts.confirm && !is_root {
        return Err(Error::hint("an update writes the boot files", "sudo oath update --confirm"));
    }
    if let Some(id) = &opts.output {
        if !up::is_output_id(id) {
            return Err(Error::hint(format!("{id:?} is not an output id"), "12 hex digits"));
        }
    }
    let work = if is_root {
        root.join("run/update")
    } else {
        std::env::temp_dir().join(format!("oath-update-{}", unsafe { libc::getuid() }))
    };
    fs::create_dir_all(&work)?;
    let _work = Scratch(work.clone());
    let current: Option<Release> = oath_core::read_json(&root.join(up::RECORD)).ok();
    // A machine that follows no profile takes only heads (R72).
    if profile.is_empty() {
        let ran = up::Ran::of(current.as_ref());
        return heads_only(root, cat, actor, &opts, &work, &ran, None);
    }

    // 1. The release.
    let name = opts.output.clone().unwrap_or_else(|| profile.clone());
    let (origin, release, signer) = fetch_release(&origins, &name, &trusted, &work)?;
    if release.profile != profile {
        return Err(Error::hint(
            format!(
                "output {} is a `{}` output; this machine follows `{profile}`",
                release.output, release.profile
            ),
            "oath set host:local profile=<profile> to follow another",
        ));
    }
    if opts.output.as_ref().is_some_and(|id| *id != release.output) {
        return Err(Error::security(
            format!("{} is signed for output {}", up::release_key(&name), release.output),
            "the origin serves the wrong release",
        ));
    }
    eprintln!(
        "output {} for {profile}, published {} (signed by {signer}), from {origin}",
        release.output, release.published
    );
    let same = current.as_ref().is_some_and(|c| c.output == release.output);
    if let Some(c) = current.as_ref().filter(|_| !same && opts.output.is_none()) {
        let when = |r: &Release| match (&r.committed, r.published.as_str()) {
            (Some(t), _) => format!("built from a commit of {t}"),
            (None, "") => "never published".to_string(),
            (None, p) => format!("published {p}"),
        };
        let why = match up::newer(c, &release) {
            Some(false) => None,
            Some(true) => Some("newer than"),
            None => Some("of unknown order against"),
        };
        if let Some(why) = why {
            return Err(Error::hint(
                format!(
                    "this machine runs output {} ({}), {why} {} ({})",
                    c.output,
                    when(c),
                    release.output,
                    when(&release)
                ),
                format!("`oath update --output {}` takes it anyway", release.output),
            ));
        }
    }

    if same {
        let pid1 = if !is_root {
            "run as root to compare PID 1".to_string()
        } else if live::init_hash()? == release.init {
            "PID 1 is the output's".to_string()
        } else if opts.confirm && !opts.reboot {
            // An update that ran before this PID 1 could re-execute (R73).
            reexec_pid1(Path::new("/lib/oath/init"), &release.init).1
        } else {
            "PID 1 is not the output's yet: `sudo oath update --confirm` re-executes it, or a \
             reboot runs it"
                .to_string()
        };
        if sshd_is_dropbear(cat)? {
            if opts.confirm {
                let bundle = fetch_bundle(&origin, &release, &work)?;
                move_sshd(root, cat, actor, &bundle)?;
            } else {
                eprintln!(
                    "svc:sshd runs dropbear: `sudo oath update --confirm` moves it to OpenSSH"
                );
            }
        }
        // An older `oath` that made this update wrote the record as it
        // knew a release, without its commit, pins and seed (R47): the
        // signed release says them.
        if current.as_ref() != Some(&release) {
            if opts.confirm {
                oath_core::write_json(&root.join(up::RECORD), &release)?;
                eprintln!("recorded the release as signed (its commit, pins and seed)");
            } else {
                eprintln!(
                    "this machine's record of the release lacks what it says (its commit, pins \
                     and seed): `sudo oath update --confirm` writes it"
                );
            }
        }
        let header = (
            json!({ "output": release.output, "runs": true }),
            format!(
                "this machine runs output {}, the newest for {profile}; {pid1}",
                release.output
            ),
        );
        let ran = up::Ran::of(current.as_ref());
        return heads_only(root, cat, actor, &opts, &work, &ran, Some(header));
    }

    // 2. The bundle.
    let bundle = fetch_bundle(&origin, &release, &work)?;

    // 3. What moves.
    preflight(root, cat)?;
    let output = up::Output::read(&bundle)?;
    let mut machine = BTreeMap::new();
    for id in cat.ls(Some("pkg"))? {
        machine.insert(id.name.clone(), cat.get(&id)?.desired);
    }
    let have = |n: &str, h: &str| {
        output.packs.get(n).map(String::as_str) == Some(h) || oath_core::store_present(root, n, h)
    };
    let ran = up::Ran::of(current.as_ref());
    let keep = keep_names(&machine, &opts.keep)?;
    let held = held_by_cells(cat, &keep, |n| output.pin(n).is_some())?;
    let moves = up::plan(&machine, &output, &ran, &held, have);
    // A head wins over the release's pin of its package (R72).
    let (offers, refused) = fetch_heads(&origins, &machine, &trusted, &work)?;
    for r in &refused {
        eprintln!("  {r}");
    }
    let taken = read_taken(root);
    let unsettled = with_heads(&machine, moves, &offers, &taken, &ran, &keep);
    let moves = settle(root, &machine, unsettled.clone());
    let plans = plan_moves(cat, &bundle, &ran)?;
    let have_ids: Vec<String> = cat.ls(None)?.iter().map(|i| i.to_string()).collect();
    // What the profile gained since the release this machine runs is made
    // (R47); what that release had and the machine lacks was its owner's
    // to remove, and anything else is the owner's to make.
    let (svcs, missing): (Vec<String>, Vec<String>) = up::seed_ids(&bundle)?
        .into_iter()
        .filter(|id| !have_ids.contains(id))
        .filter(|id| !moves.iter().any(|m| m.id() == *id))
        .filter(|id| !plans.iter().any(|(n, ..)| *id == format!("plan:{n}")))
        .partition(|id| id.starts_with("svc:") && ran.is_new(id, false));
    // What the profile dropped since the release this machine runs leaves
    // with it (R62).
    let seeded = up::seed_ids(&bundle)?;
    let gone: Vec<String> = ran
        .seed()
        .filter(|id| id.starts_with("svc:") && !seeded.contains(id) && have_ids.contains(id))
        .filter(|id| !opts.keep.contains(id))
        .cloned()
        .collect();
    for m in &moves {
        eprintln!("  {}", m.describe());
    }
    for id in &svcs {
        eprintln!("  {id} is made (new to the profile)");
    }
    for id in &gone {
        eprintln!("  {id} leaves: this release no longer has it; `--keep {id}` keeps it");
    }
    let docs = moved_docs(cat, &moves)?;
    check_moved_needs(&docs)?;
    check_links(root, &docs)?;
    for (name, from, to, _) in &plans {
        let from = if from.is_empty() { "is made (new to the profile):" } else { up::short(from) };
        eprintln!("  plan:{name} {from} -> {}", up::short(to));
    }
    if !missing.is_empty() {
        eprintln!(
            "  in the output's seed, not here, left to the owner (oath new): {}",
            missing.join(" ")
        );
    }
    let from = current.as_ref().map(|c| c.output.as_str()).unwrap_or("an unrecorded output");
    if !opts.confirm {
        finish(
            &opts,
            preview(root, cat, &moves, &machine, &output, current.as_ref(), &release, from)?
                .merge(json!({ "made": svcs, "gone": gone, "left": missing })),
            &format!(
                "{} packages move from {from} to output {}; `sudo oath update --confirm` \
                 moves this machine (add --reboot to boot it)",
                moves.iter().filter(|m| m.kind() == "drop" || m.fields().is_some()).count(),
                release.output
            ),
        );
        return Ok(0);
    }

    // 4–6, every pack and recipe first.
    prefetch(root, cat, &unsettled)?;
    let built = build_recipes(root, cat, &bundle, &output, &moves)?;
    // Every tree is here now: the library check is real, and no two
    // packages may ship one name.
    let moves = resettle(root, &machine, unsettled, &moves);
    let docs = moved_docs(cat, &moves)?;
    check_moved_needs(&docs)?;
    check_links(root, &docs)?;
    move_sshd(root, cat, actor, &bundle)?;
    let old_init = live::init_hash()?;
    let root_dev =
        cmdline_val(&fs::read_to_string("/proc/cmdline")?, "oath.root").ok_or_else(|| {
            Error::hint(
                "the running kernel has no oath.root= (not booted from an Oath ESP)",
                "boot this machine from its own disk",
            )
        })?;
    let esp_dev = match &opts.esp {
        Some(e) => e.clone(),
        None => {
            let node = if root_dev.starts_with("UUID=") {
                sh_out(Command::new(find_bin("findfs")).arg(&root_dev))?
            } else {
                root_dev.clone()
            };
            esp_of(&node).ok_or_else(|| {
                Error::hint(format!("no ESP for {root_dev}"), "oath update --esp <node>")
            })?
        }
    };
    let mnt = root.join("run/update-esp");
    fs::create_dir_all(&mnt)?;
    if !is_mounted(&mnt) {
        sh(Command::new(find_bin("mount")).args(["-t", "vfat", &esp_dev]).arg(&mnt))?;
    }
    // R73: the output's PID 1 takes over before the apply, so it is the one
    // that converges; what PID 1 does only at boot waits for a boot.
    let reexec = !opts.reboot && old_init != release.init && {
        eprintln!(">> PID 1: re-execute into the output's, every service running");
        let (done, text) = reexec_pid1(&bundle.join("stage/lib/oath/init"), &release.init);
        eprintln!("{text}");
        done
    };
    let steps = Steps {
        moves: &moves,
        svcs: &svcs,
        gone: &gone,
        plans: &plans,
        built: &built,
        reboot: opts.reboot,
    };
    let result = update(root, cat, actor, &bundle, &mnt, &root_dev, &steps);
    let _ = Command::new("sync").status();
    if Command::new(find_bin("umount")).arg(&mnt).status().is_ok_and(|s| s.success()) {
        let _ = fs::remove_dir(&mnt);
    }
    if reexec && result.is_err() {
        // Back to the binary it ran, which the update has not replaced yet,
        // so the machine is as it was.
        let (back, text) = reexec_pid1(Path::new("/lib/oath/init"), &old_init);
        if back {
            eprintln!("the update stopped: PID 1 went back to the binary it ran");
        } else {
            eprintln!(
                "the update stopped with PID 1 on the output's binary ({text}); the next boot \
                 runs the ESP's"
            );
        }
    }
    let generation = result?;
    oath_core::write_json(&root.join(up::RECORD), &release)?;
    record_heads(root, cat, &offers)?;
    let at_boot = |moves: bool| if moves { "next-boot" } else { "runs" };
    let kernel = at_boot(up::kernel_moves(current.as_ref(), &release));
    let firmware = at_boot(up::firmware_moves(current.as_ref(), &release));
    let pid1 = match (old_init == release.init, reexec) {
        (true, _) => "runs",
        (false, true) => "re-executed",
        (false, false) => "next-boot",
    };
    let packages = match generation {
        0 => "no package moved".to_string(),
        g => format!("its packages are generation {g}"),
    };
    let msg = format!(
        "updated to output {} ({packages}); the boot it replaced stays in the menu",
        release.output
    );
    if opts.reboot {
        finish(
            &opts,
            json!({ "output": release.output, "generation": generation, "reboot": true }),
            &format!("{msg}; rebooting"),
        );
        sh(&mut Command::new(find_bin("reboot")))?;
    } else {
        let mut text = msg;
        if pid1 == "re-executed" {
            text.push_str(
                "; PID 1 re-executed into its binary (what it does only at boot: mounts, \
                 modules, sysctls, device modes, /etc, waits for the next boot)",
            );
        }
        if let Some(wait) = next_boot(kernel, firmware, pid1) {
            text.push_str("; ");
            text.push_str(wait);
        }
        finish(
            &opts,
            json!({
                "output": release.output,
                "generation": generation,
                "pid1": pid1,
                "kernel": kernel,
                "firmware": firmware,
            }),
            &text,
        );
    }
    Ok(0)
}

/// What an update left for the next boot, as a person reads it. A new
/// kernel's initrd says the firmware too.
fn next_boot(kernel: &str, firmware: &str, pid1: &str) -> Option<&'static str> {
    let waits = |s: &str| s == "next-boot";
    match (waits(kernel), waits(firmware), waits(pid1)) {
        (true, _, true) => Some("reboot to run its kernel, initrd and PID 1"),
        (true, _, false) => Some("reboot to run its kernel and initrd"),
        (false, true, true) => Some("reboot to load its firmware and run its PID 1"),
        (false, true, false) => Some("reboot to load its firmware"),
        (false, false, true) => Some("reboot to run its PID 1"),
        (false, false, false) => None,
    }
}

/// Re-execute PID 1 into `bin` when it is the PID 1 hashed `init` (R73):
/// whether PID 1 runs it now, and what came of it as a person reads it.
fn reexec_pid1(bin: &Path, init: &str) -> (bool, String) {
    match fs::read(bin).map(|b| oath_core::file_hash(&b)) {
        Ok(h) if h == init => {}
        Ok(h) => {
            return (
                false,
                format!(
                    "{} is {}, not PID 1 {}: PID 1 stays on its binary until a boot",
                    bin.display(),
                    up::short(&h),
                    up::short(init)
                ),
            )
        }
        Err(e) => return (false, format!("{}: {e}; PID 1 stays on its binary", bin.display())),
    }
    match live::reexec_init(bin, init) {
        Ok(live::Reexec::Done) => {
            (true, format!("PID 1 runs {} with every service as it was", up::short(init)))
        }
        Ok(live::Reexec::Unsupported) => (
            false,
            "this PID 1 predates re-executing on request (R73): the output's runs from the next \
             boot"
                .to_string(),
        ),
        Err(e) => (false, format!("PID 1 stays on its binary until a boot: {e}")),
    }
}

/// A directory removed when the run ends, however it ends: nothing of an
/// update stays behind.
pub(crate) struct Scratch(pub(crate) PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// What `oath --json update` says without `--confirm`: each move as fields,
/// with the sentence a person reads as `text`; whether it takes a reboot to
/// run (its kernel or PID 1 moved) and whether sshd's program moves, which
/// puts the apply on probation; and the signed release.
#[allow(clippy::too_many_arguments)]
fn preview(
    root: &Path,
    cat: &Catalog,
    moves: &[Move],
    machine: &BTreeMap<String, Value>,
    output: &up::Output,
    current: Option<&Release>,
    release: &Release,
    from: &str,
) -> Result<Preview> {
    let services = services_of(cat)?;
    let pin = |name: &str| {
        machine.get(name).and_then(|d| d["hash"].as_str()).filter(|h| !h.is_empty())
    };
    let moved = |m: &Move| m.to().is_some_and(|to| pin(m.name()) != Some(to));
    // A new PID 1 alone takes no reboot when this one re-executes (R73).
    let kernel_moves = up::kernel_moves(current, release);
    let firmware_moves = up::firmware_moves(current, release);
    let runs = live::init_hash().ok().or_else(|| current.map(|c| c.init.clone()));
    let pid1 = match runs {
        Some(h) if h == release.init => "runs",
        _ if live::can_reexec() => "re-exec",
        _ => "next-boot",
    };
    let reboot = kernel_moves || firmware_moves || pid1 == "next-boot";
    let probation = moves.iter().any(|m| {
        let runs_sshd = services
            .get(m.name())
            .is_some_and(|s| s.iter().any(|v| v["id"] == json!("svc:sshd")));
        runs_sshd && moved(m)
    });
    let list: Vec<Value> = moves
        .iter()
        .map(|m| {
            let name = m.name();
            let doc = machine.get(name);
            let side = |hash: Option<&str>, version: Option<String>, source: Option<&Value>| {
                hash.map(|h| {
                    let mut v = json!({ "hash": h });
                    if let Some(ver) = version {
                        v["version"] = json!(ver);
                    }
                    if let Some(s) = source.filter(|s| !s.is_null()) {
                        v["source"] = s.clone();
                    }
                    v
                })
            };
            let to = m.to().or(pin(name));
            let doc_to = output.pins.iter().find(|(n, _, _)| n == name).map(|(_, _, d)| d);
            let version_to = release
                .packs
                .get(name)
                .map(|p| p.version.clone())
                .filter(|v| !v.is_empty())
                .or_else(|| output.versions.get(name).cloned())
                .or_else(|| to.and_then(|h| version_in_store(root, name, h)));
            json!({
                "id": m.id(),
                "move": m.kind(),
                "from": side(
                    pin(name),
                    pin(name).and_then(|h| version_in_store(root, name, h)),
                    doc.and_then(|d| d.get("source")),
                ),
                "to": side(
                    to,
                    version_to,
                    doc_to.and_then(|d| d.get("source")).or_else(|| match m {
                        Move::Head { set, .. } => set.get("source"),
                        _ => None,
                    }),
                ),
                "overlay": m.replaces_overlay(),
                "services": services.get(name).cloned().unwrap_or_default(),
                "text": m.describe(),
            })
        })
        .collect();
    Ok(Preview(json!({
        "output": release.output,
        "from": from,
        "moves": list,
        "reboot": reboot,
        "firmware": if firmware_moves { "next-boot" } else { "runs" },
        "pid1": pid1,
        "probation": probation,
        "release": release,
    })))
}

/// The preview's document, to which the caller adds what it found.
struct Preview(Value);

impl Preview {
    fn merge(self, more: Value) -> Value {
        let mut v = self.0;
        if let (Some(a), Value::Object(b)) = (v.as_object_mut(), more) {
            a.extend(b);
        }
        v
    }
}

fn version_in_store(root: &Path, name: &str, hash: &str) -> Option<String> {
    let comps = oath_core::read_components(&oath_core::realization_dir(root, name, hash))?;
    oath_core::own_version(&comps, name)
}

/// The running services, by the package each runs from: `{id, seat}`. One
/// with a seat keeps its program when its package moves, until it next
/// starts (R42).
fn services_of(cat: &Catalog) -> Result<BTreeMap<String, Vec<Value>>> {
    let mut out: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for id in cat.ls(Some("svc"))? {
        let o = cat.get(&id)?;
        if let Some(pkg) = o.actual["pkg"]["id"].as_str().and_then(|p| p.strip_prefix("pkg:")) {
            let seat = o.desired.get("seat").is_some_and(|s| !s.is_null());
            out.entry(pkg.to_string())
                .or_default()
                .push(json!({ "id": id.to_string(), "seat": seat }));
        }
    }
    Ok(out)
}

/// `host:local`'s origins, and the keys it trusts to have signed what they
/// serve (R38).
pub(crate) fn trust(cat: &Catalog) -> Result<(Vec<String>, Vec<String>)> {
    let host = cat.get(&ObjectId::new("host", "local"))?.desired;
    let list = |k: &str| -> Vec<String> {
        host[k]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(String::from))
            .collect()
    };
    let (origins, trusted) = (list("origins"), list("publishers"));
    if trusted.is_empty() {
        return Err(Error::hint(
            "host:local.publishers is empty: this machine trusts no build host's outputs",
            "a person adds a build host's key: oath set host:local publishers='[\"ssh-ed25519 \
             AAAA… oath@buccina\"]' (R38)",
        ));
    }
    if origins.is_empty() {
        return Err(Error::hint("host:local names no origins", "oath schema host"));
    }
    Ok((origins, trusted))
}

/// What steps 4–6 move: the packages, the plans, the recipes built for
/// them, and whether a reboot follows.
struct Steps<'a> {
    moves: &'a [Move],
    /// Services new to the profile, made from the output's seed.
    svcs: &'a [String],
    /// Services the profile dropped.
    gone: &'a [String],
    plans: &'a [(String, String, String, PathBuf)],
    built: &'a [oath_core::BuildReport],
    reboot: bool,
}

/// Steps 4–6 with the ESP mounted at `mnt`. Returns the generation.
fn update(
    root: &Path,
    cat: &Catalog,
    actor: &Actor,
    bundle: &Path,
    mnt: &Path,
    root_dev: &str,
    steps: &Steps,
) -> Result<u64> {
    let Steps { moves, svcs, gone, plans, built, reboot } = *steps;
    let mut ids = esp::existing_archive_ids(
        &esp::list_archive_confs(&mnt.join("loader/entries")).unwrap_or_default(),
    );
    ids.extend(esp::existing_archive_slot_ids(&mnt.join("oath/boot")));
    ids.sort_unstable();
    ids.dedup();
    let rotate = esp::plan_rotate(&ids);
    let current = fs::read_to_string(mnt.join("loader/entries/oath.conf")).unwrap_or_default();
    let extra = extra_options(&esp::parse_bls(&current).options);
    let top = Path::new(oath_core::BTRFS_TOP);
    let snap = top.join(oath_core::boot_subvol_name(rotate.new_id));
    // One left by an update that stopped before the ESP is not this boot's.
    if snap.exists() {
        sh(Command::new(find_bin("btrfs")).args(["subvolume", "delete"]).arg(&snap))?;
    }
    eprintln!(">> snapshot @ as {} (boot {}'s userspace)", snap.display(), rotate.new_id);
    sh(Command::new(find_bin("btrfs"))
        .args(["subvolume", "snapshot"])
        .arg(top.join(oath_core::LIVE_SUBVOL))
        .arg(&snap))?;

    let stage = bundle.join("stage");
    if Path::new("/bin/zsh").exists() {
        put_root_files(&stage, up::ACCOUNT_FILES)?;
    }
    for (name, _, to, path) in plans {
        let (plan, bytes) = oath_core::lint_path(path)?;
        if plan.name != *name {
            return Err(Error::Msg(format!("{}: names plan {}", path.display(), plan.name)));
        }
        oath_core::file_plan(root, &bytes)?;
        eprintln!(">> plan:{name} {}", up::short(to));
    }
    for r in built {
        oath_core::record_build(root, r)?;
    }
    let mut made = Vec::new();
    for id in svcs {
        let id: ObjectId = id.parse()?;
        let doc: Value = oath_core::read_json(
            &stage.join("oath/objects/svc").join(&id.name).join("desired.json"),
        )?;
        let Value::Object(map) = doc else { continue };
        made.push((id, map));
    }
    let generation = apply_moves(root, cat, actor, moves, gone, made, reboot, Some(&snap))?;

    eprintln!(">> {}", up::ROOT_FILES.join(" "));
    put_root_files(&stage, up::ROOT_FILES)?;
    eprintln!(">> ESP: archive boot {}, write the new kernel, initrd and oath-efi", rotate.new_id);
    let loader = fs::read_to_string(mnt.join("loader/loader.conf"))
        .unwrap_or_else(|_| esp::LOADER_CONF_METAL.to_string());
    let (id, prune) = esp::apply_rotate_files(
        mnt,
        &bundle.join(up::KERNEL),
        &bundle.join(up::INITRD),
        Some(&bundle.join(up::EFI)),
        root_dev,
        &extra,
        &loader,
    )?;
    if id != rotate.new_id {
        eprintln!("warning: the ESP archived boot {id}, the snapshot is boot {}", rotate.new_id);
    }
    for old in prune {
        let sub = top.join(oath_core::boot_subvol_name(old));
        if sub.exists() {
            let _ =
                Command::new(find_bin("btrfs")).args(["subvolume", "delete"]).arg(&sub).status();
        }
    }
    Ok(generation)
}

/// Write the moves into the catalog and apply them in one generation:
/// remove what leaves (`gone`, and each drop), set or make each package,
/// make each service in `made`. The seat's services keep their programs
/// (R42), and with `reboot` no service restarts. Waits out a probation, and
/// on an undo deletes `snap` (the archived boot) and says so. Returns the
/// generation, 0 when nothing changed.
#[allow(clippy::too_many_arguments)]
fn apply_moves(
    root: &Path,
    cat: &Catalog,
    actor: &Actor,
    moves: &[Move],
    gone: &[String],
    made: Vec<(ObjectId, Map<String, Value>)>,
    reboot: bool,
    snap: Option<&Path>,
) -> Result<u64> {
    let mut changed = Vec::new();
    for id in moves.iter().filter(|m| m.kind() == "drop").map(Move::id).chain(gone.iter().cloned()) {
        let id: ObjectId = id.parse()?;
        // One a newer base fixes, or that a person's overlay made fixed.
        match cat.remove_object(&id) {
            Ok(()) => changed.push(id),
            Err(e) => eprintln!("{id} stays: {e}"),
        }
    }
    for m in moves {
        let Some(fields) = m.fields().and_then(Value::as_object) else { continue };
        let id: ObjectId = m.id().parse()?;
        let map: Map<String, Value> = fields.clone();
        if matches!(m, Move::New { .. }) {
            cat.new_object(&id, map)?;
        } else {
            cat.set_fields(&id, map)?;
        }
        changed.push(id);
    }
    for (id, map) in made {
        cat.new_object(&id, map)?;
        changed.push(id);
    }
    let mut generation = 0;
    if !changed.is_empty() {
        eprintln!(">> apply {} objects", changed.len());
        // The seat's services are a person's session: an update restarts
        // none, and before a reboot no service at all.
        let follow = if reboot { Follow::AtReboot } else { Follow::KeepSeat };
        let hooks = Live { catalog_root: root.to_path_buf(), follow };
        let r = cat.apply_with(
            Some(changed.clone()),
            true,
            oath_core::probation::DEFAULT_SECS,
            actor,
            &hooks,
        )?;
        generation = r.generation;
        eprintln!("applied generation {}", r.generation);
        // Each package the update pinned is linked at its pin, or it stops
        // here: `oath diff` cannot see a pin that was never linked.
        let off: Vec<String> = changed
            .iter()
            .filter_map(|id| {
                let o = cat.get(id).ok()?;
                let pin = o.desired["hash"].as_str().filter(|h| !h.is_empty())?;
                let got = o.actual["hash"].as_str().unwrap_or("none");
                (o.desired["present"] != json!(false) && got != pin)
                    .then(|| format!("{id} is {}, pinned {}", up::short(got), up::short(pin)))
            })
            .collect();
        if !off.is_empty() {
            return Err(Error::hint(
                format!("the apply left packages off their pins: {}", off.join("; ")),
                "oath get <id>; `sudo oath apply <id>` links it",
            ));
        }
        if reboot && r.probation.is_none() {
            eprintln!("PID 1 restarts no service now: the reboot starts each on its new program");
        }
        if let Some(secs) = r.probation {
            if reboot {
                // A new session can keep only an sshd that runs its new program.
                eprintln!(
                    "sshd's program moved: PID 1 restarts the services whose programs moved \
                     now, but the seat's, so a new session reaches the new sshd"
                );
                Live { catalog_root: root.to_path_buf(), follow: Follow::KeepSeat }
                    .notify_init()?;
            }
            wait_kept(root, r.generation, secs)?;
            // PID 1's undo puts the old pins back.
            let undone = moves.iter().any(|m| {
                let Some(h) = m.fields().and_then(|f| f.get("hash")) else { return false };
                let id: Option<ObjectId> = m.id().parse().ok();
                let pin = id
                    .and_then(|id| cat.get(&id).ok())
                    .and_then(|o| o.desired.get("hash").cloned());
                pin.as_ref() != Some(h)
            });
            if undone {
                if let Some(snap) = snap {
                    let _ = Command::new(find_bin("btrfs"))
                        .args(["subvolume", "delete"])
                        .arg(snap)
                        .status();
                }
                return Err(Error::hint(
                    format!("generation {} was undone when its probation ran out", r.generation),
                    "nothing else changed; run `sudo oath update --confirm` again and keep it \
                     from a new SSH session",
                ));
            }
        }
    }
    Ok(generation)
}

/// `(name, machine pin, output pin, plan file)` of each plan the machine
/// has whose pin the output moves (R23), and of each new to the profile
/// since the release it runs (`ran`, R47), with no machine pin.
fn plan_moves(
    cat: &Catalog,
    bundle: &Path,
    ran: &up::Ran,
) -> Result<Vec<(String, String, String, PathBuf)>> {
    let mut out = Vec::new();
    let Ok(dir) = fs::read_dir(bundle.join("stage/oath/objects/plan")) else { return Ok(out) };
    for e in dir {
        let name = e?.file_name().to_string_lossy().into_owned();
        let id = ObjectId::new("plan", &name);
        let mine = match cat.get(&id) {
            Ok(o) => o.desired,
            Err(_) if ran.is_new(&id.to_string(), false) => json!({}),
            Err(_) => continue,
        };
        let doc: Value = oath_core::read_json(
            &bundle.join("stage/oath/objects/plan").join(&name).join("desired.json"),
        )?;
        let Some(pin) = doc["hash"].as_str().filter(|h| !h.is_empty()) else { continue };
        let cur = mine["hash"].as_str().unwrap_or("");
        if cur == pin {
            continue;
        }
        let path =
            bundle.join("stage/oath/store/plan").join(&name).join(pin).join(oath_core::PLAN_NAME);
        out.push((name, cur.to_string(), pin.to_string(), path));
    }
    out.sort();
    Ok(out)
}

/// The first origin that serves `out/<name>.json` signed by a trusted key.
fn fetch_release(
    origins: &[String],
    name: &str,
    trusted: &[String],
    work: &Path,
) -> Result<(String, Release, String)> {
    let key = up::release_key(name);
    let mut why = Vec::new();
    for o in origins {
        let o = o.trim_end_matches('/');
        let (doc, sig) = (work.join("release.json"), work.join("release.json.sig"));
        let got = fetch(&format!("{o}/{key}"), &doc)
            .and_then(|_| fetch(&format!("{o}/{key}.sig"), &sig))
            .and_then(|_| {
                let bytes = fs::read(&doc)?;
                let signer = oath_core::sshsig::verify(
                    &bytes,
                    &fs::read_to_string(&sig)?,
                    up::NAMESPACE,
                    trusted,
                )?;
                let r: Release = serde_json::from_slice(&bytes)?;
                Ok((r, signer))
            });
        match got {
            Ok((r, signer)) => return Ok((o.to_string(), r, signer)),
            Err(e) => why.push(format!("{o}: {e}")),
        }
    }
    Err(Error::hint(
        format!(
            "no origin served a release for `{name}` that this machine trusts:\n  {}",
            why.join("\n  ")
        ),
        "oath get host:local (origins, publishers, profile)",
    ))
}

/// The head of each package `machine` has, and of each package those
/// heads and the machine's packages need, from every origin (R72): the
/// newest a key in `trusted` may sign for that name. Returns the offers and
/// what was refused or could not be read, one line each.
pub(crate) fn fetch_heads(
    origins: &[String],
    machine: &BTreeMap<String, Value>,
    trusted: &[String],
    work: &Path,
) -> Result<(BTreeMap<String, Offer>, Vec<String>)> {
    let dir = work.join("heads");
    let _ = fs::remove_dir_all(&dir);
    let (mut offers, mut refused) = (BTreeMap::<String, Offer>::new(), Vec::new());
    let need_names = |d: &Value| -> Vec<String> {
        d["needs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n["id"].as_str())
            .map(|id| id.trim_start_matches("pkg:").to_string())
            .collect()
    };
    let mut want: BTreeSet<String> = machine.keys().cloned().collect();
    want.extend(machine.values().flat_map(need_names));
    let mut asked = BTreeSet::new();
    for round in 0.. {
        if want.is_empty() {
            break;
        }
        for (i, o) in origins.iter().enumerate() {
            let o = o.trim_end_matches('/');
            let d = dir.join(format!("{round}-{i}"));
            let Some(got) = fetch_many(o, &want, &d)? else {
                refused.push(format!("{o} did not answer for heads"));
                continue;
            };
            for name in got {
                let read = |ext: &str| fs::read(d.join(format!("{name}.{ext}")));
                let checked = (|| {
                    let bytes = read("json")?;
                    let sig = String::from_utf8_lossy(&read("json.sig")?).into_owned();
                    let signer = oath_core::sshsig::verify_for(
                        &bytes,
                        &sig,
                        head::NAMESPACE,
                        trusted,
                        &name,
                    )?;
                    let h: Head = serde_json::from_slice(&bytes)?;
                    h.check(&name)?;
                    Ok::<_, Error>((h, signer))
                })();
                match checked {
                    Ok((head, signer)) => {
                        let offer = Offer { head, signer, origin: o.to_string() };
                        let offer = match offers.remove(&name) {
                            Some(prev) => prev.later(offer),
                            None => offer,
                        };
                        offers.insert(name, offer);
                    }
                    Err(e) => refused.push(format!("pkg:{name}'s head on {o} is refused: {e}")),
                }
            }
        }
        asked.extend(std::mem::take(&mut want));
        want = offers
            .values()
            .flat_map(|o| o.head.needs.iter())
            .map(|n| n.id.trim_start_matches("pkg:").to_string())
            .filter(|n| !asked.contains(n))
            .collect();
    }
    Ok((offers, refused))
}

/// `{origin}/pkg/<name>.json` and its `.sig` for each of `names`, into
/// `dir`, in one curl run. The names it got both files of, or None when the
/// origin answered none of the requests.
fn fetch_many(origin: &str, names: &BTreeSet<String>, dir: &Path) -> Result<Option<Vec<String>>> {
    fs::create_dir_all(dir)?;
    let mut cfg = String::new();
    for n in names {
        for ext in ["json", "json.sig"] {
            let out = dir.join(format!("{n}.{ext}"));
            let url = format!("{origin}/pkg/{n}.{ext}");
            cfg.push_str(&format!("url = \"{url}\"\noutput = \"{}\"\n", out.display()));
        }
    }
    let cfg_file = dir.join("curl.cfg");
    fs::write(&cfg_file, cfg)?;
    let out = Command::new(find_bin("curl"))
        .args(["-sSL", "--fail", "--parallel", "--parallel-max", "16", "--retry", "1"])
        .args(["-w", "%{http_code}\\n", "-K"])
        .arg(&cfg_file)
        .output()
        .map_err(|e| Error::Msg(format!("curl: {e}")))?;
    let answered = String::from_utf8_lossy(&out.stdout).lines().any(|c| c.trim() != "000");
    if !answered {
        return Ok(None);
    }
    let have = |n: &str, ext: &str| {
        fs::metadata(dir.join(format!("{n}.{ext}"))).is_ok_and(|m| m.len() > 0)
    };
    Ok(Some(names.iter().filter(|n| have(n, "json") && have(n, "json.sig")).cloned().collect()))
}

/// The heads this machine took (`/oath/heads.json`).
fn read_taken(root: &Path) -> head::Record {
    oath_core::read_json(&root.join(head::RECORD)).unwrap_or_default()
}

/// Record each offered head a package here now runs: the update will
/// refuse an older one.
fn record_heads(root: &Path, cat: &Catalog, offers: &BTreeMap<String, Offer>) -> Result<()> {
    let mut record = read_taken(root);
    let before = record.clone();
    for (name, o) in offers {
        let Ok(obj) = cat.get(&ObjectId::new(KIND_PKG, name)) else { continue };
        if obj.desired["hash"].as_str() == Some(o.head.hash.as_str()) {
            record.insert(name.clone(), Taken::of(&o.head, &o.signer));
        }
    }
    if record != before {
        oath_core::write_json(&root.join(head::RECORD), &record)?;
    }
    Ok(())
}

/// The release's moves, with each package that has a head moved by its
/// head instead (R72): a head wins over the release's pin, a package with
/// a head is never dropped for leaving the release, and one new to the
/// profile is made at its head.
fn with_heads(
    machine: &BTreeMap<String, Value>,
    moves: Vec<Move>,
    offers: &BTreeMap<String, Offer>,
    taken: &head::Record,
    ran: &up::Ran,
    keep: &BTreeSet<String>,
) -> Vec<Move> {
    let mut out = Vec::new();
    for m in moves {
        let Some(offer) = offers.get(m.name()) else {
            out.push(m);
            continue;
        };
        if let Move::New { name, doc, why } = m {
            let mut d = offer.head.doc();
            d["present"] = doc.get("present").cloned().unwrap_or(json!(true));
            out.push(Move::New { name, doc: d, why });
        }
    }
    let made: BTreeSet<String> = out
        .iter()
        .filter(|m| matches!(m, Move::New { .. }))
        .map(|m| m.name().to_string())
        .collect();
    let heads = head::plan(machine, offers, taken, keep, |n, pin| ran.overlay(n, pin).is_some());
    out.extend(heads.into_iter().filter(|m| !made.contains(m.name())));
    out
}

/// [`head::settle`] against the trees in this store: what a package's ELF
/// files load, and what each package puts in `/lib`.
pub(crate) fn settle(root: &Path, machine: &BTreeMap<String, Value>, moves: Vec<Move>) -> Vec<Move> {
    let tree =
        |n: &str, h: &str| Some(oath_core::realization_dir(root, n, h)).filter(|t| t.is_dir());
    head::settle(
        machine,
        moves,
        |n, h| tree(n, h).map(|t| oath_core::elf::loads(&t)),
        |n, h| tree(n, h).map(|t| oath_core::elf::provides(&t)),
    )
}

/// Settle again once every pack is here, and say what changed since the
/// preview (`before`).
fn resettle(
    root: &Path,
    machine: &BTreeMap<String, Value>,
    unsettled: Vec<Move>,
    before: &[Move],
) -> Vec<Move> {
    let after = settle(root, machine, unsettled);
    let text = |m: &[Move]| m.iter().map(Move::describe).collect::<Vec<_>>();
    if text(&after) != text(before) {
        eprintln!("with every pack here, the library check settles:");
        for m in &after {
            eprintln!("  {}", m.describe());
        }
    }
    after
}

/// An update that moves only packages to their heads (R72): the release
/// this machine runs is the newest (`header` says so), or it follows no
/// profile. No boot file changes; with `--reboot` it reboots after.
fn heads_only(
    root: &Path,
    cat: &Catalog,
    actor: &Actor,
    opts: &Opts,
    work: &Path,
    ran: &up::Ran,
    header: Option<(Value, String)>,
) -> Result<i32> {
    let (origins, trusted) = trust(cat)?;
    let mut machine = BTreeMap::new();
    for id in cat.ls(Some(KIND_PKG))? {
        machine.insert(id.name.clone(), cat.get(&id)?.desired);
    }
    let (offers, refused) = fetch_heads(&origins, &machine, &trusted, work)?;
    for r in &refused {
        eprintln!("  {r}");
    }
    let keep = keep_names(&machine, &opts.keep)?;
    let taken = read_taken(root);
    let unsettled =
        head::plan(&machine, &offers, &taken, &keep, |n, pin| ran.overlay(n, pin).is_some());
    let moves = settle(root, &machine, unsettled.clone());
    for m in &moves {
        eprintln!("  {}", m.describe());
    }
    let (mut v, said) = header.unwrap_or((json!({}), String::new()));
    let said = |more: String| if said.is_empty() { more } else { format!("{said}; {more}") };
    let is_root = unsafe { libc::geteuid() } == 0;
    v["heads"] = json!(offers.len());
    v["refused"] = json!(refused);
    let moving = moves.iter().filter(|m| m.fields().is_some()).count();
    if moving == 0 {
        if opts.confirm && is_root {
            record_heads(root, cat, &offers)?;
        }
        v["moves"] = json!(move_list(cat, &moves, &machine)?);
        let held = moves.len();
        let text = match (offers.len(), held) {
            (0, _) => "no package here has a head".to_string(),
            (n, 0) => format!("every package with a head runs it ({n} here)"),
            (n, h) => format!("{h} of the {n} packages here with a head wait"),
        };
        finish(opts, v, &said(text));
        return Ok(0);
    }
    preflight(root, cat)?;
    let docs = moved_docs(cat, &moves)?;
    check_moved_needs(&docs)?;
    check_links(root, &docs)?;
    if !opts.confirm {
        v["moves"] = json!(move_list(cat, &moves, &machine)?);
        finish(
            opts,
            v,
            &said(format!(
                "{moving} packages move to their heads; `sudo oath update --confirm` moves them"
            )),
        );
        return Ok(0);
    }
    prefetch(root, cat, &unsettled)?;
    let moves = resettle(root, &machine, unsettled, &moves);
    let docs = moved_docs(cat, &moves)?;
    check_moved_needs(&docs)?;
    check_links(root, &docs)?;
    let generation = apply_moves(root, cat, actor, &moves, &[], Vec::new(), opts.reboot, None)?;
    record_heads(root, cat, &offers)?;
    v["generation"] = json!(generation);
    v["moves"] = json!(move_list(cat, &moves, &machine)?);
    let text = match generation {
        0 => "no package moved".to_string(),
        g => format!("packages moved to their heads in generation {g}"),
    };
    if opts.reboot {
        v["reboot"] = json!(true);
        finish(opts, v, &said(format!("{text}; rebooting")));
        sh(&mut Command::new(find_bin("reboot")))?;
    } else {
        finish(opts, v, &said(text));
    }
    Ok(0)
}

/// The moves as a program reads them.
fn move_list(
    cat: &Catalog,
    moves: &[Move],
    machine: &BTreeMap<String, Value>,
) -> Result<Vec<Value>> {
    let services = services_of(cat)?;
    Ok(moves
        .iter()
        .map(|m| {
            let name = m.name();
            let from = machine.get(name).and_then(|d| d["hash"].as_str()).filter(|h| !h.is_empty());
            json!({
                "id": m.id(),
                "move": m.kind(),
                "from": from,
                "to": m.to(),
                "overlay": m.replaces_overlay(),
                "services": services.get(name).cloned().unwrap_or_default(),
                "text": m.describe(),
            })
        })
        .collect())
}

/// The bundle, checked against the release and unpacked into `work/bundle`.
fn fetch_bundle(origin: &str, r: &Release, work: &Path) -> Result<PathBuf> {
    let tar = bundle_cache().unwrap_or_else(|| work.join("update.tar"));
    let is_it = |bytes: &[u8]| {
        bytes.len() as u64 == r.bundle.size && oath_core::file_hash(bytes) == r.bundle.hash
    };
    if !fs::read(&tar).is_ok_and(|b| is_it(&b)) {
        let part = tar.with_extension("part");
        fetch(&format!("{origin}/{}", up::bundle_key(&r.output)), &part)?;
        let bytes = fs::read(&part)?;
        if !is_it(&bytes) {
            let _ = fs::remove_file(&part);
            return Err(Error::security(
                format!(
                    "{} is not the bundle the release signed ({}, {} bytes)",
                    up::bundle_key(&r.output),
                    oath_core::file_hash(&bytes),
                    bytes.len()
                ),
                "the origin serves another file; try again, or another origin",
            ));
        }
        fs::rename(&part, &tar)?;
    }
    let dir = work.join("bundle");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    sh(Command::new(find_bin("tar")).arg("-xf").arg(&tar).arg("-C").arg(&dir))?;
    Ok(dir)
}

/// Where a person's previews keep the last bundle, checked against the
/// release each time: a preview runs again and the file is tens of MB. Root
/// keeps none under `/oath`, which holds only what a package prescribes.
fn bundle_cache() -> Option<PathBuf> {
    if unsafe { libc::geteuid() } == 0 {
        return None;
    }
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .filter(|d| !d.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?
        .join("oath");
    fs::create_dir_all(&dir).ok()?;
    Some(dir.join("update.tar"))
}

pub(crate) fn fetch(url: &str, dest: &Path) -> Result<()> {
    let out = Command::new(find_bin("curl"))
        .args(["-fsSL", "--retry", "2", "-o"])
        .arg(dest)
        .arg(url)
        .output()
        .map_err(|e| Error::Msg(format!("curl: {e}")))?;
    if !out.status.success() {
        return Err(Error::Msg(format!("{url}: {}", String::from_utf8_lossy(&out.stderr).trim())));
    }
    Ok(())
}

/// In sync, and nothing on probation.
pub(crate) fn preflight(root: &Path, cat: &Catalog) -> Result<()> {
    let drift = cat.diff(None)?;
    if !drift.is_empty() {
        let ids: Vec<String> = drift.iter().map(|d| d.id.to_string()).collect();
        return Err(Error::hint(
            format!("this machine has changes not applied: {}", ids.join(" ")),
            "oath diff; apply or undo them first",
        ));
    }
    if oath_core::probation::read(root).is_some() {
        return Err(Error::hint("an apply is on probation", "keep or undo it first (oath log)"));
    }
    Ok(())
}

/// Every pack the moves pin that the store lacks, fetched before this
/// machine changes: a fetch that fails leaves it as it was, and the next
/// run starts with what came.
fn prefetch(root: &Path, cat: &Catalog, moves: &[Move]) -> Result<()> {
    let mut want = Vec::new();
    for m in moves {
        let Some(set) = m.fields().and_then(Value::as_object) else { continue };
        // No origin serves a recipe's tree: `build_recipes` makes it.
        if matches!(m, Move::Rebuild { .. }) {
            continue;
        }
        let id: ObjectId = m.id().parse()?;
        let mut d = cat.get(&id).map(|o| o.desired).unwrap_or_else(|_| json!({}));
        for (k, v) in set {
            d[k] = v.clone();
        }
        let hash = d["hash"].as_str().unwrap_or("");
        if d["present"].as_bool() != Some(false)
            && !hash.is_empty()
            && !oath_core::store_present(root, &id.name, hash)
        {
            want.push((id.name, d));
        }
    }
    if !want.is_empty() {
        eprintln!(">> fetch {} packs before anything changes", want.len());
    }
    for (name, d) in &want {
        let s = |k: &str| d[k].as_str().unwrap_or("").to_string();
        oath_core::prefetch_pkg_any(root, name, &s("url"), &s("file"), &s("hash")).map_err(|e| {
            Error::hint(
                e.to_string(),
                "nothing on this machine changed; `sudo oath update --confirm` again fetches the rest",
            )
        })?;
    }
    Ok(())
}

/// The names `--keep` gives (`pkg:<name>` or `<name>`), each a package
/// this machine has.
/// `keep`, and what a present cell needs that this release no longer pins
/// (R78): an update does not take a package from under a cell. A package
/// the release does pin is not held, since a cell runs with what is linked.
fn held_by_cells(
    cat: &Catalog,
    keep: &BTreeSet<String>,
    pinned: impl Fn(&str) -> bool,
) -> Result<BTreeSet<String>> {
    let mut held = keep.clone();
    for id in cat.ls(Some(oath_core::KIND_CELL))? {
        let Ok(cell) = serde_json::from_value::<oath_core::Cell>(cat.get(&id)?.desired) else {
            continue;
        };
        if cell.present {
            let names = cell.needs.iter().map(|n| n.trim_start_matches("pkg:").to_string());
            held.extend(names.filter(|n| !pinned(n)));
        }
    }
    Ok(held)
}

fn keep_names(machine: &BTreeMap<String, Value>, keep: &[String]) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for k in keep.iter().filter(|k| !k.starts_with("svc:")) {
        let name = k.strip_prefix("pkg:").unwrap_or(k);
        if !machine.contains_key(name) {
            return Err(Error::hint(
                format!("--keep {k}: this machine has no pkg:{name}"),
                "oath ls --kind pkg",
            ));
        }
        out.insert(name.to_string());
    }
    Ok(out)
}

/// The packages as the moves leave them, by id, a discovered pin filled in
/// from actual.
fn moved_docs(cat: &Catalog, moves: &[Move]) -> Result<BTreeMap<String, Value>> {
    let mut docs = BTreeMap::new();
    for id in cat.ls(Some("pkg"))? {
        let o = cat.get(&id)?;
        let mut d = o.desired;
        if d["hash"].as_str().unwrap_or("").is_empty() {
            d["hash"] = o.actual.get("hash").cloned().unwrap_or_else(|| json!(""));
        }
        docs.insert(id.to_string(), d);
    }
    for m in moves {
        if m.kind() == "drop" {
            docs.remove(&m.id());
            continue;
        }
        let Some(fields) = m.fields().and_then(Value::as_object) else { continue };
        let d = docs.entry(m.id()).or_insert_with(|| json!({}));
        for (k, v) in fields {
            d[k] = v.clone();
        }
    }
    Ok(docs)
}

/// The packages as the moves leave them (`docs`), checked as `oath apply`
/// will check them, before anything changes: a recipe the update did not
/// move once still needed the old 32-bit Mesa (canto's Steam, 2026-10-29),
/// and the apply refused with the specs already set.
fn check_moved_needs(docs: &BTreeMap<String, Value>) -> Result<()> {
    let mut pkgs = Vec::new();
    for (id, d) in docs.clone() {
        if let Ok(p) = serde_json::from_value::<oath_core::Pkg>(d) {
            pkgs.push((id, p));
        }
    }
    oath_core::check_pkg_needs(&pkgs).map_err(|e| {
        let (message, hint) = match e {
            Error::Hint { message, hint } => (message, hint),
            other => (other.to_string(), String::new()),
        };
        Error::hint(
            format!("after these moves the apply would refuse: {message}"),
            format!("nothing on this machine changed; {hint}"),
        )
    })
}

/// No two of the packages as the moves leave them (`docs`) ship one name
/// in `/bin` or `/lib`, of the trees in the store: linking would refuse the
/// second half-way through the apply, with the first already moved. A
/// package split in two (R47) is the case: the old one kept (`--keep`)
/// still ships what the new one does.
fn check_links(root: &Path, docs: &BTreeMap<String, Value>) -> Result<()> {
    let pkgs: Vec<(String, String)> = docs
        .iter()
        .filter(|(_, d)| d["present"] != json!(false))
        .filter_map(|(id, d)| {
            let name = id.strip_prefix("pkg:")?;
            Some((name.to_string(), d["hash"].as_str()?.to_string()))
        })
        .collect();
    let clashes = oath_core::link_clashes(root, &pkgs);
    if clashes.is_empty() {
        return Ok(());
    }
    let list: Vec<String> =
        clashes.iter().map(|(n, a, b)| format!("{n} (pkg:{a} and pkg:{b})")).collect();
    Err(Error::hint(
        format!("after these moves two packages would ship one name: {}", list.join(", ")),
        "nothing on this machine changed; a package kept with --keep may still ship what the \
         output moved to another: update without that --keep",
    ))
}

/// Each recipe the moves rebuild, built from the output's plan with its
/// build needs at the output's pins, and required to be the output's pin,
/// before anything changes.
fn build_recipes(
    root: &Path,
    cat: &Catalog,
    bundle: &Path,
    output: &up::Output,
    moves: &[Move],
) -> Result<Vec<oath_core::BuildReport>> {
    let stage = bundle.join("stage/oath");
    let unchanged = |e: Error| {
        Error::hint(
            e.to_string(),
            "nothing on this machine changed; `sudo oath update --confirm` again",
        )
    };
    let mut built = Vec::new();
    for m in moves {
        let Move::Rebuild { name, to, .. } = m else { continue };
        let doc = stage.join("objects/plan").join(name).join("desired.json");
        let pin: Value = oath_core::read_json(&doc).map_err(|_| {
            Error::hint(
                format!("the output has no plan:{name} to build pkg:{name} {}", up::short(to)),
                "nothing on this machine changed",
            )
        })?;
        let path = stage
            .join("store/plan")
            .join(name)
            .join(pin["hash"].as_str().unwrap_or(""))
            .join(oath_core::PLAN_NAME);
        let (plan, _) = oath_core::lint_path(&path)?;
        for n in &plan.build_needs {
            let need = n.id.trim_start_matches("pkg:");
            if oath_core::store_present(root, need, &n.hash) {
                continue;
            }
            // A source's url is the output's, not the one this machine had.
            let doc = output
                .pins
                .iter()
                .find(|(p, _, _)| p == need)
                .map(|(_, _, d)| d.clone())
                .or_else(|| cat.get(&ObjectId::new("pkg", need)).ok().map(|o| o.desired))
                .unwrap_or_else(|| json!({}));
            let s = |k: &str| doc[k].as_str().unwrap_or("").to_string();
            oath_core::prefetch_pkg_any(root, need, &s("url"), &s("file"), &n.hash)
                .map_err(unchanged)?;
        }
        eprintln!(">> build plan:{name}, for pkg:{name} {}", up::short(to));
        let r = oath_core::build_file(root, &path).map_err(unchanged)?;
        if r.product != *to {
            return Err(Error::hint(
                format!(
                    "plan:{name} built {}, the output pins {}",
                    up::short(&r.product),
                    up::short(to)
                ),
                format!("nothing on this machine changed; oath logs plan:{name}"),
            ));
        }
        eprintln!("built pkg:{name} {}", up::short(&r.product));
        built.push(r);
    }
    Ok(built)
}

fn sshd_is_dropbear(cat: &Catalog) -> Result<bool> {
    let cur = cat.get(&ObjectId::new("svc", "sshd"))?.desired;
    Ok(cur.pointer("/exec/0").and_then(Value::as_str) == Some("/bin/dropbear"))
}

/// `svc:sshd` from dropbear to the output's OpenSSH, once PID 1 has made
/// sshd's config and host key (it carries dropbear's key over).
fn move_sshd(root: &Path, cat: &Catalog, actor: &Actor, bundle: &Path) -> Result<()> {
    if !sshd_is_dropbear(cat)? {
        return Ok(());
    }
    let ready = Path::new("/bin/sshd").exists()
        && [oath_core::SSHD_CONFIG, oath_core::SSH_HOST_KEY]
            .iter()
            .all(|p| fs::metadata(p).is_ok_and(|m| m.len() > 0));
    if !ready {
        eprintln!(
            "svc:sshd runs dropbear until this machine boots the output; then `sudo oath update \
             --confirm` moves it to OpenSSH"
        );
        return Ok(());
    }
    let want: Value =
        oath_core::read_json(&bundle.join("stage/oath/objects/svc/sshd/desired.json"))?;
    let id = ObjectId::new("svc", "sshd");
    let mut map = Map::new();
    map.insert("exec".into(), want["exec"].clone());
    eprintln!(">> svc:sshd: dropbear to OpenSSH");
    cat.set_fields(&id, map)?;
    let hooks = Live::new(root);
    let r =
        cat.apply_with(Some(vec![id]), true, oath_core::probation::DEFAULT_SECS, actor, &hooks)?;
    if let Some(secs) = r.probation {
        wait_kept(root, r.generation, secs)?;
        if sshd_is_dropbear(cat)? {
            return Err(Error::hint(
                "OpenSSH's sshd took no login in time: PID 1 put dropbear back",
                "run `sudo oath update --confirm` again and keep it from a new SSH session",
            ));
        }
    }
    Ok(())
}

/// Wait for `sudo oath keep` (or PID 1's undo) to end a probation.
fn wait_kept(root: &Path, generation: u64, secs: u64) -> Result<()> {
    eprintln!(
        "generation {generation} can cut off SSH and is on probation for {secs} s: open a NEW SSH \
         session and run `sudo oath keep` there; the update goes on once it is kept"
    );
    let deadline = Instant::now() + Duration::from_secs(secs + 30);
    while oath_core::probation::read(root).is_some() {
        if Instant::now() > deadline {
            return Err(Error::Msg(format!("generation {generation} is still on probation")));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    Ok(())
}

/// Each file written beside itself and renamed over, so a running program
/// keeps its old file and nothing is ever half there.
fn put_root_files(stage: &Path, files: &[&str]) -> Result<()> {
    let script = format!(
        "set -e\ncd {stage}\n\
         find {files} ! -type d | while read -r f; do\n\
         \tt=/$f\n\
         \tmkdir -p \"$(dirname \"$t\")\"\n\
         \tcp -a \"$f\" \"$t.oath-new\"\n\
         \tmv -f \"$t.oath-new\" \"$t\"\n\
         done\n\
         if test -e /lib/oath/sudo; then chown 0:0 /lib/oath/sudo; chmod 4755 /lib/oath/sudo; fi\n\
         mkdir -p {void}\nchown 0:0 {void}\nchmod 755 {void}\n",
        stage = sh_quote(&stage.display().to_string()),
        files =
            files.iter().filter(|f| stage.join(f).exists()).copied().collect::<Vec<_>>().join(" "),
        void = oath_core::seat::VOID_HOME,
    );
    sh(Command::new(find_bin("sh")).args(["-c", &script]))
}

fn finish(opts: &Opts, v: Value, text: &str) {
    if opts.json {
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        println!("{text}");
    }
}

pub(crate) fn sh(c: &mut Command) -> Result<()> {
    let st = c.status().map_err(|e| Error::Msg(format!("{c:?}: {e}")))?;
    if !st.success() {
        return Err(Error::Msg(format!("{c:?}: {st}")));
    }
    Ok(())
}

pub(crate) fn sh_out(c: &mut Command) -> Result<String> {
    let out = c.output().map_err(|e| Error::Msg(format!("{c:?}: {e}")))?;
    if !out.status.success() {
        return Err(Error::Msg(format!("{c:?}: {}", String::from_utf8_lossy(&out.stderr).trim())));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub(crate) fn is_mounted(p: &Path) -> bool {
    let p = p.display().to_string();
    fs::read_to_string("/proc/mounts")
        .unwrap_or_default()
        .lines()
        .any(|l| l.split_whitespace().nth(1) == Some(p.as_str()))
}

pub(crate) fn cmdline_val(cmdline: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    cmdline.split_whitespace().find_map(|t| t.strip_prefix(&prefix).map(str::to_string))
}

/// Partition 1 of the disk that holds `root` (`/dev/sda2` → `/dev/sda1`,
/// `/dev/nvme0n1p2` → `/dev/nvme0n1p1`).
pub(crate) fn esp_of(root: &str) -> Option<String> {
    let disk = root.trim_end_matches(|c: char| c.is_ascii_digit());
    if disk == root || !disk.starts_with("/dev/") {
        return None;
    }
    Some(format!("{disk}1"))
}

/// The kernel options of the current entry that `esp::kernel_options`
/// does not write itself.
pub(crate) fn extra_options(current: &str) -> String {
    let own = esp::kernel_options("", "", "");
    let own: Vec<&str> = own.split_whitespace().filter(|t| !t.starts_with("oath.")).collect();
    current
        .split_whitespace()
        .filter(|t| {
            !own.contains(t) && !t.starts_with("oath.root=") && !t.starts_with("oath.subvol=")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_cell_needs_stays_unless_the_release_pins_it() {
        let d = tempfile::tempdir().unwrap();
        oath_core::seed(d.path()).unwrap();
        let cat = Catalog::open(d.path()).unwrap();
        let hash = format!("sha256-{}", "a".repeat(64));
        let doc = |present: bool, needs: Value| {
            json!({ "present": present, "hash": hash, "needs": needs }).as_object().unwrap().clone()
        };
        let on: ObjectId = "cell:on".parse().unwrap();
        let off: ObjectId = "cell:off".parse().unwrap();
        cat.new_object(&on, doc(true, json!(["pkg:gone", "pkg:pinned"]))).unwrap();
        cat.new_object(&off, doc(false, json!(["pkg:idle"]))).unwrap();
        let keep = BTreeSet::from(["kept".to_string()]);
        let held = held_by_cells(&cat, &keep, |n| n == "pinned").unwrap();
        assert_eq!(held, BTreeSet::from(["gone".to_string(), "kept".to_string()]));
    }

    #[test]
    fn what_waits_for_a_boot_is_said() {
        let (w, r) = ("next-boot", "runs");
        assert_eq!(next_boot(r, r, r), None);
        assert_eq!(next_boot(r, w, r), Some("reboot to load its firmware"));
        assert_eq!(next_boot(r, w, w), Some("reboot to load its firmware and run its PID 1"));
        assert_eq!(next_boot(r, r, "re-executed"), None);
        assert_eq!(next_boot(r, w, "re-executed"), Some("reboot to load its firmware"));
        assert_eq!(next_boot(w, w, r), Some("reboot to run its kernel and initrd"));
        assert_eq!(next_boot(w, r, w), Some("reboot to run its kernel, initrd and PID 1"));
        assert_eq!(next_boot(r, r, w), Some("reboot to run its PID 1"));
    }

    #[test]
    fn esp_is_partition_one_of_the_root_disk() {
        assert_eq!(esp_of("/dev/sda2").as_deref(), Some("/dev/sda1"));
        assert_eq!(esp_of("/dev/vda2").as_deref(), Some("/dev/vda1"));
        assert_eq!(esp_of("/dev/nvme0n1p2").as_deref(), Some("/dev/nvme0n1p1"));
        assert_eq!(esp_of("/dev/sda"), None);
    }

    #[test]
    fn prefetch_takes_every_pack_before_anything_changes() {
        let d = tempfile::tempdir().unwrap();
        let tree = d.path().join("tree");
        fs::create_dir_all(tree.join("bin")).unwrap();
        fs::write(tree.join("bin/hello"), b"hello\n").unwrap();
        let pin = oath_core::hash_tree(&tree).unwrap();
        let origin = d.path().join("origin");
        fs::create_dir_all(origin.join("pkg/hello")).unwrap();
        let tar = origin.join("pkg/hello").join(format!("{pin}.tar"));
        sh(Command::new("tar").arg("-C").arg(&tree).arg("-cf").arg(&tar).arg(".")).unwrap();
        let root = d.path().join("cat");
        oath_core::seed(&root).unwrap();
        let host = root.join("objects/host/local/desired.json");
        let mut h: Value = oath_core::read_json(&host).unwrap();
        h["origins"] = json!([format!("file://{}", origin.display())]);
        oath_core::write_json(&host, &h).unwrap();
        let cat = Catalog::open(&root).unwrap();
        let pin_to = |name: &str, to: &str| Move::Pin {
            name: name.into(),
            from: String::new(),
            to: to.into(),
            set: json!({ "hash": to }),
            overlay: None,
        };
        let gone = "sha256-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        // One the origin lacks: an error that says nothing changed, and the
        // pack the origin has is already in the store for the next run.
        let e = prefetch(&root, &cat, &[pin_to("hello", &pin), pin_to("gone", gone)])
            .unwrap_err()
            .to_string();
        assert!(e.contains("no origin served it"), "{e}");
        assert!(oath_core::store_present(&root, "hello", &pin));
        assert!(cat.get(&"pkg:hello".parse().unwrap()).is_err());
        // Removed packages and pins the store has fetch nothing.
        let off = Move::Pin {
            name: "gone".into(),
            from: String::new(),
            to: gone.into(),
            set: json!({ "hash": gone, "present": false }),
            overlay: None,
        };
        prefetch(&root, &cat, &[pin_to("hello", &pin), off]).unwrap();
    }

    #[test]
    fn extra_options_keep_only_what_the_entry_added() {
        let cur = esp::kernel_options("/dev/sda2", "@", "quiet amdgpu.dc=0");
        assert_eq!(extra_options(&cur), "quiet amdgpu.dc=0");
        assert_eq!(
            cmdline_val("a oath.root=/dev/vda2 b", "oath.root").as_deref(),
            Some("/dev/vda2")
        );
    }
}
