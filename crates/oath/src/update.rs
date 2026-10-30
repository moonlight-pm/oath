//! `oath update` (R38): move this machine to the newest output published
//! for its profile, from its origins, when a key it trusts signed it.
//!
//! 1. Fetch `out/<profile>.json` (or `out/<id>.json` for `--output`) and
//!    its signature from the first origin whose release a key in
//!    `host:local.publishers` signed. A release older than the one this
//!    machine runs (`/oath/output.json`) is refused unless named.
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

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use oath_core::esp;
use oath_core::update::{self as up, Move, Release};
use oath_core::{find_bin, Actor, ApplyHooks, Catalog, Error, ObjectId, Result};
use serde_json::{json, Map, Value};

use crate::live::{Follow, Live};

pub struct Opts {
    pub confirm: bool,
    pub output: Option<String>,
    pub reboot: bool,
    pub esp: Option<String>,
    pub json: bool,
}

pub fn run(root: &Path, cat: &Catalog, actor: &Actor, opts: Opts) -> Result<i32> {
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
    let profile = host["profile"].as_str().unwrap_or("").to_string();
    if profile.is_empty() {
        return Err(Error::hint(
            "host:local names no profile, so there is nothing to follow",
            "oath set host:local profile=<the profile it was installed from> (oath schema host)",
        ));
    }
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
    let current: Option<Release> = oath_core::read_json(&root.join(up::RECORD)).ok();
    eprintln!(
        "output {} for {profile}, published {} (signed by {signer}), from {origin}",
        release.output, release.published
    );
    let same = current.as_ref().is_some_and(|c| c.output == release.output);
    if let Some(c) = current.as_ref().filter(|_| !same && opts.output.is_none()) {
        if c.published > release.published {
            return Err(Error::hint(
                format!(
                    "this machine runs output {} (published {}), newer than {} ({})",
                    c.output, c.published, release.output, release.published
                ),
                format!("`oath update --output {}` takes it anyway", release.output),
            ));
        }
    }

    if same {
        let pid1 = if is_root {
            match oath_core::file_hash(&fs::read("/proc/1/exe")?) {
                h if h == release.init => "PID 1 is the output's",
                _ => "PID 1 is not the output's yet: reboot to run it",
            }
        } else {
            "run as root to compare PID 1"
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
        finish(
            &opts,
            json!({ "output": release.output, "runs": true }),
            &format!(
                "this machine runs output {}, the newest for {profile}; {pid1}",
                release.output
            ),
        );
        let _ = fs::remove_dir_all(&work);
        return Ok(0);
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
    let ran = current.as_ref().map(|c| c.pinned.clone()).unwrap_or_default();
    let moves = up::plan(&machine, &output, &ran, have);
    let plans = plan_moves(cat, &bundle)?;
    let have_ids: Vec<String> = cat.ls(None)?.iter().map(|i| i.to_string()).collect();
    let missing: Vec<String> = up::seed_ids(&bundle)?
        .into_iter()
        .filter(|id| !have_ids.contains(id))
        .filter(|id| !moves.iter().any(|m| m.id() == *id))
        .collect();
    for m in &moves {
        eprintln!("  {}", m.describe());
    }
    check_moved_needs(cat, &moves)?;
    for (name, from, to, _) in &plans {
        eprintln!("  plan:{name} {} -> {}", up::short(from), up::short(to));
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
            json!({ "output": release.output, "from": from, "moves": moves.iter().map(Move::describe).collect::<Vec<_>>() }),
            &format!(
                "{} packages move from {from} to output {}; `sudo oath update --confirm` \
                 moves this machine (add --reboot to boot it)",
                moves.iter().filter(|m| m.fields().is_some()).count(),
                release.output
            ),
        );
        return Ok(0);
    }

    // 4–6, every pack and recipe first.
    prefetch(root, cat, &moves)?;
    let built = build_recipes(root, cat, &bundle, &output, &moves)?;
    move_sshd(root, cat, actor, &bundle)?;
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
    let steps = Steps { moves: &moves, plans: &plans, built: &built, reboot: opts.reboot };
    let result = update(root, cat, actor, &bundle, &mnt, &root_dev, &steps);
    let _ = Command::new("sync").status();
    let _ = Command::new(find_bin("umount")).arg(&mnt).status();
    let generation = result?;
    oath_core::write_json(&root.join(up::RECORD), &release)?;
    let _ = fs::remove_dir_all(&work);
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
        finish(
            &opts,
            json!({ "output": release.output, "generation": generation }),
            &format!("{msg}; reboot to run its kernel, initrd and PID 1"),
        );
    }
    Ok(0)
}

