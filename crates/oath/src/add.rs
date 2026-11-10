//! Packages by name (R81). `oath ls --origin` lists what the origins hold.
//! `oath new pkg:<name>` with no fields, and `oath add`, take a package at
//! its newest signed head with each package it needs that this machine
//! lacks, by the rule `oath update` follows (R72): the key must be one
//! `host:local.publishers` lets sign that name. Nothing the machine has
//! moves. A need it holds at another pin is re-pointed if the package loses
//! no library; if it would, nothing is made and the update comes first.
//! What it takes is recorded in `/oath/heads.json`, as an update records
//! the heads it took. Run by a user who cannot change the catalog, `oath
//! add` says what `sudo oath add` would make and the size of each pack.
//!
//! The list is `index.json`, which is not signed. Nothing is installed from
//! it: only a head says what a name is.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use oath_core::head::{self, Offer};
use oath_core::update::{short, Move};
use oath_core::{Actor, ApplyHooks, Catalog, Error, ObjectId, Result, KIND_PKG};
use serde_json::{json, Map, Value};

use crate::publish::read_url;
use crate::update::{fetch_heads, record_heads, settle, trust, Scratch};

/// One package to make, its pack in the store.
struct Made {
    id: ObjectId,
    doc: Map<String, Value>,
    version: String,
    signer: String,
    repointed: Vec<String>,
}

impl Made {
    fn hash(&self) -> &str {
        self.doc["hash"].as_str().unwrap_or("")
    }

    fn json(&self) -> Value {
        json!({
            "id": self.id.to_string(),
            "hash": self.hash(),
            "version": self.version,
            "signer": self.signer,
            "repointed": self.repointed,
        })
    }

    fn line(&self) -> String {
        let version =
            if self.version.is_empty() { String::new() } else { format!(" ({})", self.version) };
        format!("{} {}{version}, signed by {}", self.id, short(self.hash()), self.signer)
    }
}

