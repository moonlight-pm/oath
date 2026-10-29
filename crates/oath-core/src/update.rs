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
    /// When it was published (RFC 3339, UTC). A machine takes no release
    /// older than the one it runs unless its owner names it.
    pub published: String,
    /// `out/<output>/update.tar`.
    pub bundle: Bundle,
    /// The hash of its PID 1 (`/lib/oath/init`), to check after the reboot.
    pub init: String,
    /// The pins of the packages it pins from elsewhere (R28), by name. A
    /// machine still at the pin the release it runs set moves with the next
    /// output; one its owner moved keeps its own.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pinned: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// To the output's pin: `set` is its hash and needs (and a recipe's
    /// `url` and `file`).
    Pin { name: String, from: String, to: String, set: Value },
    /// Packed elsewhere (R28) and moved by its owner off the pin the
    /// release the machine runs set: the machine keeps its own pin, its
    /// owner's to move; `set` moves only its needs, when any moved.
    Keep { name: String, pin: String, output: String, set: Option<Value>, moved: Vec<String> },
    /// Not in the output (one the owner added, `pkg:dropbear` after R36):
    /// its pin stays and its needs move.
    Stay { name: String, set: Value, moved: Vec<String> },
    /// The output fixes it and the machine lacks it: made, since the
    /// image's own files need it (`/etc/passwd` names `pkg:zsh`'s shell).
    New { name: String, doc: Value },
    /// A recipe (R23) the owner built: the output carries no tree, so it
    /// is built on the machine.
    Rebuild { name: String, to: String },
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
            Self::Pin { set, .. } | Self::Stay { set, .. } => Some(set),
            Self::Keep { set, .. } => set.as_ref(),
            Self::New { doc, .. } => Some(doc),
            Self::Rebuild { .. } => None,
        }
    }

    pub fn describe(&self) -> String {
        let id = self.id();
        match self {
            Self::Pin { from, to, .. } if from == to => {
                format!("{id} {} (its needs move)", short(to))
            }
            Self::Pin { from, to, .. } => format!("{id} {} -> {}", short(from), short(to)),
            Self::Keep { pin, output, moved, .. } => format!(
                "{id} keeps this machine's pin {} (packed elsewhere and moved here; the output \
                 has {}){}",
                short(pin),
                short(output),
                needs_moved(moved)
            ),
            Self::Stay { moved, .. } => {
                format!("{id} is not in the output and keeps its pin{}", needs_moved(moved))
            }
            Self::New { doc, .. } => format!(
                "{id} {} is made (the output fixes it)",
                short(doc["hash"].as_str().unwrap_or(""))
            ),
            Self::Rebuild { name, to } => format!(
                "{id}: the output's recipe builds {to}; run `sudo oath build plan:{name}`, \
                 `sudo oath set {id} hash={to}` and `sudo oath apply {id}`"
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
/// has it, or an origin serves it). Each package takes the output's pin
/// and needs; a package whose own tree is unchanged still moves its needs
/// when what it runs with was rebuilt. A pack packed elsewhere moves too
/// while the machine's pin is the one the release it runs set (`ran`, its
/// `pinned`); one its owner moved keeps the machine's pin, and a need on
/// another such pack the machine keeps stays at that pin.
pub fn plan(
    machine: &BTreeMap<String, Value>,
    output: &Output,
    ran: &BTreeMap<String, String>,
    have: impl Fn(&str, &str) -> bool,
) -> Vec<Move> {
    let pin = |doc: &Value| doc["hash"].as_str().unwrap_or("").to_string();
    let owners_pin = |n: &str, mine: &str| {
        output.pinned.contains(n) && ran.get(n).map(String::as_str) != Some(mine)
    };
    let kept: BTreeMap<String, String> = output
        .pins
        .iter()
        .filter_map(|(n, p, _)| {
            let mine = pin(machine.get(n)?);
            (owners_pin(n, &mine) && mine != *p).then(|| (n.clone(), mine))
        })
        .collect();
    let ships = needed_at(&kept, |n: &str| output.pin(n).map(str::to_string));
    let mut moves = Vec::new();
    for (name, to, doc) in &output.pins {
        let Some(cur) = machine.get(name) else {
            if output.fixed.contains(name) {
                moves.push(Move::New { name: name.clone(), doc: doc.clone() });
            }
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
        for k in ["url", "file"] {
            if let Some(v) = doc.get(k) {
                set[k] = v.clone();
            }
        }
        let none = json!([]);
        let differs = set.as_object().into_iter().flatten().any(|(k, v)| {
            cur.get(k).unwrap_or(if k == "needs" { &none } else { &Value::Null }) != v
        });
        if !differs {
            continue;
        }
        if !have(name, to) && cur["present"] == json!(true) {
            moves.push(Move::Rebuild { name: name.clone(), to: to.clone() });
            continue;
        }
        moves.push(Move::Pin { name: name.clone(), from: mine, to: to.clone(), set });
    }
    for (name, cur) in machine {
        if output.pin(name).is_some() {
            continue;
        }
        if let Some((set, moved)) = keep_machine_pin(cur, &ships) {
            moves.push(Move::Stay { name: name.clone(), set, moved });
        }
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
            packs: BTreeMap::from([("bash".into(), B.into()), ("cef".into(), C.into())]),
        }
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
        let moves = plan(&machine, &out, &BTreeMap::new(), have);
        let ids: Vec<String> = moves.iter().map(|m| m.describe()).collect();
        assert_eq!(moves.len(), 6, "{ids:#?}");
        assert!(matches!(&moves[0], Move::Pin { name, to, .. } if name == "bash" && to == B));
        assert!(matches!(&moves[1], Move::Pin { name, .. } if name == "cef"));
        // A recipe the owner built: the output carries no tree.
        assert!(matches!(&moves[2], Move::Rebuild { name, .. } if name == "claude-code"));
        // Sola's own pack keeps its pin; its need on cef moves.
        let Move::Keep { set: Some(set), pin, .. } = &moves[3] else { panic!("{ids:#?}") };
        assert_eq!(pin, A);
        assert_eq!(set["needs"][0]["hash"], json!(C));
        assert!(matches!(&moves[4], Move::New { name, .. } if name == "zsh"));
        let Move::Stay { name, set, .. } = &moves[5] else { panic!("{ids:#?}") };
        assert_eq!(name, "dropbear");
        assert_eq!(set["needs"][0]["hash"], json!(B));
        // At the output already: nothing moves.
        let done = BTreeMap::from([
            ("bash".to_string(), json!({ "present": true, "hash": B })),
            ("zsh".to_string(), json!({ "present": true, "hash": B })),
        ]);
        assert!(plan(&done, &out, &BTreeMap::new(), |_, _| true).is_empty());
    }

    #[test]
    fn a_pin_its_output_set_moves_with_the_next() {
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
        let ran = |sola: &str| {
            BTreeMap::from([("sola".to_string(), sola.into()), ("sola-browser".into(), A.into())])
        };
        let of = |moves: &[Move], name: &str| {
            moves.iter().find(|m| m.id() == format!("pkg:{name}")).cloned().unwrap()
        };
        // At the pins the release it runs set: both move to the output's.
        let moves = plan(&machine, &out, &ran(A), |_, _| true);
        let Move::Pin { to, set, .. } = of(&moves, "sola") else { panic!("{moves:#?}") };
        assert_eq!((to.as_str(), &set["needs"][0]["hash"]), (B, &json!(C)));
        let Move::Pin { to, set, .. } = of(&moves, "sola-browser") else { panic!("{moves:#?}") };
        assert_eq!((to.as_str(), &set["needs"][0]["hash"]), (C, &json!(B)));
        // Its owner moved sola: it keeps its pin, and the browser, which
        // moves, needs it at that pin.
        let moves = plan(&machine, &out, &ran(C), |_, _| true);
        let Move::Keep { pin, .. } = of(&moves, "sola") else { panic!("{moves:#?}") };
        assert_eq!(pin, A);
        let Move::Pin { set, .. } = of(&moves, "sola-browser") else { panic!("{moves:#?}") };
        assert_eq!(set["needs"][0]["hash"], json!(A));
        // A release that names no pins (from before it signed them) moves neither.
        let moves = plan(&machine, &out, &BTreeMap::new(), |_, _| true);
        assert!(matches!(of(&moves, "sola"), Move::Keep { .. }), "{moves:#?}");
        assert!(matches!(of(&moves, "sola-browser"), Move::Keep { .. }), "{moves:#?}");
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