/// What steps 4–6 move: the packages, the plans, the recipes built for
/// them, and whether a reboot follows.
struct Steps<'a> {
    moves: &'a [Move],
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
    let Steps { moves, plans, built, reboot } = *steps;
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
    let mut changed = Vec::new();
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
    let mut generation = 0;
    if !changed.is_empty() {
        eprintln!(">> apply {} packages", changed.len());
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
            let undone =
                changed.iter().zip(moves.iter().filter_map(Move::fields)).any(|(id, f)| {
                    let pin = cat.get(id).map(|o| o.desired.get("hash").cloned()).ok().flatten();
                    f.get("hash").is_some_and(|h| pin.as_ref() != Some(h))
                });
            if undone {
                let _ = Command::new(find_bin("btrfs"))
                    .args(["subvolume", "delete"])
                    .arg(&snap)
                    .status();
                return Err(Error::hint(
                    format!("generation {} was undone when its probation ran out", r.generation),
                    "nothing else changed; run `sudo oath update --confirm` again and keep it \
                     from a new SSH session",
                ));
            }
        }
    }

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

/// `(name, machine pin, output pin, plan file)` of each plan the machine
/// has whose pin the output moves (R23).
fn plan_moves(cat: &Catalog, bundle: &Path) -> Result<Vec<(String, String, String, PathBuf)>> {
    let mut out = Vec::new();
    let Ok(dir) = fs::read_dir(bundle.join("stage/oath/objects/plan")) else { return Ok(out) };
    for e in dir {
        let name = e?.file_name().to_string_lossy().into_owned();
        let id = ObjectId::new("plan", &name);
        let Ok(mine) = cat.get(&id) else { continue };
        let doc: Value = oath_core::read_json(
            &bundle.join("stage/oath/objects/plan").join(&name).join("desired.json"),
        )?;
        let Some(pin) = doc["hash"].as_str().filter(|h| !h.is_empty()) else { continue };
        let cur = mine.desired["hash"].as_str().unwrap_or("");
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

/// The bundle, checked against the release and unpacked into `work/bundle`.
fn fetch_bundle(origin: &str, r: &Release, work: &Path) -> Result<PathBuf> {
    let tar = work.join("update.tar");
    fetch(&format!("{origin}/{}", up::bundle_key(&r.output)), &tar)?;
    let size = fs::metadata(&tar)?.len();
    let hash = oath_core::file_hash(&fs::read(&tar)?);
    if size != r.bundle.size || hash != r.bundle.hash {
        return Err(Error::security(
            format!(
                "{} is not the bundle the release signed ({hash}, {size} bytes)",
                up::bundle_key(&r.output)
            ),
            "the origin serves another file; try again, or another origin",
        ));
    }
    let dir = work.join("bundle");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    sh(Command::new(find_bin("tar")).arg("-xf").arg(&tar).arg("-C").arg(&dir))?;
    let _ = fs::remove_file(&tar);
    Ok(dir)
}

fn fetch(url: &str, dest: &Path) -> Result<()> {
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

/// The packages as the moves leave them, checked as `oath apply` will check
/// them, before anything changes: a recipe the update did not move once
/// still needed the old 32-bit Mesa (canto's Steam, 2026-10-29), and the
/// apply refused with the specs already set.
fn check_moved_needs(cat: &Catalog, moves: &[Move]) -> Result<()> {
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
        let Some(fields) = m.fields().and_then(Value::as_object) else { continue };
        let d = docs.entry(m.id()).or_insert_with(|| json!({}));
        for (k, v) in fields {
            d[k] = v.clone();
        }
    }
    let mut pkgs = Vec::new();
    for (id, d) in docs {
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