fn scratch() -> Result<Scratch> {
    let d = std::env::temp_dir().join(format!("oath-add-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d)?;
    Ok(Scratch(d))
}

/// Whether this user may change the catalog at `root`.
fn can_write(root: &Path) -> bool {
    let Ok(p) = CString::new(root.join("objects").as_os_str().as_bytes()) else { return false };
    unsafe { libc::access(p.as_ptr(), libc::W_OK) == 0 }
}

/// The heads of `names` and of what they need, from this machine's origins,
/// signed by keys it trusts. What was refused goes to stderr.
fn offers_for(cat: &Catalog, names: &[String], work: &Path) -> Result<BTreeMap<String, Offer>> {
    let (origins, trusted) = trust(cat)?;
    let asked: BTreeMap<String, Value> = names.iter().map(|n| (n.clone(), json!({}))).collect();
    let (offers, refused) = fetch_heads(&origins, &asked, &trusted, work)?;
    for r in &refused {
        eprintln!("  {r}");
    }
    Ok(offers)
}

/// What taking `names` makes, every pack fetched (with `fetch`) and every
/// need settled before the catalog changes: a failure here leaves the
/// machine as it was. Without `fetch`, a need is re-pointed unchecked when
/// a pack is not in the store yet, as an update's preview does.
fn plan(
    root: &Path,
    cat: &Catalog,
    names: &[String],
    offers: &BTreeMap<String, Offer>,
    fetch: bool,
) -> Result<Vec<Made>> {
    let mut machine = BTreeMap::new();
    for id in cat.ls(Some(KIND_PKG))? {
        machine.insert(id.name.clone(), cat.get(&id)?.desired);
    }
    let (with, moves) = head::additions(&machine, offers, names)?;
    for m in moves.iter().filter(|_| fetch) {
        let Move::Head { name, to, .. } = m else { continue };
        if !oath_core::store_present(root, name, to) {
            oath_core::prefetch_pkg_any(root, name, "", "", to)?;
        }
    }
    let settled = settle(root, &with, moves);
    if let Some(Move::Hold { why, .. }) = settled.iter().find(|m| matches!(m, Move::Hold { .. })) {
        return Err(Error::hint(
            why.clone(),
            "nothing changed; `sudo oath update --confirm` moves this machine to the heads",
        ));
    }
    check_links(root, cat, &settled)?;
    let mut out = Vec::new();
    for m in settled {
        let Move::Head { name, set, signer, repointed, .. } = m else { continue };
        let mut doc = set.as_object().cloned().unwrap_or_default();
        doc.retain(|_, v| !v.is_null());
        doc.insert("present".into(), json!(true));
        let version = offers.get(&name).map(|o| o.head.version.clone()).unwrap_or_default();
        out.push(Made { id: ObjectId::new(KIND_PKG, &name), doc, version, signer, repointed });
    }
    Ok(out)
}

/// Refuse when a package made would put a name in `/bin` or `/lib` that a
/// package here, or another one made, puts there: the apply would link
/// some and stop. A pack not in the store yet (a preview's) is not checked.
fn check_links(root: &Path, cat: &Catalog, moves: &[Move]) -> Result<()> {
    let mut pkgs = Vec::new();
    for id in cat.ls(Some(KIND_PKG))? {
        let actual = cat.get(&id)?.actual;
        if actual["present"] == json!(true) {
            pkgs.push((id.name.clone(), actual["hash"].as_str().unwrap_or("").to_string()));
        }
    }
    for m in moves {
        if let Move::Head { name, to, .. } = m {
            pkgs.push((name.clone(), to.clone()));
        }
    }
    let clashes = oath_core::link_clashes(root, &pkgs);
    if clashes.is_empty() {
        return Ok(());
    }
    let list: Vec<String> =
        clashes.iter().map(|(n, a, b)| format!("{n} (pkg:{a} and pkg:{b})")).collect();
    Err(Error::hint(
        format!("two packages would ship one name: {}", list.join(", ")),
        "nothing changed; one machine has one of them (oath get pkg:<name>)",
    ))
}

/// Make each object, or none of them.
fn create(cat: &Catalog, made: &[Made]) -> Result<Vec<ObjectId>> {
    let mut done = Vec::new();
    for m in made {
        if let Err(e) = cat.new_object(&m.id, m.doc.clone()) {
            forget(cat, &done);
            return Err(e);
        }
        done.push(m.id.clone());
    }
    Ok(done)
}

/// Take back what [`create`] made and no apply reached.
fn forget(cat: &Catalog, ids: &[ObjectId]) {
    for id in ids {
        let _ = cat.forget_new(id);
    }
}

fn need_root(root: &Path, verb: &str, what: &str) -> Result<()> {
    if can_write(root) {
        return Ok(());
    }
    Err(Error::hint(format!("{verb} changes this machine"), format!("sudo oath {verb} {what}")))
}

/// `oath new pkg:<name>` with no fields. None when the old way holds: the
/// name is taken, for `new_object` to say so, or no origin here has a head
/// of it and the store has a tree of it, which is the pin (`oath schema pkg`).
pub fn new(root: &Path, cat: &Catalog, id: &ObjectId, as_json: bool) -> Result<Option<i32>> {
    if cat.get(id).is_ok() {
        return Ok(None);
    }
    let names = [id.name.clone()];
    let local = oath_core::resolve_tree(root, &id.name, "").is_ok();
    let work = scratch()?;
    let offers = match offers_for(cat, &names, &work.0) {
        Ok(o) => o,
        Err(_) if local => return Ok(None),
        Err(e) => return Err(e),
    };
    if local && !offers.contains_key(&id.name) {
        return Ok(None);
    }
    need_root(root, "new", &id.to_string())?;
    let made = plan(root, cat, &names, &offers, true)?;
    let ids = create(cat, &made)?;
    let list: Vec<String> = ids.iter().map(ObjectId::to_string).collect();
    if as_json {
        let v: Vec<Value> = made.iter().map(Made::json).collect();
        println!("{}", json!({ "ok": true, "made": v }));
    } else {
        for m in &made {
            println!("{} created", m.line());
            for r in &m.repointed {
                println!("  runs with this machine's {r}");
            }
        }
        println!("`oath apply {}` starts {}", list.join(" "), starts(list.len()));
    }
    Ok(Some(0))
}

fn starts(n: usize) -> &'static str {
    if n == 1 {
        "it"
    } else {
        "them"
    }
}

/// `oath add`: [`new`] and the apply in one step. A package already here is
/// said and left: `oath update` moves it.
pub fn add(
    root: &Path,
    cat: &Catalog,
    actor: &Actor,
    hooks: &dyn ApplyHooks,
    names: Vec<String>,
    confirm: bool,
    as_json: bool,
) -> Result<i32> {
    let mut wanted = Vec::new();
    let mut here = Vec::new();
    let mut seen = BTreeSet::new();
    for n in names {
        let id: ObjectId = if n.contains(':') { n.parse()? } else { format!("pkg:{n}").parse()? };
        if id.kind != KIND_PKG {
            return Err(Error::hint(format!("{id} is not a package"), "oath add <name>"));
        }
        if !seen.insert(id.name.clone()) {
            continue;
        }
        match cat.get(&id) {
            Ok(o) if o.desired["present"] == json!(false) => {
                return Err(Error::hint(
                    format!("{id} is here with present=false"),
                    format!("oath set {id} present=true, then oath apply {id}"),
                ));
            }
            Ok(o) => here.push((id, o.desired["hash"].as_str().unwrap_or("").to_string())),
            Err(_) => wanted.push(id.name),
        }
    }
    let here_json: Vec<Value> =
        here.iter().map(|(id, h)| json!({ "id": id.to_string(), "hash": h })).collect();
    if !as_json {
        for (id, h) in &here {
            println!("{id} is here at {}; `oath update` moves it to its head", short(h));
        }
    }
    if wanted.is_empty() {
        if as_json {
            println!("{}", json!({ "added": [], "here": here_json, "generation": Value::Null }));
        }
        return Ok(0);
    }
    let work = scratch()?;
    let offers = offers_for(cat, &wanted, &work.0)?;
    if !can_write(root) {
        return preview(root, cat, &wanted, &offers, here_json, as_json);
    }
    let made = plan(root, cat, &wanted, &offers, true)?;
    let ids = create(cat, &made)?;
    let report = cat.apply_with(
        Some(ids.clone()),
        confirm,
        oath_core::probation::DEFAULT_SECS,
        actor,
        hooks,
    );
    let report = match report {
        Ok(r) => r,
        Err(e) => {
            // Each or none: an apply that stopped part way linked some.
            match cat.undo_if_newest(&ids, actor, hooks) {
                Ok(Some(r)) => eprintln!("undone: back to generation {}", r.generation),
                Ok(None) => {}
                Err(u) => eprintln!("could not undo the failed apply: {u}; `sudo oath undo`"),
            }
            forget(cat, &ids);
            return Err(e);
        }
    };
    // The heads it took, as an update records them: none older comes after.
    record_heads(root, cat, &offers)?;
    if as_json {
        let v: Vec<Value> = made.iter().map(Made::json).collect();
        println!("{}", json!({ "added": v, "here": here_json, "generation": report.generation }));
    } else {
        for m in &made {
            println!("{}", m.line());
            for r in &m.repointed {
                println!("  runs with this machine's {r}");
            }
        }
        println!("added in generation {}", report.generation);
    }
    Ok(0)
}

/// `oath add` by a user who cannot change the catalog: what `sudo oath add`
/// would make, each with its pack's size on the origin it came from, and
/// nothing fetched or changed.
fn preview(
    root: &Path,
    cat: &Catalog,
    wanted: &[String],
    offers: &BTreeMap<String, Offer>,
    here: Vec<Value>,
    as_json: bool,
) -> Result<i32> {
    let made = plan(root, cat, wanted, offers, false)?;
    let run = format!("sudo oath add {}", wanted.join(" "));
    let mut rows = Vec::new();
    let mut fetch = 0u64;
    let mut unknown = false;
    for m in &made {
        let in_store = oath_core::store_present(root, &m.id.name, m.hash());
        let size = offers.get(&m.id.name).and_then(|o| pack_size(&o.origin, &m.id.name, m.hash()));
        match size {
            Some(n) if !in_store => fetch += n,
            None if !in_store => unknown = true,
            _ => {}
        }
        let mut v = m.json();
        v["asked"] = json!(wanted.contains(&m.id.name));
        v["size"] = json!(size);
        v["in_store"] = json!(in_store);
        rows.push(v);
        if !as_json {
            let what = match (in_store, size) {
                (true, _) => "in the store already".to_string(),
                (false, Some(n)) => mb(n),
                (false, None) => "its size unknown".to_string(),
            };
            println!("would add {}: {what}", m.line());
            for r in &m.repointed {
                println!("  runs with this machine's {r}");
            }
        }
    }
    if as_json {
        let v = json!({
            "would_add": rows,
            "size": fetch,
            "size_known": !unknown,
            "added": [],
            "here": here,
            "generation": Value::Null,
            "run": run,
        });
        println!("{v}");
    } else {
        let more = if unknown { " and more" } else { "" };
        println!(
            "{}{more} to fetch; `{run}` adds {} and checks every library and link once the packs \
             are here",
            mb(fetch),
            starts(made.len()),
        );
    }
    Ok(0)
}

/// The size of `pkg/<name>/<hash>.tar` on `origin`, from its headers.
fn pack_size(origin: &str, name: &str, hash: &str) -> Option<u64> {
    let url = oath_core::fetch_url(origin, name, hash);
    let out = std::process::Command::new(oath_core::find_bin("curl"))
        .args(["-sfIL", &url])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    // After a redirect, the last response's length.
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.split_once(':'))
        .filter(|(k, _)| k.trim().eq_ignore_ascii_case("content-length"))
        .filter_map(|(_, v)| v.trim().parse().ok())
        .last()
}

