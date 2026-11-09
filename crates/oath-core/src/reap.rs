//! What a machine holds beyond what its packages prescribe (R62), and how
//! it goes. A version of a package stays while something names it: a pin in
//! the catalog, another package's need, the release the machine runs, a
//! current plan, or a generation kept for undo. The newest
//! [`GENERATIONS_KEPT`] generations are kept, and every one taken since this
//! boot: what was pinned since then may still be running (a seat's service
//! keeps its program across an update, and the kernel its modules), so this
//! is safe at any time, and at boot it leaves exactly what is prescribed. A
//! tree that arrived in the last day stays too: `oath store` and `oath
//! build` put it there, and its pin comes next. A cell's versions (R78) go
//! the same way, under `cell:<name>`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::{
    is_realization_id, probation, read_json, tree_size, Catalog, CatalogLock, ObjectId, Result,
    KIND_CELL, KIND_PKG,
};

/// Generations kept for undo, as the ESP keeps its boots.
pub const GENERATIONS_KEPT: usize = 5;
/// How long a tree no pin names yet may wait for one.
pub const ARRIVING_SECS: i64 = 24 * 60 * 60;
/// Trees on their way out: a rename under the lock, emptied after it.
const TRASH: &str = "store/.reap";

/// One version of a package in the store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Version {
    pub id: String,
    pub hash: String,
    /// When it reached this store (RFC 3339, UTC).
    pub arrived: String,
    /// Bytes of its files, when asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// What keeps it: `pin`, `live`, `release`, `arrived`, `needs:pkg:<name>`,
    /// `plan:<name>`, `generation:<n>`. Empty: it goes.
    pub held: Vec<String>,
}

impl Version {
    fn name(&self) -> &str {
        self.id.trim_start_matches("pkg:")
    }
}

/// What a machine holds, and what would go.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Reap {
    pub generations_kept: Vec<u64>,
    pub generations_gone: Vec<u64>,
    pub versions: Vec<Version>,
    /// Filed plans no plan object or kept generation pins: `(name, hash)`.
    pub plans_gone: Vec<(String, String)>,
    /// Store slots with no object and no version left.
    pub slots_gone: Vec<String>,
    /// Builds underway (`run/plan-build`): nothing goes while one runs.
    pub building: Vec<String>,
}

/// What [`survey`] measures against.
pub struct Policy {
    /// Seconds since the epoch.
    pub now: i64,
    /// A tree younger than this many seconds stays.
    pub arriving: i64,
    /// When this boot began (RFC 3339, UTC), to keep what was pinned since.
    pub boot: Option<String>,
    /// Add up each version's size, which reads every tree.
    pub sizes: bool,
}

/// A package's name, or `cell:<name>`, and a hash, to what keeps it.
type Holds = BTreeMap<(String, String), BTreeSet<String>>;

fn strs<'a>(v: &'a Value, keys: &[&str]) -> Vec<&'a str> {
    keys.iter().filter_map(|k| v.get(k)?.as_str()).filter(|s| !s.is_empty()).collect()
}

fn names_in(dir: &Path) -> Vec<String> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<String> =
        rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    out.sort();
    out
}

/// The specs of `kind:name` under a catalog root: desired, and applied.
fn specs(root: &Path, kind: &str, name: &str) -> Vec<Value> {
    let dir = root.join("objects").join(kind).join(name);
    ["desired.json", "applied.json"]
        .iter()
        .filter_map(|f| read_json::<Value>(&dir.join(f)).ok())
        .collect()
}

/// What a catalog's packages pin, each under `why`; its needs under `needs`.
fn hold_packages(
    root: &Path,
    holds: &mut Holds,
    why: &dyn Fn() -> String,
    needs: &dyn Fn(&str) -> String,
) {
    for name in names_in(&root.join("objects/pkg")) {
        for spec in specs(root, "pkg", &name) {
            for hash in strs(&spec, &["hash"]) {
                holds.entry((name.clone(), hash.to_string())).or_default().insert(why());
            }
            for n in spec["needs"].as_array().into_iter().flatten() {
                let (Some(id), Some(hash)) = (n["id"].as_str(), n["hash"].as_str()) else {
                    continue;
                };
                let need = id.trim_start_matches("pkg:").to_string();
                holds.entry((need, hash.to_string())).or_default().insert(needs(&name));
            }
        }
    }
    // A cell's needs are named, not pinned: the machine's pin holds those.
    for name in names_in(&root.join("objects/cell")) {
        for spec in specs(root, "cell", &name) {
            for hash in strs(&spec, &["hash"]) {
                holds.entry((format!("cell:{name}"), hash.to_string())).or_default().insert(why());
            }
        }
    }
}

