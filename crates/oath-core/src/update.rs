//! Pull updates (R38). A build host publishes each output it checked to
//! the origin: `out/<id>/update.tar` (the bundle: the output's seed
//! objects and plans, the image's own files, kernel, initrd and
//! oath-efi), `out/<id>.json` (the release: what the output is and the
//! bundle's hash), and `out/<profile>.json`, the newest release for that
//! profile, each `.json` with an OpenSSH signature beside it (`.sig`,
//! namespace `oath-output`). `oath update` on a machine takes the newest
//! release a key in `host:local.publishers` signed, and this module says
//! how its packages move to the output.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{Error, Result};

/// The signature namespace of a release (`ssh-keygen -Y sign -n`).
pub const NAMESPACE: &str = "oath-output";
/// Under the catalog root: the release this machine runs.
pub const RECORD: &str = "output.json";
/// The accounts, written before the packages move: `/etc/passwd` names
/// `pkg:zsh`'s shell (R27), and the services an update restarts drop to
/// `void` (R36).
pub const ACCOUNT_FILES: &[&str] = &["etc/passwd", "etc/group", "etc/shadow"];
/// The image's files outside `/oath` that an update replaces. The rest of
/// the root tree is the catalog's (`/bin` links, `/etc/hosts`) or the
/// machine's own.
pub const ROOT_FILES: &[&str] = &[
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
/// The bundle's boot files (beside `stage/` and `manifest.json`).
pub const KERNEL: &str = "bzImage";
pub const INITRD: &str = "initrd.gz";
pub const EFI: &str = "BOOTX64.EFI";

/// What a build host signs for an output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    /// The output's id (12 hex digits of its manifest's hash).
    pub output: String,
    /// The seed profile it was built from; a machine follows one.
    pub profile: String,
    /// When it was published (RFC 3339, UTC). Empty in the record an
    /// install writes (`/oath/output.json`) for an output never published.
    #[serde(default)]
    pub published: String,
    /// When the commit it was built from was made (RFC 3339, UTC): the
    /// order of outputs, published or not ([`newer`]). Releases published
    /// before 2026-10-31 have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed: Option<String>,
    /// `out/<output>/update.tar`; empty in an install's record.
    #[serde(default)]
    pub bundle: Bundle,
    /// The hash of its PID 1 (`/lib/oath/init`), to check after the reboot.
    pub init: String,
    /// The pins of the packages it pins from elsewhere (R28), by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pinned: BTreeMap<String, String>,
    /// The commit it was built from (40 hex), when the build host's git
    /// knew it. Releases from before R47 have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Every package its seed pins, by name: a machine that runs another
    /// pin of one runs an overlay (R47).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pins: BTreeMap<String, String>,
    /// Every object its seed has (`kind:name`, generations and devices
    /// aside): the next update makes what is new to the profile since, and
    /// not what the owner removed (R47).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seed: Vec<String>,
}

/// What a machine knows of the release it runs (`/oath/output.json`).
#[derive(Clone, Debug, Default)]
pub struct Ran {
    pins: BTreeMap<String, String>,
    seed: Option<BTreeSet<String>>,
    pinned: bool,
}

impl Ran {
    pub fn of(record: Option<&Release>) -> Self {
        let Some(r) = record else { return Self::default() };
        let mut pins = r.pinned.clone();
        pins.extend(r.pins.clone());
        let seed = (!r.seed.is_empty()).then(|| r.seed.iter().cloned().collect());
        Self { pins, seed, pinned: !r.pinned.is_empty() }
    }

    /// The pin that release set for `name`, when its record says.
    pub fn pin(&self, name: &str) -> Option<&str> {
        self.pins.get(name).map(String::as_str)
    }

    /// The release's pin of `name` when this machine runs another, `hash`:
    /// an overlay (R47). None when it is the release's, or the record does
    /// not say.
    pub fn overlay(&self, name: &str, hash: &str) -> Option<&str> {
        self.pin(name).filter(|p| !hash.is_empty() && *p != hash)
    }

    /// Whether `id` came into the profile after the release this machine
    /// runs: an object the machine lacks for that reason is made, while
    /// one that release had and the machine lacks was its owner's to
    /// remove. A record with no seed list (before R47) tells only for
    /// packs pinned from elsewhere (`elsewhere`), by its `pinned`; with
    /// neither, nothing is new.
    pub fn is_new(&self, id: &str, elsewhere: bool) -> bool {
        match &self.seed {
            Some(seed) => !seed.contains(id),
            None => {
                elsewhere
                    && self.pinned
                    && id.strip_prefix("pkg:").is_some_and(|n| !self.pins.contains_key(n))
            }
        }
    }
}

/// The `source` a package doc records, as a person reads it.
pub fn source_label(doc: &Value) -> Option<String> {
    let s: crate::PkgSource = serde_json::from_value(doc.get("source")?.clone()).ok()?;
    Some(s.label())
}