fn mb(bytes: u64) -> String {
    if bytes < 1_000_000 {
        return format!("{} kB", bytes.div_ceil(1000));
    }
    format!("{:.1} MB", bytes as f64 / 1e6)
}

/// One row of an origin's `index.json`.
fn row(e: &Value, recipe: bool, name: &str) -> Value {
    json!({
        "name": name,
        "version": e["version"].as_str().unwrap_or(""),
        "category": e["category"].as_str().unwrap_or(""),
        "description": e["description"].as_str().unwrap_or(""),
        "license": e["license"].as_str().unwrap_or(""),
        "hash": e["hash"].as_str().or(e["builds"].as_str()).unwrap_or(""),
        "recipe": recipe,
    })
}

fn matches(r: &Value, word: &str) -> bool {
    let word = word.to_lowercase();
    ["name", "category", "description"]
        .iter()
        .any(|k| r[k].as_str().is_some_and(|s| s.to_lowercase().contains(&word)))
}

/// What the origins hold, in `host:local.origins` order for a name more than
/// one lists: `oath ls --origin [word]`.
pub fn list(root: &Path, cat: &Catalog, word: Option<&str>, as_json: bool) -> Result<i32> {
    let origins = oath_core::host_origins(root);
    if origins.is_empty() {
        return Err(Error::hint("host:local names no origins", "oath schema host"));
    }
    let mut rows = BTreeMap::<String, Value>::new();
    let mut answered = false;
    for o in &origins {
        let o = o.trim_end_matches('/');
        let got = read_url(&format!("{o}/index.json"))
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
        let Some(index) = got else {
            eprintln!("  {o} served no index");
            continue;
        };
        answered = true;
        for (list, recipe) in [("packages", false), ("recipes", true)] {
            for e in index[list].as_array().into_iter().flatten() {
                let Some(name) = e["name"].as_str() else { continue };
                rows.entry(name.to_string()).or_insert_with(|| row(e, recipe, name));
            }
        }
    }
    if !answered {
        return Err(Error::hint("no origin served an index", "oath get host:local (origins)"));
    }
    let mut here = BTreeMap::new();
    for id in cat.ls(Some(KIND_PKG))? {
        let d = cat.get(&id)?.desired;
        if d["present"] != json!(false) {
            here.insert(id.name.clone(), d["hash"].as_str().unwrap_or("").to_string());
        }
    }
    let mut shown: Vec<Value> =
        rows.into_values().filter(|r| word.is_none_or(|w| matches(r, w))).collect();
    for r in &mut shown {
        let hash = here.get(r["name"].as_str().unwrap_or("")).cloned();
        r["installed"] = json!(hash);
    }
    if as_json {
        println!("{}", serde_json::to_string_pretty(&shown)?);
        return Ok(0);
    }
    if shown.is_empty() {
        eprintln!("nothing on the origins matches");
        return Ok(0);
    }
    let width = |k: &str| shown.iter().map(|r| r[k].as_str().unwrap_or("").len()).max().unwrap_or(0);
    let (name, version, category) = (width("name"), width("version"), width("category"));
    for r in &shown {
        let s = |k: &str| r[k].as_str().unwrap_or("");
        let mark = if r["installed"].is_null() { ' ' } else { '*' };
        let what = if r["recipe"] == json!(true) {
            format!("{} (a recipe: `oath build` makes it here)", s("description"))
        } else {
            s("description").to_string()
        };
        let line = format!(
            "{mark} {:<name$}  {:<version$}  {:<category$}  {what}",
            s("name"),
            s("version"),
            s("category"),
        );
        println!("{}", line.trim_end());
    }
    eprintln!("* is on this machine; `sudo oath add <name>` installs one with what it needs");
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Value {
        json!({ "name": "cell", "version": "0.1.0", "category": "machines",
                "description": "Run a container", "hash": "sha256-a" })
    }

    #[test]
    fn a_word_finds_a_name_a_category_or_a_description() {
        let r = row(&entry(), false, "cell");
        assert!(matches(&r, "CEL") && matches(&r, "machine") && matches(&r, "contain"));
        assert!(!matches(&r, "postgres"));
    }

    #[test]
    fn a_recipe_says_what_it_builds() {
        let e = json!({ "name": "claude-code", "builds": "sha256-b", "version": "2" });
        let r = row(&e, true, "claude-code");
        assert_eq!(r["hash"], "sha256-b");
        assert_eq!(r["recipe"], true);
    }

    #[test]
    fn an_origin_index_with_no_description_still_lists() {
        let e = json!({ "name": "old", "hash": "sha256-c" });
        let r = row(&e, false, "old");
        assert_eq!(r["description"], "");
        assert!(matches(&r, "old"));
    }

    fn seeded() -> (tempfile::TempDir, Catalog) {
        let d = tempfile::tempdir().unwrap();
        oath_core::seed(d.path()).unwrap();
        let cat = Catalog::open(d.path()).unwrap();
        (d, cat)
    }

    #[test]
    fn a_name_taken_is_left_to_new_object_to_refuse() {
        let (d, cat) = seeded();
        let id: ObjectId = "pkg:oath".parse().unwrap();
        assert!(cat.get(&id).is_ok());
        assert_eq!(new(d.path(), &cat, &id, false).unwrap(), None);
    }

    #[test]
    fn forgetting_takes_back_only_what_was_never_applied() {
        let (_d, cat) = seeded();
        let new: ObjectId = "pkg:fresh".parse().unwrap();
        let old: ObjectId = "pkg:oath".parse().unwrap();
        let before = cat.get(&old).unwrap().desired;
        let doc = json!({ "present": false }).as_object().unwrap().clone();
        cat.new_object(&new, doc).unwrap();
        forget(&cat, &[new.clone(), old.clone()]);
        assert!(cat.get(&new).is_err(), "never applied, so gone");
        assert_eq!(cat.get(&old).unwrap().desired, before);
        assert!(cat.diff(Some(&old)).unwrap().is_empty(), "not marked for removal");
    }

    #[test]
    fn another_kind_is_not_added() {
        let (d, cat) = seeded();
        let a = Actor::current();
        let hooks = oath_core::NullHooks;
        let e = add(d.path(), &cat, &a, &hooks, vec!["svc:sshd".into()], false, false).unwrap_err();
        assert!(e.to_string().contains("is not a package"), "{e}");
    }

    #[test]
    fn a_package_already_here_is_said_and_left() {
        let (d, cat) = seeded();
        let a = Actor::current();
        let hooks = oath_core::NullHooks;
        let before = cat.get(&"pkg:oath".parse().unwrap()).unwrap().desired;
        assert_eq!(add(d.path(), &cat, &a, &hooks, vec!["oath".into()], false, false).unwrap(), 0);
        assert_eq!(cat.get(&"pkg:oath".parse().unwrap()).unwrap().desired, before);
    }
}