/// Where a package or a `cell:<name>` keeps its versions.
fn slot_of(root: &Path, key: &str) -> PathBuf {
    match key.strip_prefix("cell:") {
        Some(name) => crate::cell::slot_dir(root, name),
        None => crate::slot_dir(root, key),
    }
}

/// The hashes of `plan:<name>` a catalog pins.
fn plan_pins(root: &Path) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for name in names_in(&root.join("objects/plan")) {
        for spec in specs(root, "plan", &name) {
            for hash in strs(&spec, &["hash"]) {
                out.insert((name.clone(), hash.to_string()));
            }
        }
    }
    out
}

fn plan_file(root: &Path, name: &str, hash: &str) -> PathBuf {
    root.join("store/plan").join(name).join(hash).join(crate::plan::PLAN_NAME)
}

fn rfc3339(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// When this boot began, from `/proc/stat`.
pub fn boot_time() -> Option<String> {
    let stat = fs::read_to_string("/proc/stat").ok()?;
    let secs = stat.lines().find_map(|l| l.strip_prefix("btime "))?.trim().parse().ok()?;
    Some(rfc3339(secs))
}

/// Look at the catalog `cat`, with its generations (`id`, the catalog root
/// in that snapshot), and say what holds each version and what would go.
pub fn survey(cat: &Catalog, generations: &[(u64, PathBuf)], policy: &Policy) -> Result<Reap> {
    let root = cat.root();
    let mut ids: Vec<u64> = generations.iter().map(|(id, _)| *id).collect();
    ids.sort_unstable_by(|a, b| b.cmp(a));
    let mut kept: BTreeSet<u64> = ids.iter().take(GENERATIONS_KEPT).copied().collect();
    if let Some(boot) = &policy.boot {
        for rec in cat.log_lines()? {
            if rec["time"].as_str().is_some_and(|t| t >= boot.as_str()) {
                kept.extend(rec["generation"].as_u64());
            }
        }
    }
    if let Some(p) = probation::read(root) {
        kept.insert(p.generation);
    }

    let mut holds = Holds::new();
    hold_packages(root, &mut holds, &|| "pin".to_string(), &|n| format!("needs:pkg:{n}"));
    for id in cat.ls(Some(KIND_PKG))? {
        let actual = cat.get(&id)?.actual;
        for hash in strs(&actual, &["hash"]) {
            holds.entry((id.name.clone(), hash.to_string())).or_default().insert("pin".into());
        }
    }
    for id in cat.ls(Some(KIND_CELL))? {
        let actual = cat.get(&id)?.actual;
        for hash in strs(&actual, &["hash"]) {
            holds.entry((id.to_string(), hash.to_string())).or_default().insert("pin".into());
        }
    }
    if let Ok(record) = read_json::<Value>(&root.join(crate::update::RECORD)) {
        for key in ["pins", "pinned"] {
            for (name, hash) in record[key].as_object().into_iter().flatten() {
                if let Some(h) = hash.as_str() {
                    let k = (name.clone(), h.to_string());
                    holds.entry(k).or_default().insert("release".into());
                }
            }
        }
    }
    let mut plans = plan_pins(root);
    for (name, hash) in &plans {
        let Ok((plan, _)) = crate::plan::lint_path(&plan_file(root, name, hash)) else { continue };
        for n in plan.build_needs.iter().chain(&plan.run_needs) {
            let k = (n.id.trim_start_matches("pkg:").to_string(), n.hash.clone());
            holds.entry(k).or_default().insert(format!("plan:{name}"));
        }
    }
    let mut generations_gone = Vec::new();
    for (id, at) in generations {
        if !kept.contains(id) {
            generations_gone.push(*id);
            continue;
        }
        let why = format!("generation:{id}");
        hold_packages(at, &mut holds, &|| why.clone(), &|_| why.clone());
        plans.extend(plan_pins(at));
    }
    generations_gone.sort_unstable();

    let mut versions = Vec::new();
    let mut slots_gone = Vec::new();
    let names = |area: &str| -> Vec<(String, String)> {
        let of = names_in(&root.join("store").join(area));
        of.into_iter().map(|n| (area.to_string(), n)).collect()
    };
    for (area, name) in names("pkg").into_iter().chain(names("cell")) {
        let key = if area == "cell" { format!("cell:{name}") } else { name.clone() };
        let slot = slot_of(root, &key);
        if !slot.is_dir() {
            continue;
        }
        let has_object = root.join("objects").join(&area).join(&name).is_dir();
        let live = fs::read_link(slot.join(crate::LIVE_NAME))
            .ok()
            .and_then(|l| l.file_name().map(|n| n.to_string_lossy().into_owned()))
            .filter(|_| has_object);
        let mut stays = false;
        for hash in names_in(&slot).into_iter().filter(|h| is_realization_id(h)) {
            let tree = slot.join(&hash);
            if !tree.is_dir() {
                continue;
            }
            let ctime = fs::metadata(&tree)?.ctime();
            let mut held: BTreeSet<String> =
                holds.get(&(key.clone(), hash.clone())).cloned().unwrap_or_default();
            if live.as_deref() == Some(hash.as_str()) {
                held.insert("live".into());
            }
            if policy.now - ctime < policy.arriving {
                held.insert("arrived".into());
            }
            stays |= !held.is_empty();
            versions.push(Version {
                id: format!("{area}:{name}"),
                size: policy.sizes.then(|| tree_size(&tree)).transpose()?,
                arrived: rfc3339(ctime),
                held: held.into_iter().collect(),
                hash,
            });
        }
        if !stays && !has_object {
            slots_gone.push(key);
        }
    }
    let mut plans_gone = Vec::new();
    for name in names_in(&root.join("store/plan")) {
        for hash in names_in(&root.join("store/plan").join(&name)) {
            if !plans.contains(&(name.clone(), hash.clone())) {
                plans_gone.push((name.clone(), hash));
            }
        }
    }
    Ok(Reap {
        generations_kept: kept.into_iter().filter(|g| ids.contains(g)).collect(),
        generations_gone,
        versions,
        plans_gone,
        slots_gone,
        building: names_in(&root.join("run/plan-build")),
    })
}

impl Reap {
    /// Versions nothing holds.
    pub fn going(&self) -> impl Iterator<Item = &Version> {
        self.versions.iter().filter(|v| v.held.is_empty())
    }

    /// Whether anything would go.
    pub fn is_empty(&self) -> bool {
        self.generations_gone.is_empty()
            && self.going().next().is_none()
            && self.plans_gone.is_empty()
            && self.slots_gone.is_empty()
    }

    /// Under the catalog's lock: move what goes aside, remove its plans and
    /// slots, and say so in each package's status. [`empty_trash`] then
    /// frees the space, outside the lock. Generations are the caller's: how
    /// one goes depends on what it is.
    pub fn execute(&self, cat: &Catalog, lock: &CatalogLock) -> Result<()> {
        let root = cat.root();
        let trash = root.join(TRASH);
        fs::create_dir_all(&trash)?;
        let mut touched = BTreeSet::new();
        for (i, v) in self.going().enumerate() {
            let name = v.name();
            fs::rename(slot_of(root, name).join(&v.hash), trash.join(i.to_string()))?;
            touched.insert(name.to_string());
        }
        for name in &self.slots_gone {
            fs::remove_dir_all(slot_of(root, name))?;
        }
        for (name, hash) in &self.plans_gone {
            let dir = root.join("store/plan").join(name);
            fs::remove_dir_all(dir.join(hash))?;
            if names_in(&dir).is_empty() {
                fs::remove_dir(&dir)?;
            }
        }
        for name in touched {
            let id = ObjectId::new(KIND_PKG, &name);
            if !name.starts_with("cell:") && root.join("objects/pkg").join(&name).is_dir() {
                cat.refresh_realizations(lock, &id)?;
            }
        }
        Ok(())
    }
}

/// Free what [`Reap::execute`] set aside, and what a run that stopped left.
pub fn empty_trash(root: &Path) -> Result<()> {
    let trash = root.join(TRASH);
    if trash.exists() {
        fs::remove_dir_all(&trash)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const A: &str = "sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "sha256-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "sha256-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const D: &str = "sha256-dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

    fn tree(root: &Path, name: &str, hash: &str) {
        let t = root.join("store/pkg").join(name).join(hash).join("bin");
        fs::create_dir_all(&t).unwrap();
        fs::write(t.join(name), "#!/bin/sh\n").unwrap();
    }

    fn object(root: &Path, name: &str, doc: Value) {
        let d = root.join("objects/pkg").join(name);
        fs::create_dir_all(&d).unwrap();
        crate::write_json(&d.join("desired.json"), &doc).unwrap();
        let meta = json!({ "id": format!("pkg:{name}"), "kind": "pkg", "name": name,
            "safety": "mutate", "status": "in-sync" });
        crate::write_json(&d.join("meta.json"), &meta).unwrap();
    }

    fn catalog(root: &Path) -> Catalog {
        crate::seed(root).unwrap();
        Catalog::open(root).unwrap()
    }

    /// Every tree old enough to go, unless a test says otherwise.
    fn policy() -> Policy {
        Policy {
            now: chrono::Utc::now().timestamp() + 2 * ARRIVING_SECS,
            arriving: ARRIVING_SECS,
            boot: None,
            sizes: false,
        }
    }

    fn held<'a>(r: &'a Reap, name: &str, hash: &str) -> &'a [String] {
        let id = format!("pkg:{name}");
        &r.versions.iter().find(|v| v.id == id && v.hash == hash).unwrap().held
    }

    #[test]
    fn a_version_stays_while_something_names_it() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let cat = catalog(root);
        let needs = json!([{ "id": "pkg:yy", "hash": C }]);
        object(root, "zz", json!({ "present": true, "hash": A, "needs": needs }));
        for (n, h) in [("zz", A), ("zz", B), ("zz", D), ("yy", C), ("orphan", A)] {
            tree(root, n, h);
        }
        crate::write_json(&root.join("output.json"), &json!({ "pins": { "zz": D } })).unwrap();

        let r = survey(&cat, &[], &policy()).unwrap();
        assert_eq!(held(&r, "zz", A), ["pin"]);
        assert_eq!(held(&r, "yy", C), ["needs:pkg:zz"]);
        assert_eq!(held(&r, "zz", D), ["release"]);
        assert!(held(&r, "zz", B).is_empty());
        assert!(held(&r, "orphan", A).is_empty());
        assert_eq!(r.slots_gone, ["orphan"], "no object, no version left");

        let lock = cat.lock().unwrap();
        r.execute(&cat, &lock).unwrap();
        drop(lock);
        empty_trash(root).unwrap();
        let store = root.join("store/pkg");
        assert!(store.join("zz").join(A).is_dir() && store.join("yy").join(C).is_dir());
        assert!(!store.join("zz").join(B).exists() && !store.join("orphan").exists());
        assert!(!root.join(TRASH).exists());
        let again = survey(&cat, &[], &policy()).unwrap();
        assert!(again.is_empty(), "nothing is left to go: {again:?}");
    }

    #[test]
    fn a_cell_keeps_its_pin_and_loses_the_rest() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let cat = catalog(root);
        let dir = root.join("objects/cell/hamlet");
        fs::create_dir_all(&dir).unwrap();
        crate::write_json(&dir.join("desired.json"), &json!({ "present": true, "hash": A }))
            .unwrap();
        let meta = crate::Meta::new("cell", "hamlet", "mutate");
        crate::write_json(&dir.join("meta.json"), &meta).unwrap();
        for h in [A, B] {
            fs::create_dir_all(root.join("store/cell/hamlet").join(h).join("bin")).unwrap();
        }
        // A package of the same name is another thing, held by nothing.
        tree(root, "hamlet", A);

        let r = survey(&cat, &[], &policy()).unwrap();
        let of = |id: &str, h: &str| {
            r.versions.iter().find(|v| v.id == id && v.hash == h).unwrap().held.clone()
        };
        assert_eq!(of("cell:hamlet", A), ["pin"]);
        assert!(of("cell:hamlet", B).is_empty());
        assert!(of("pkg:hamlet", A).is_empty());

        let lock = cat.lock().unwrap();
        r.execute(&cat, &lock).unwrap();
        drop(lock);
        empty_trash(root).unwrap();
        assert!(root.join("store/cell/hamlet").join(A).is_dir());
        assert!(!root.join("store/cell/hamlet").join(B).exists());
        assert!(!root.join("store/pkg/hamlet").exists());
    }

    #[test]
    fn a_tree_that_just_arrived_waits_for_its_pin() {
        let d = tempfile::tempdir().unwrap();
        let cat = catalog(d.path());
        tree(d.path(), "new", A);
        let p = Policy { now: chrono::Utc::now().timestamp(), ..policy() };
        let r = survey(&cat, &[], &p).unwrap();
        assert_eq!(held(&r, "new", A), ["arrived"]);
        assert!(r.slots_gone.is_empty());
    }

    #[test]
    fn the_newest_generations_and_those_since_boot_stay() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let cat = catalog(root);
        let mut gens = Vec::new();
        for n in 1..=8u64 {
            let at = root.join("gens").join(n.to_string());
            let hash = [A, B, C, D][(n % 4) as usize];
            object(&at, "pk", json!({ "present": true, "hash": hash }));
            gens.push((n, at));
        }
        object(root, "pk", json!({ "present": true, "hash": A }));
        for h in [A, B, C, D] {
            tree(root, "pk", h);
        }
        // Generation 1 was applied in this boot, 2 before it.
        let log = root.join("log/apply.jsonl");
        let lines = [
            r#"{"time":"2026-11-03T10:00:00Z","generation":1,"ids":[],"result":"ok"}"#,
            r#"{"time":"2026-11-01T10:00:00Z","generation":2,"ids":[],"result":"ok"}"#,
        ];
        fs::write(&log, lines.join("\n") + "\n").unwrap();
        let p = Policy { boot: Some("2026-11-02T00:00:00Z".into()), ..policy() };
        let r = survey(&cat, &gens, &p).unwrap();
        assert_eq!(r.generations_kept, [1, 4, 5, 6, 7, 8]);
        assert_eq!(r.generations_gone, [2, 3]);
        // 8 pins A, 7 D, 6 C, 5 B: each version is some kept generation's.
        assert_eq!(held(&r, "pk", D), ["generation:7"]);
        assert_eq!(held(&r, "pk", B), ["generation:1", "generation:5"]);
    }

    #[test]
    fn a_plan_keeps_what_it_builds_with_until_it_is_replaced() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path();
        let cat = catalog(root);
        let need = |h: &str| crate::PkgNeed { id: "pkg:hello-src".into(), hash: h.into() };
        let text = crate::fmt_plan(&crate::plan::PlanFile {
            name: "hello".into(),
            produces: "pkg:hello".into(),
            build_needs: vec![need(A)],
            run_needs: vec![],
            script: "#!/bin/sh\ntrue\n".into(),
        });
        let pinned = crate::file_plan(root, text.as_bytes()).unwrap();
        tree(root, "hello-src", A);
        tree(root, "hello-src", B);
        let old = root.join("store/plan/hello/sha256-old");
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join(crate::plan::PLAN_NAME), "old").unwrap();
        let r = survey(&cat, &[], &policy()).unwrap();
        assert_eq!(held(&r, "hello-src", A), ["plan:hello"]);
        assert!(held(&r, "hello-src", B).is_empty());
        assert_eq!(r.plans_gone, [("hello".to_string(), "sha256-old".to_string())]);
        assert!(!r.plans_gone.iter().any(|(_, h)| *h == pinned));
    }
}