/// Whether `ran`, the release this machine runs, is newer than `next`: by
/// the times of the commits they were built from when both say, else by
/// when they were published. None when neither tells: an install's record
/// of an output never published, against a release from before
/// `committed`. A machine takes no older release, and none of unknown
/// order, unless its owner names it (`oath update --output`).
pub fn newer(ran: &Release, next: &Release) -> Option<bool> {
    match (&ran.committed, &next.committed) {
        (Some(a), Some(b)) => Some(a > b),
        _ if !ran.published.is_empty() => Some(ran.published > next.published),
        _ => None,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bundle {
    /// `sha256-<hex>` of the tar file.
    pub hash: String,
    pub size: u64,
}

/// `out/<name>.json`: an output id, or a profile for the newest.
pub fn release_key(name: &str) -> String {
    format!("out/{name}.json")
}

pub fn bundle_key(output: &str) -> String {
    format!("out/{output}/update.tar")
}

/// An output id: 12 lowercase hex digits.
pub fn is_output_id(s: &str) -> bool {
    s.len() == 12 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// What happens to one package.
#[derive(Clone, Debug, PartialEq)]
pub enum Move {
    /// To the output's pin: `set` is its hash, needs and `source` (and a
    /// recipe's `url` and `file`). `overlay` names what the machine ran
    /// when that was not the pin of the release it runs (R47): the update
    /// replaces it.
    Pin { name: String, from: String, to: String, set: Value, overlay: Option<String> },
    /// The owner said to keep the machine's pin (`oath update --keep`):
    /// `set` moves only its needs, when any moved.
    Keep { name: String, pin: String, output: String, set: Option<Value>, moved: Vec<String> },
    /// Not in the output (one the owner added, `pkg:dropbear` after R36):
    /// its pin stays and its needs move.
    Stay { name: String, set: Value, moved: Vec<String> },
    /// The machine lacks it and the output has it: made, because the
    /// output fixes it (the image's own files need it: `/etc/passwd` names
    /// `pkg:zsh`'s shell), it is new to the profile (R47), or a package the
    /// machine has needs it. `why` says which.
    New { name: String, doc: Value, why: &'static str },
    /// A recipe (R23) the owner built: the output carries no tree, so the
    /// update builds the output's plan on the machine before anything
    /// changes, then moves it like a pin (`set`).
    Rebuild { name: String, from: String, to: String, set: Value },
}

impl Move {
    pub fn id(&self) -> String {
        let (Self::Pin { name, .. }
        | Self::Keep { name, .. }
        | Self::Stay { name, .. }
        | Self::New { name, .. }
        | Self::Rebuild { name, .. }) = self;
        format!("pkg:{name}")
    }

    /// The fields `oath set` writes (or `oath new` makes), if any.
    pub fn fields(&self) -> Option<&Value> {
        match self {
            Self::Pin { set, .. } | Self::Stay { set, .. } | Self::Rebuild { set, .. } => {
                Some(set)
            }
            Self::Keep { set, .. } => set.as_ref(),
            Self::New { doc, .. } => Some(doc),
        }
    }

    pub fn describe(&self) -> String {
        let id = self.id();
        match self {
            Self::Pin { to, set, overlay: Some(was), .. } => format!(
                "{id} is {was}, not the release's: the update replaces it with {}{}; \
                 `--keep {id}` keeps it",
                short(to),
                source_label(set).map(|l| format!(" ({l})")).unwrap_or_default()
            ),
            Self::Pin { from, to, set, .. } if from == to => {
                let what = match (set.get("needs").is_some(), set.get("source").is_some()) {
                    _ if set["present"] == json!(true) => "made present: new to the profile",
                    (true, true) => "its needs and source move",
                    (false, true) => "its source moves",
                    _ => "its needs move",
                };
                format!("{id} {} ({what})", short(to))
            }
            Self::Pin { from, to, set, .. } if set["present"] == json!(true) => format!(
                "{id} {} -> {} (made present: new to the profile)",
                short(from),
                short(to)
            ),
            Self::Pin { from, to, .. } => format!("{id} {} -> {}", short(from), short(to)),
            Self::Keep { pin, output, moved, .. } => format!(
                "{id} keeps this machine's pin {} (--keep; the output has {}){}",
                short(pin),
                short(output),
                needs_moved(moved)
            ),
            Self::Stay { moved, .. } => {
                format!("{id} is not in the output and keeps its pin{}", needs_moved(moved))
            }
            Self::New { doc, why, .. } => {
                format!("{id} {} is made ({why})", short(doc["hash"].as_str().unwrap_or("")))
            }
            Self::Rebuild { name, from, to, .. } => format!(
                "{id} {} -> {} (a recipe: plan:{name} builds it here first)",
                short(from),
                short(to)
            ),
        }
    }
}

fn needs_moved(moved: &[String]) -> String {
    if moved.is_empty() {
        String::new()
    } else {
        format!("; its needs move: {}", moved.join(", "))
    }
}

/// `sha256-` and the first 12 hex digits.
pub fn short(hash: &str) -> &str {
    &hash[..hash.len().min(19)]
}

/// The output as a machine sees it: its packages' pins and seed docs, the
/// ones it fixes, the ones packed elsewhere (R28), and which packs it
/// carries (published beside it).
pub struct Output {
    pub pins: Vec<(String, String, Value)>,
    pub fixed: BTreeSet<String>,
    pub pinned: BTreeSet<String>,
    pub packs: BTreeMap<String, String>,
}

impl Output {
    /// From an unpacked bundle: `manifest.json` and `stage/oath/objects`.
    pub fn read(bundle: &Path) -> Result<Self> {
        let manifest: Value = crate::read_json(&bundle.join("manifest.json"))?;
        let pinned = manifest["pinned"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n.as_str().map(str::to_string))
            .collect();
        let packs = manifest["packs"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(n, p)| Some((n.clone(), p["hash"].as_str()?.to_string())))
            .collect();
        let dir = bundle.join("stage/oath/objects/pkg");
        let mut pins = Vec::new();
        let mut fixed = BTreeSet::new();
        for e in fs::read_dir(&dir).map_err(|e| Error::Msg(format!("{}: {e}", dir.display())))? {
            let e = e?;
            let name = e.file_name().to_string_lossy().into_owned();
            let v: Value = crate::read_json(&e.path().join("desired.json"))?;
            let meta: Value = crate::read_json(&e.path().join("meta.json")).unwrap_or(Value::Null);
            if meta["fixed"] == json!(true) {
                fixed.insert(name.clone());
            }
            if let Some(h) = v["hash"].as_str().filter(|h| !h.is_empty()) {
                pins.push((name, h.to_string(), v.clone()));
            }
        }
        pins.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Self { pins, fixed, pinned, packs })
    }

    fn pin(&self, name: &str) -> Option<&str> {
        self.pins.iter().find(|(n, _, _)| n == name).map(|(_, h, _)| h.as_str())
    }
}

/// Every object the bundle's seed made, generations and devices aside.
pub fn seed_ids(bundle: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for kind in fs::read_dir(bundle.join("stage/oath/objects"))? {
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

/// How the machine's packages (`machine`: name to desired doc) move to
/// `output`. `have(name, hash)`: the machine can get that tree (its store
/// has it, or an origin serves it). Each package takes the output's pin,
/// needs and `source`, a pin the machine runs that is not the release's
/// (an overlay, by `ran`) too, named so in the move (R47); a package whose
/// own tree is unchanged still moves its needs when what it runs with was
/// rebuilt. One the owner keeps (`keep`, by name) keeps the machine's pin,
/// and a need on it stays at that pin. A package the machine lacks is made
/// when the output fixes it, when it is new to the profile since the
/// release the machine runs, or when a package the machine has present
/// needs it in the output (and so on down its needs), so the needs graph
/// holds: R41's Mesa left libEGL to `pkg:libglvnd`, which wlroots and
/// Xwayland then need. One that release had and the machine lacks stays
/// removed.
pub fn plan(
    machine: &BTreeMap<String, Value>,
    output: &Output,
    ran: &Ran,
    keep: &BTreeSet<String>,
    have: impl Fn(&str, &str) -> bool,
) -> Vec<Move> {
    let pin = |doc: &Value| doc["hash"].as_str().unwrap_or("").to_string();
    let kept: BTreeMap<String, String> = output
        .pins
        .iter()
        .filter_map(|(n, p, _)| {
            let mine = pin(machine.get(n)?);
            (keep.contains(n) && mine != *p).then(|| (n.clone(), mine))
        })
        .collect();
    let ships = needed_at(&kept, |n: &str| output.pin(n).map(str::to_string));
    let mut moves = Vec::new();
    for (name, to, doc) in &output.pins {
        let Some(cur) = machine.get(name) else {
            let why = if output.fixed.contains(name) {
                "the output fixes it"
            } else if ran.is_new(&format!("pkg:{name}"), output.pinned.contains(name)) {
                "new to the profile"
            } else {
                continue;
            };
            moves.push(Move::New { name: name.clone(), doc: doc.clone(), why });
            continue;
        };
        let mine = pin(cur);
        if kept.contains_key(name) {
            let (set, moved) = match keep_machine_pin(cur, &ships) {
                Some((s, m)) => (Some(s), m),
                None => (None, Vec::new()),
            };
            moves.push(Move::Keep {
                name: name.clone(),
                pin: mine,
                output: to.clone(),
                set,
                moved,
            });
            continue;
        }
        // A need on a pack the machine keeps stays at the machine's pin.
        let mut needs = doc.get("needs").cloned().unwrap_or_else(|| json!([]));
        for n in needs.as_array_mut().into_iter().flatten() {
            let need = n["id"].as_str().unwrap_or("").trim_start_matches("pkg:");
            if let Some(k) = kept.get(need) {
                n["hash"] = json!(k);
            }
        }
        let mut set = json!({ "hash": to, "needs": needs });
        for k in ["url", "file", "source"] {
            if let Some(v) = doc.get(k) {
                set[k] = v.clone();
            }
        }
        // New to the profile, yet here: absent, as `oath build` leaves a
        // product it made (novus's Sola apps, 2026-10-31). It is present
        // as the profile has it; after that release, `present` is the
        // owner's again.
        // Never the other way: one the owner linked before the profile
        // had it (a recipe it ships absent) stays linked.
        if ran.is_new(&format!("pkg:{name}"), output.pinned.contains(name))
            && doc["present"] != json!(false)
            && cur["present"] == json!(false)
        {
            set["present"] = json!(true);
        }
        // The release records no source: neither does the machine after.
        if set.get("source").is_none() && cur.get("source").is_some_and(|v| !v.is_null()) {
            set["source"] = Value::Null;
        }
        let none = json!([]);
        let same = |k: &str, v: &Value| {
            cur.get(k).unwrap_or(if k == "needs" { &none } else { &Value::Null }) == v
        };
        if set.as_object().into_iter().flatten().all(|(k, v)| same(k, v)) {
            continue;
        }
        if mine == *to {
            // Only its record moves: say which part.
            if let Some(m) = set.as_object_mut() {
                let cur = cur.clone();
                m.retain(|k, v| {
                    k == "hash"
                        || cur.get(k.as_str()).unwrap_or(if k == "needs" {
                            &none
                        } else {
                            &Value::Null
                        }) != v
                });
            }
        }
        if !have(name, to) && cur["present"] == json!(true) {
            moves.push(Move::Rebuild { name: name.clone(), from: mine, to: to.clone(), set });
            continue;
        }
        let overlay = ran
            .overlay(name, &mine)
            .filter(|_| mine != *to)
            .map(|_| source_label(cur).unwrap_or_else(|| short(&mine).to_string()));
        moves.push(Move::Pin { name: name.clone(), from: mine, to: to.clone(), set, overlay });
    }
    for (name, cur) in machine {
        if output.pin(name).is_some() {
            continue;
        }
        if let Some((set, moved)) = keep_machine_pin(cur, &ships) {
            moves.push(Move::Stay { name: name.clone(), set, moved });
        }
    }
    let doc_of = |n: &str| output.pins.iter().find(|(p, _, _)| p == n).map(|(_, _, d)| d);
    let needs_of = |doc: &Value| -> Vec<String> {
        doc.get("needs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|n| n.get("id").or(Some(n)).and_then(Value::as_str))
            .map(|id| id.trim_start_matches("pkg:").to_string())
            .collect()
    };
    let made: BTreeSet<String> = moves
        .iter()
        .filter_map(|m| match m {
            Move::New { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    let mut todo: Vec<String> = output
        .pins
        .iter()
        .filter(|(n, _, _)| {
            machine.get(n).is_some_and(|c| c["present"] != json!(false)) || made.contains(n)
        })
        .flat_map(|(_, _, doc)| needs_of(doc))
        .collect();
    let mut seen = made;
    while let Some(need) = todo.pop() {
        if machine.contains_key(&need) || !seen.insert(need.clone()) {
            continue;
        }
        let Some(doc) = doc_of(&need) else { continue };
        todo.extend(needs_of(doc));
        moves.push(Move::New { name: need, doc: doc.clone(), why: "a package here needs it" });
    }
    moves
}

/// What a machine's packs pinned from elsewhere need of `need` after an
/// update: a pack the machine keeps at its own pin (`kept`, by name) at
/// that pin, since its owner built them together; anything else at the
/// output's pin (`output`).
pub fn needed_at<'a>(
    kept: &'a BTreeMap<String, String>,
    output: impl Fn(&str) -> Option<String> + 'a,
) -> impl Fn(&str) -> Option<String> + 'a {
    move |need| kept.get(need).cloned().or_else(|| output(need))
}

/// The needs of a package that keeps the machine's pin: each one the
/// output ships at another hash moves to it. `None` when nothing moves.
/// `machine` is the machine's desired doc; `ships(need)` is the output's pin.
pub fn keep_machine_pin(
    machine: &Value,
    ships: impl Fn(&str) -> Option<String>,
) -> Option<(Value, Vec<String>)> {
    let mut needs = machine["needs"].as_array().cloned().unwrap_or_default();
    let mut moved = Vec::new();
    for n in needs.iter_mut() {
        let Some(id) = n["id"].as_str().map(str::to_string) else { continue };
        let name = id.trim_start_matches("pkg:");
        if let Some(s) = ships(name).filter(|s| n["hash"].as_str() != Some(s.as_str())) {
            moved.push(format!("{id} {} -> {s}", n["hash"].as_str().unwrap_or("")));
            n["hash"] = json!(s);
        }
    }
    (!moved.is_empty()).then(|| (json!({ "needs": needs }), moved))
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "sha256-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "sha256-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    fn release(published: &str, committed: Option<&str>) -> Release {
        Release {
            output: "000000000000".into(),
            profile: "desk".into(),
            published: published.into(),
            committed: committed.map(Into::into),
            bundle: Bundle::default(),
            init: A.into(),
            pinned: BTreeMap::new(),
            commit: None,
            pins: BTreeMap::new(),
            seed: Vec::new(),
        }
    }

    fn none() -> (Ran, BTreeSet<String>) {
        (Ran::default(), BTreeSet::new())
    }

    #[test]
    fn releases_order_by_commit_then_publication() {
        let t = |s: &str| format!("2026-10-30T{s}:00:00Z");
        // Both built from known commits: the commit decides, not publication.
        let ran = release(&t("23"), Some(&t("20")));
        assert_eq!(newer(&ran, &release(&t("23"), Some(&t("21")))), Some(false));
        assert_eq!(newer(&ran, &release(&t("23"), Some(&t("19")))), Some(true));
        // A release from before `committed`: publication decides.
        let old = release(&t("14"), None);
        assert_eq!(newer(&release(&t("13"), None), &old), Some(false));
        assert_eq!(newer(&release(&t("15"), None), &old), Some(true));
        // An install of an output never published (novus, 2026-10-30): its
        // commit is newer than a release that has one, and of unknown order
        // against one that has none, which is refused, not taken.
        let installed = release("", Some(&t("20")));
        assert_eq!(newer(&installed, &release(&t("14"), Some(&t("13")))), Some(true));
        assert_eq!(newer(&installed, &old), None);
    }

    fn output() -> Output {
        let pins = vec![
            ("bash".into(), B.into(), json!({ "present": true, "hash": B })),
            ("cef".into(), C.into(), json!({ "present": true, "hash": C })),
            ("claude-code".into(), C.into(), json!({ "present": false, "hash": C })),
            (
                "sola".into(),
                B.into(),
                json!({ "present": true, "hash": B, "needs": [{ "id": "pkg:cef", "hash": C }] }),
            ),
            ("zsh".into(), B.into(), json!({ "present": true, "hash": B })),
        ];
        Output {
            pins,
            fixed: BTreeSet::from(["bash".into(), "zsh".into()]),
            pinned: BTreeSet::from(["sola".into()]),
            // A pack pinned from elsewhere is in the manifest's packs too.
            packs: BTreeMap::from([
                ("bash".into(), B.into()),
                ("cef".into(), C.into()),
                ("sola".into(), B.into()),
            ]),
        }
    }

    #[test]
    fn a_new_package_a_present_one_needs_is_made() {
        let need = |id: &str| json!({ "id": format!("pkg:{id}"), "hash": B });
        let pin = |name: &str, present: bool, needs: &[&str]| -> (String, String, Value) {
            let needs: Vec<Value> = needs.iter().map(|n| need(n)).collect();
            (name.into(), B.into(), json!({ "present": present, "hash": B, "needs": needs }))
        };
        let pins = vec![
            pin("bluez", true, &["readline"]),
            pin("cuda-x", true, &[]),
            pin("libglvnd", true, &[]),
            pin("nvidia", false, &["cuda-x"]),
            pin("pipewire", true, &["bluez"]),
            pin("readline", true, &[]),
            pin("unneeded", true, &[]),
            pin("wlroots", true, &["libglvnd"]),
        ];
        let (fixed, pinned, packs) = (BTreeSet::new(), BTreeSet::new(), BTreeMap::new());
        let out = Output { pins, fixed, pinned, packs };
        let machine = BTreeMap::from([
            ("nvidia".to_string(), json!({ "present": false, "hash": A })),
            ("pipewire".to_string(), json!({ "present": true, "hash": A })),
            ("wlroots".to_string(), json!({ "present": true, "hash": A })),
        ]);
        let (ran, keep) = none();
        let moves = plan(&machine, &out, &ran, &keep, |_, _| true);
        let mut made: Vec<&str> = moves
            .iter()
            .filter_map(|m| match m {
                Move::New { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        made.sort();
        // pipewire's bluez and its readline, wlroots' libglvnd; not what an
        // absent package needs, nor what nothing needs.
        assert_eq!(made, ["bluez", "libglvnd", "readline"]);
    }

    #[test]
    fn packages_move_to_the_output() {
        let machine = BTreeMap::from([
            ("bash".to_string(), json!({ "present": true, "hash": A })),
            ("cef".to_string(), json!({ "present": true, "hash": A })),
            ("claude-code".to_string(), json!({ "present": true, "hash": A })),
            (
                "sola".to_string(),
                json!({ "present": true, "hash": A, "needs": [{ "id": "pkg:cef", "hash": A }] }),
            ),
            (
                "dropbear".to_string(),
                json!({ "present": true, "hash": A, "needs": [{ "id": "pkg:bash", "hash": A }] }),
            ),
        ]);
        let out = output();
        let have = |n: &str, h: &str| out.packs.get(n).map(String::as_str) == Some(h);
        let (ran, keep) = none();
        let moves = plan(&machine, &out, &ran, &keep, have);
        let ids: Vec<String> = moves.iter().map(|m| m.describe()).collect();
        assert_eq!(moves.len(), 6, "{ids:#?}");
        assert!(matches!(&moves[0], Move::Pin { name, to, .. } if name == "bash" && to == B));
        assert!(matches!(&moves[1], Move::Pin { name, .. } if name == "cef"));
        // A recipe the owner built: the output carries no tree, and the
        // update builds it to the output's pin.
        let Move::Rebuild { name, set, .. } = &moves[2] else { panic!("{ids:#?}") };
        assert_eq!((name.as_str(), &set["hash"]), ("claude-code", &json!(C)));
        // Sola's own pack takes the output's pin and its need on cef.
        let Move::Pin { to, set, .. } = &moves[3] else { panic!("{ids:#?}") };
        assert_eq!((to.as_str(), &set["needs"][0]["hash"]), (B, &json!(C)));
        assert!(matches!(&moves[4], Move::New { name, .. } if name == "zsh"));
        let Move::Stay { name, set, .. } = &moves[5] else { panic!("{ids:#?}") };
        assert_eq!(name, "dropbear");
        assert_eq!(set["needs"][0]["hash"], json!(B));
        // At the output already: nothing moves.
        let done = BTreeMap::from([
            ("bash".to_string(), json!({ "present": true, "hash": B })),
            ("zsh".to_string(), json!({ "present": true, "hash": B })),
        ]);
        assert!(plan(&done, &out, &ran, &keep, |_, _| true).is_empty());
    }

    fn sola_and_browser() -> (Output, BTreeMap<String, Value>) {
        let mut out = output();
        out.pins.push((
            "sola-browser".into(),
            C.into(),
            json!({ "present": true, "hash": C, "needs": [{ "id": "pkg:sola", "hash": B }] }),
        ));
        out.pinned.insert("sola-browser".into());
        let machine = BTreeMap::from([
            ("sola".to_string(), json!({ "present": true, "hash": A })),
            (
                "sola-browser".to_string(),
                json!({ "present": true, "hash": A, "needs": [{ "id": "pkg:sola", "hash": A }] }),
            ),
        ]);
        (out, machine)
    }

    fn of(moves: &[Move], name: &str) -> Move {
        moves.iter().find(|m| m.id() == format!("pkg:{name}")).cloned().unwrap()
    }

    fn ran_with(pins: &[(&str, &str)], seed: &[&str]) -> Ran {
        let mut r = release("2026-10-31T00:00:00Z", None);
        r.pins = pins.iter().map(|(n, h)| (n.to_string(), h.to_string())).collect();
        r.seed = seed.iter().map(|s| s.to_string()).collect();
        Ran::of(Some(&r))
    }

    #[test]
    fn an_overlay_is_named_and_replaced() {
        let (out, machine) = sola_and_browser();
        let keep = BTreeSet::new();
        // At the pins the release it runs set: both move, as nothing special.
        let ran = ran_with(&[("sola", A), ("sola-browser", A)], &[]);
        let moves = plan(&machine, &out, &ran, &keep, |_, _| true);
        let Move::Pin { to, set, overlay, .. } = of(&moves, "sola") else { panic!("{moves:#?}") };
        assert_eq!((to.as_str(), &set["needs"][0]["hash"], overlay), (B, &json!(C), None));
        let Move::Pin { to, set, .. } = of(&moves, "sola-browser") else { panic!("{moves:#?}") };
        assert_eq!((to.as_str(), &set["needs"][0]["hash"]), (C, &json!(B)));
        // An overlay of sola, with the source its tool recorded: replaced,
        // and the move says what it was.
        let ran = ran_with(&[("sola", C), ("sola-browser", A)], &[]);
        let mut machine = machine;
        machine.get_mut("sola").unwrap()["source"] = json!({
            "repo": "https://github.com/moonlight-pm/Sola",
            "commit": "0d99e340".repeat(5),
            "branch": "sessions-anywhere",
            "dirty": false,
            "built_by": "home@novus",
            "built_at": "2026-10-31T12:00:00Z"
        });
        let moves = plan(&machine, &out, &ran, &keep, |_, _| true);
        let m = of(&moves, "sola");
        let Move::Pin { to, set, overlay, .. } = &m else { panic!("{moves:#?}") };
        assert_eq!((to.as_str(), overlay.as_deref()), (B, Some("sessions-anywhere@0d99e340")));
        // The release records no source for it, so the machine's goes.
        assert_eq!(set["source"], Value::Null);
        assert!(m.describe().starts_with(
            "pkg:sola is sessions-anywhere@0d99e340, not the release's: the update replaces it"
        ));
        // --keep keeps it, and the browser, which moves, needs it at that pin.
        let keep = BTreeSet::from(["sola".to_string()]);
        let moves = plan(&machine, &out, &ran, &keep, |_, _| true);
        let Move::Keep { pin, .. } = of(&moves, "sola") else { panic!("{moves:#?}") };
        assert_eq!(pin, A);
        let Move::Pin { set, .. } = of(&moves, "sola-browser") else { panic!("{moves:#?}") };
        assert_eq!(set["needs"][0]["hash"], json!(A));
        // A record that names no pins: everything takes the output's pins.
        let moves = plan(&machine, &out, &Ran::default(), &BTreeSet::new(), |_, _| true);
        assert!(matches!(of(&moves, "sola"), Move::Pin { overlay: None, .. }), "{moves:#?}");
        assert!(matches!(of(&moves, "sola-browser"), Move::Pin { .. }), "{moves:#?}");
    }

    #[test]
    fn a_release_source_moves_with_its_pin() {
        let src = json!({
            "repo": "https://github.com/moonlight-pm/Sola",
            "commit": "ce6408de".repeat(5),
            "branch": "master",
            "dirty": false,
            "built_by": "home@novus",
            "built_at": "2026-11-01T12:00:00Z"
        });
        let mut out = output();
        out.pins.retain(|(n, _, _)| n == "sola");
        out.pins[0].2["source"] = src.clone();
        let (ran, keep) = none();
        // Off the pin: the source comes with it.
        let machine = BTreeMap::from([("sola".to_string(), json!({ "present": true, "hash": A }))]);
        let moves = plan(&machine, &out, &ran, &keep, |_, _| true);
        let Move::Pin { set, .. } = of(&moves, "sola") else { panic!("{moves:#?}") };
        assert_eq!(set["source"], src);
        // At the pin with its needs, only the record moves, and says so.
        let cur = json!({ "present": true, "hash": B, "needs": [{ "id": "pkg:cef", "hash": C }] });
        let machine = BTreeMap::from([("sola".to_string(), cur)]);
        let moves = plan(&machine, &out, &ran, &keep, |_, _| true);
        let m = of(&moves, "sola");
        assert_eq!(m.describe(), format!("pkg:sola {} (its source moves)", short(B)));
        assert_eq!(m.fields().unwrap(), &json!({ "hash": B, "source": src }));
    }

    #[test]
    fn what_is_new_to_the_profile_is_made_and_what_was_removed_is_not() {
        let mut out = output();
        let app = |n: &str| (n.to_string(), C.to_string(), json!({ "present": true, "hash": C }));
        out.pins.extend([app("sola-terminal"), app("sola-gone"), app("mesa")]);
        out.pinned.extend(["sola-terminal".to_string(), "sola-gone".to_string()]);
        let machine = BTreeMap::from([
            ("bash".to_string(), json!({ "present": true, "hash": B })),
            ("zsh".to_string(), json!({ "present": true, "hash": B })),
        ]);
        let keep = BTreeSet::new();
        let made = |ran: &Ran| -> Vec<String> {
            let mut v: Vec<String> = plan(&machine, &out, ran, &keep, |_, _| true)
                .iter()
                .filter_map(|m| match m {
                    Move::New { name, why, .. } => Some(format!("{name}: {why}")),
                    _ => None,
                })
                .collect();
            v.sort();
            v
        };
        // A record with the seed: what it lacked is new; what it had and
        // the machine lacks (sola-gone, cef) the owner removed.
        let ran = ran_with(&[], &["pkg:bash", "pkg:cef", "pkg:sola", "pkg:sola-gone", "pkg:zsh"]);
        assert_eq!(
            made(&ran),
            [
                "claude-code: new to the profile",
                "mesa: new to the profile",
                "sola-terminal: new to the profile"
            ]
        );
        // A record from before the seed list: its pins from elsewhere tell
        // for those alone.
        let mut r = release("2026-10-31T00:00:00Z", None);
        r.pinned = BTreeMap::from([("sola".into(), A.into()), ("sola-gone".into(), A.into())]);
        assert_eq!(made(&Ran::of(Some(&r))), ["sola-terminal: new to the profile"]);
        // No record: nothing is new.
        assert!(made(&Ran::default()).is_empty());
    }

    /// novus, 2026-10-31: `oath build` of a new Sola app left its object
    /// absent at the release's pin; the update makes it present, as new to
    /// the profile. A package the release it runs had stays as its owner
    /// left it.
    #[test]
    fn an_absent_package_new_to_the_profile_is_made_present() {
        let mut out = output();
        let app = json!({ "present": true, "hash": C, "needs": [{ "id": "pkg:cef", "hash": C }] });
        out.pins.push(("sola-terminal".into(), C.into(), app));
        out.pinned.insert("sola-terminal".into());
        let built = json!({ "present": false, "hash": C });
        let machine = BTreeMap::from([
            ("sola-terminal".to_string(), built),
            ("bash".to_string(), json!({ "present": false, "hash": B })),
        ]);
        let mut r = release("2026-10-31T00:00:00Z", None);
        r.pinned = BTreeMap::from([("sola".into(), B.into())]);
        let moves = plan(&machine, &out, &Ran::of(Some(&r)), &BTreeSet::new(), |_, _| true);
        let m = of(&moves, "sola-terminal");
        assert_eq!(m.fields().unwrap()["present"], json!(true), "{moves:#?}");
        assert_eq!(
            m.describe(),
            format!("pkg:sola-terminal {} (made present: new to the profile)", short(C))
        );
        // bash was the release's: absent here is the owner's choice.
        assert!(moves.iter().all(|m| m.id() != "pkg:bash"), "{moves:#?}");
        // A recipe new to the profile, shipped absent, that the owner linked
        // already: it stays linked.
        let mut out = output();
        out.pins.retain(|(n, _, _)| n == "claude-code");
        out.pinned.insert("claude-code".into());
        let mine = json!({ "present": true, "hash": C });
        let machine = BTreeMap::from([("claude-code".to_string(), mine)]);
        let moves = plan(&machine, &out, &Ran::of(Some(&r)), &BTreeSet::new(), |_, _| true);
        assert!(moves.iter().all(|m| m.fields().is_none_or(|f| f.get("present").is_none())));
    }

    #[test]
    fn a_kept_pin_moves_only_its_needs() {
        let machine = json!({
            "present": true,
            "hash": B,
            "needs": [{ "id": "pkg:wayland", "hash": B }, { "id": "pkg:mesa", "hash": C }]
        });
        let ships = |n: &str| matches!(n, "wayland" | "mesa").then(|| C.to_string());
        let (set, moved) = keep_machine_pin(&machine, ships).unwrap();
        assert_eq!(
            set,
            json!({ "needs": [{ "id": "pkg:wayland", "hash": C }, { "id": "pkg:mesa", "hash": C }] })
        );
        assert_eq!(moved.len(), 1);
        assert!(keep_machine_pin(&machine, |_| None).is_none());
        // Its need on another pack it keeps stays at the machine's pin of
        // that pack, and goes back to it if an update moved it.
        let kept = BTreeMap::from([("sola".to_string(), A.to_string())]);
        let ships = needed_at(&kept, |n: &str| match n {
            "sola" => Some(B.to_string()),
            "cef" => Some(C.to_string()),
            _ => None,
        });
        let browser = json!({
            "hash": C,
            "needs": [{ "id": "pkg:sola", "hash": A }, { "id": "pkg:cef", "hash": B }]
        });
        let (set, moved) = keep_machine_pin(&browser, &ships).unwrap();
        assert_eq!(
            set["needs"],
            json!([{ "id": "pkg:sola", "hash": A }, { "id": "pkg:cef", "hash": C }])
        );
        assert_eq!(moved, vec![format!("pkg:cef {B} -> {C}")]);
        let moved_wrong =
            json!({ "needs": [{ "id": "pkg:sola", "hash": B }, { "id": "pkg:cef", "hash": C }] });
        let (set, _) = keep_machine_pin(&moved_wrong, &ships).unwrap();
        assert_eq!(set["needs"][0]["hash"], json!(A));
    }

    #[test]
    fn release_names() {
        assert_eq!(release_key("desk"), "out/desk.json");
        assert_eq!(bundle_key("5d9bce056737"), "out/5d9bce056737/update.tar");
        assert!(is_output_id("5d9bce056737"));
        assert!(!is_output_id("desk") && !is_output_id("5D9BCE056737"));
        let r: Release = serde_json::from_value(json!({
            "output": "5d9bce056737",
            "profile": "desk",
            "published": "2026-10-28T00:00:00Z",
            "bundle": { "hash": A, "size": 1 },
            "init": B
        }))
        .unwrap();
        assert_eq!(r.bundle.size, 1);
    }
}
