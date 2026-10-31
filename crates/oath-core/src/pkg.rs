//! Link farm: `/oath/store/pkg/<name>/<hash>/bin/*` ↔ `/bin/<basename>`.
//!
//! Slot wrappers (not part of the hash): `live` → `<hash>`, plus a
//! symlink per top-level pack entry (`bin` → `live/bin`, …) so RPATH
//! and env that still name `/oath/store/pkg/<name>/lib` keep working.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};
use crate::kinds::{
    Pkg, PkgActual, PkgAdvisory, PkgComponent, PkgNeed, PkgRealization, PkgVerified,
};
use crate::packhash::{
    fetch_adv_url, fetch_url, hash_tree, is_realization_id, realization_dir, slot_dir, LIVE_NAME,
};

/// Pack-root file: upstream identity for feed matching (T44). Part of
/// the tree hash. Missing means unknown, not safe.
pub const COMPONENTS_NAME: &str = "components.json";
use crate::write_json;

/// Create or remove this package’s `/bin` symlinks, and its `/lib` ones
/// (R17: `/lib` is the link farm the loader searches; `lib_dir` is
/// `bin_dir`'s sibling). Never clobber a name that does not already point
/// at this package’s store.
///
/// `link_root` is the prefix written into symlink targets (`/oath` on
/// the appliance, even when packing from a stage tree).
pub fn converge_with_link_root(
    store_root: &Path,
    bin_dir: &Path,
    link_root: &Path,
    name: &str,
    present: bool,
    hash: &str,
) -> Result<PkgActual> {
    let resolved = if present { Some(resolve_tree(store_root, name, hash)?) } else { None };
    let tree = match &resolved {
        Some(p) => p.clone(),
        None => match resolve_tree(store_root, name, hash) {
            Ok(p) => p,
            Err(_) => slot_dir(store_root, name),
        },
    };
    let store = tree.join("bin");
    if present && !store.is_dir() {
        return Err(Error::hint(
            format!("no store for pkg:{name}"),
            format!("oath get pkg:{name}"),
        ));
    }
    let live_hash = tree_hash_id(store_root, name, &tree)?;
    let mut verified = None;
    if present
        && is_realization_id(&live_hash)
        && realization_dir(store_root, name, &live_hash).is_dir()
    {
        // Never link a tree that no longer is what its name says.
        verified = Some(verify_tree(store_root, name, &live_hash)?);
        if let Some(v) = verified.as_ref().filter(|v| !v.ok) {
            return Err(Error::hint(
                format!(
                    "pkg:{name} store tree {live_hash} now hashes to {}: the store is damaged",
                    v.got
                ),
                format!("oath verify; fetch pkg:{name} again (url) or reinstall"),
            ));
        }
        write_slot_links(store_root, name, &live_hash)?;
    }
    fs::create_dir_all(bin_dir)?;
    let lib_dir = bin_dir.with_file_name("lib");
    let names = bin_names(&store)?;
    let libs = lib_names(&tree.join("lib"))?;
    if present {
        for n in &libs {
            link_one(&lib_dir, "lib", link_root, store_root, name, n, &tree)?;
        }
        let mut links = Vec::new();
        for n in &names {
            let target = store_target(link_root, name, "bin", n, &tree, store_root);
            let dest = bin_dir.join(n);
            if dest.symlink_metadata().is_ok() {
                if is_our_link(&dest, &target, &store.join(n), &slot_dir(store_root, name)) {
                    let cur = fs::read_link(&dest).ok();
                    if cur.as_deref() == Some(target.as_path()) {
                        links.push(n.clone());
                        continue;
                    }
                    fs::remove_file(&dest)?;
                } else if handed_over(store_root, &dest, "bin", n) {
                    fs::remove_file(&dest)?;
                } else {
                    return Err(Error::hint(
                        format!("/bin/{n} exists and is not pkg:{name}"),
                        "oath schema pkg",
                    ));
                }
            }
            symlink(&target, &dest).map_err(|e| {
                Error::Msg(format!("symlink {} -> {}: {e}", dest.display(), target.display()))
            })?;
            links.push(n.clone());
        }
        unlink_dropped(bin_dir, name, &names)?;
        unlink_dropped(&lib_dir, name, &libs)?;
        let mut actual = pkg_actual(true, links, live_hash, store_root, name);
        actual.verified = verified;
        Ok(actual)
    } else {
        for n in &names {
            unlink_ours(bin_dir, "bin", link_root, store_root, name, n, &tree)?;
        }
        for n in &libs {
            unlink_ours(&lib_dir, "lib", link_root, store_root, name, n, &tree)?;
        }
        unlink_dropped(bin_dir, name, &[])?;
        unlink_dropped(&lib_dir, name, &[])?;
        Ok(pkg_actual(false, Vec::new(), live_hash, store_root, name))
    }
}

/// What of `name` is linked now, changing nothing: the live realization
/// and the `/bin` names that point into it. With no tree in the store it
/// is absent; a tree with nothing in `bin/` cannot show it is present, so
/// it is what `applied_present` says.
pub fn observe(
    catalog_root: &Path,
    bin_dir: &Path,
    name: &str,
    applied_present: bool,
) -> PkgActual {
    let hash = live_hash(catalog_root, name).ok().flatten().unwrap_or_default();
    let tree = if hash.is_empty() {
        slot_dir(catalog_root, name)
    } else {
        realization_dir(catalog_root, name, &hash)
    };
    let store = tree.join("bin");
    let slot = slot_dir(catalog_root, name);
    let names = bin_names(&store).unwrap_or_default();
    let links: Vec<String> = names
        .iter()
        .filter(|n| {
            let target = store_target(catalog_root, name, "bin", n, &tree, catalog_root);
            is_our_link(&bin_dir.join(n), &target, &store.join(n), &slot)
        })
        .cloned()
        .collect();
    let present = if !tree.is_dir() {
        false
    } else if names.is_empty() {
        applied_present
    } else {
        links.len() == names.len()
    };
    pkg_actual(present, links, hash, catalog_root, name)
}

pub fn normalize_need(w: &str) -> String {
    if w.contains(':') {
        w.to_string()
    } else {
        format!("pkg:{w}")
    }
}

fn need_id(n: &PkgNeed) -> String {
    normalize_need(&n.id)
}

/// Cycles refuse. present=true needs every need present at `need.hash`.
/// present=false is refused while another present pack still lists this.
pub fn check_needs(pkgs: &[(String, Pkg)]) -> Result<()> {
    use std::collections::{HashMap, HashSet, VecDeque};

    let by_id: HashMap<String, &Pkg> = pkgs.iter().map(|(id, p)| (id.clone(), p)).collect();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut indeg: HashMap<String, usize> = HashMap::new();
    for id in by_id.keys() {
        adj.insert(id.clone(), Vec::new());
        indeg.insert(id.clone(), 0);
    }
    for (id, spec) in pkgs {
        let mut seen = HashSet::new();
        for n in &spec.needs {
            let w = need_id(n);
            if spec.present && !is_realization_id(&n.hash) {
                return Err(Error::hint(
                    format!("{id} needs {w} hash (sha256-… required)"),
                    "oath schema pkg",
                ));
            }
            if w == *id {
                return Err(Error::hint(format!("{id} needs itself"), "oath schema pkg"));
            }
            if !by_id.contains_key(&w) {
                if spec.present {
                    return Err(Error::hint(
                        format!("{id} needs {w}, which is not in the catalog"),
                        "oath schema pkg",
                    ));
                }
                continue;
            }
            if !seen.insert(w.clone()) {
                continue;
            }
            adj.get_mut(&w).expect("adj").push(id.clone());
            *indeg.get_mut(id).expect("indeg") += 1;
        }
    }
    let mut q: VecDeque<String> =
        indeg.iter().filter(|(_, d)| **d == 0).map(|(id, _)| id.clone()).collect();
    q.make_contiguous().sort();
    let mut out = Vec::new();
    while let Some(id) = q.pop_front() {
        out.push(id.clone());
        let mut nxt = adj.remove(&id).unwrap_or_default();
        nxt.sort();
        for n in nxt {
            if let Some(d) = indeg.get_mut(&n) {
                *d -= 1;
                if *d == 0 {
                    q.push_back(n);
                }
            }
        }
    }
    if out.len() != by_id.len() {
        return Err(Error::hint("pkg needs cycle", "oath schema pkg"));
    }

    for (id, spec) in pkgs {
        if spec.present {
            continue;
        }
        let mut users = Vec::new();
        for (other, ospec) in pkgs {
            if !ospec.present {
                continue;
            }
            if ospec.needs.iter().any(|n| need_id(n) == *id) {
                users.push(other.clone());
            }
        }
        if !users.is_empty() {
            users.sort();
            return Err(Error::hint(
                format!("{id} is needed by {}", users.join(", ")),
                format!("oath set {} present=false", users.join(" ")),
            ));
        }
    }

    for (id, spec) in pkgs {
        if !spec.present {
            continue;
        }
        for n in &spec.needs {
            let w = need_id(n);
            if !is_realization_id(&n.hash) {
                return Err(Error::hint(
                    format!("{id} needs {w} hash (sha256-… required)"),
                    "oath schema pkg",
                ));
            }
            let Some(need) = by_id.get(&w) else {
                return Err(Error::hint(
                    format!("{id} needs {w}, which is not in the catalog"),
                    "oath schema pkg",
                ));
            };
            if !need.present {
                return Err(Error::hint(
                    format!("{id} needs {w} present"),
                    format!("oath set {w} present=true"),
                ));
            }
            if !need.hash.is_empty() && need.hash != n.hash {
                return Err(Error::hint(
                    format!("{id} needs {w} at {}, live is {}", n.hash, need.hash),
                    format!("oath set {id} needs, or pin {w} hash={}", n.hash),
                ));
            }
        }
    }
    Ok(())
}

/// Names in `/lib` that are Oath's own (its helpers, the kernel's modules
/// and firmware), never a package's link; and pkg-config files, which
/// stay with their pack.
const LIB_NOT_LINKED: &[&str] = &["oath", "modules", "firmware", "pkgconfig"];

/// The entries of a pack's `lib/` that go into `/lib`.
fn lib_names(lib: &Path) -> Result<Vec<String>> {
    if !lib.is_dir() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for e in fs::read_dir(lib)? {
        let n = e?.file_name().to_string_lossy().into_owned();
        if !LIB_NOT_LINKED.contains(&n.as_str()) {
            names.push(n);
        }
    }
    names.sort();
    Ok(names)
}

/// Link `<dir_path>/<n>` to the pack's `<dir>/<n>`, or leave it if it is
/// already ours; another package's or a real file is an error, unless
/// that package hands the name over (`handed_over`).
fn link_one(
    dir_path: &Path,
    dir: &str,
    link_root: &Path,
    store_root: &Path,
    name: &str,
    n: &str,
    tree: &Path,
) -> Result<()> {
    fs::create_dir_all(dir_path)?;
    let target = store_target(link_root, name, dir, n, tree, store_root);
    let dest = dir_path.join(n);
    if dest.symlink_metadata().is_ok() && handed_over(store_root, &dest, dir, n) {
        fs::remove_file(&dest)?;
    }
    if dest.symlink_metadata().is_ok() {
        if !is_our_link(&dest, &target, &tree.join(dir).join(n), &slot_dir(store_root, name)) {
            return Err(Error::hint(
                format!("/{dir}/{n} exists and is not pkg:{name}"),
                "oath schema pkg",
            ));
        }
        if fs::read_link(&dest).ok().as_deref() == Some(target.as_path()) {
            return Ok(());
        }
        fs::remove_file(&dest)?;
    }
    symlink(&target, &dest)
        .map_err(|e| Error::Msg(format!("symlink {} -> {}: {e}", dest.display(), target.display())))
}

fn unlink_ours(
    dir_path: &Path,
    dir: &str,
    link_root: &Path,
    store_root: &Path,
    name: &str,
    n: &str,
    tree: &Path,
) -> Result<()> {
    let dest = dir_path.join(n);
    let store = tree.join(dir);
    let target = store_target(link_root, name, dir, n, tree, store_root);
    if dest.symlink_metadata().is_ok()
        && is_our_link(&dest, &target, &store.join(n), &slot_dir(store_root, name))
    {
        fs::remove_file(&dest)?;
    }
    Ok(())
}

/// Remove the links in `dir_path` into pkg:`name`'s slot whose names are
/// not in `keep`: what an earlier realization had and this one dropped.
/// Whether `dest` links into another package's store, and that package's
/// desired tree (in the store already) has no `<dir>/<n>`, or it is to be
/// absent: it drops the name in this apply, and another package may take
/// it whichever converges first (R41: `libEGL.so.1` from Mesa to
/// libglvnd, in one update).
fn handed_over(store_root: &Path, dest: &Path, dir: &str, n: &str) -> bool {
    let Ok(cur) = fs::read_link(dest) else { return false };
    let cur = cur.to_string_lossy();
    let Some(other) = cur.split("/store/pkg/").nth(1).and_then(|r| r.split('/').next()) else {
        return false;
    };
    let desired = store_root.join("objects/pkg").join(other).join("desired.json");
    let Ok(doc) = crate::read_json::<serde_json::Value>(&desired) else { return false };
    if doc.get("present") == Some(&serde_json::json!(false)) {
        return true;
    }
    let Some(hash) = doc.get("hash").and_then(|h| h.as_str()) else { return false };
    let tree = realization_dir(store_root, other, hash);
    tree.is_dir() && tree.join(dir).join(n).symlink_metadata().is_err()
}

fn unlink_dropped(dir_path: &Path, name: &str, keep: &[String]) -> Result<()> {
    let Ok(entries) = fs::read_dir(dir_path) else { return Ok(()) };
    let ours = format!("/store/pkg/{name}/");
    for e in entries {
        let e = e?;
        let n = e.file_name().to_string_lossy().into_owned();
        if keep.contains(&n) || LIB_NOT_LINKED.contains(&n.as_str()) || !e.file_type()?.is_symlink()
        {
            continue;
        }
        if fs::read_link(e.path())?.to_string_lossy().contains(&ours) {
            fs::remove_file(e.path())?;
        }
    }
    Ok(())
}

pub fn converge(
    catalog_root: &Path,
    bin_dir: &Path,
    name: &str,
    present: bool,
    hash: &str,
) -> Result<PkgActual> {
    converge_with_link_root(catalog_root, bin_dir, catalog_root, name, present, hash)
}

fn store_target(
    link_root: &Path,
    name: &str,
    dir: &str,
    file: &str,
    tree: &Path,
    store_root: &Path,
) -> PathBuf {
    let slot = slot_dir(store_root, name);
    if let Ok(rel) = tree.strip_prefix(&slot) {
        let rel = rel.to_string_lossy();
        if is_realization_id(&rel) {
            return link_root
                .join("store")
                .join("pkg")
                .join(name)
                .join(rel.as_ref())
                .join(dir)
                .join(file);
        }
    }
    link_root.join("store").join("pkg").join(name).join(dir).join(file)
}

fn bin_names(store: &Path) -> Result<Vec<String>> {
    if !store.is_dir() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for e in fs::read_dir(store)? {
        let e = e?;
        let path = e.path();
        if path.is_dir() {
            continue;
        }
        names.push(e.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

fn is_our_link(dest: &Path, target: &Path, store_file: &Path, slot: &Path) -> bool {
    let Ok(meta) = dest.symlink_metadata() else {
        return false;
    };
    if !meta.file_type().is_symlink() {
        return false;
    }
    let Ok(cur) = fs::read_link(dest) else {
        return false;
    };
    if cur == target || cur == store_file {
        return true;
    }
    let resolved = if cur.is_absolute() {
        cur.clone()
    } else {
        dest.parent().unwrap_or(Path::new(".")).join(&cur)
    };
    if let (Ok(a), Ok(b)) = (fs::canonicalize(&resolved), fs::canonicalize(store_file)) {
        if a == b {
            return true;
        }
    }
    if let (Ok(a), Ok(slot)) = (fs::canonicalize(&resolved), fs::canonicalize(slot)) {
        if a.starts_with(&slot) {
            return true;
        }
    }
    let cur_s = cur.to_string_lossy();
    let needle = format!("/store/pkg/{}/", slot.file_name().and_then(|n| n.to_str()).unwrap_or(""));
    cur_s.contains(&needle)
}

fn pkg_actual(
    present: bool,
    links: Vec<String>,
    hash: String,
    store_root: &Path,
    name: &str,
) -> PkgActual {
    let hashed = realization_dir(store_root, name, &hash);
    let tree = if is_realization_id(&hash) && hashed.is_dir() {
        hashed
    } else {
        slot_dir(store_root, name)
    };
    let components = read_components(&tree).unwrap_or_default();
    let advisories = read_advisories(store_root, name, &hash);
    PkgActual {
        present,
        links,
        removable: true,
        url: String::new(),
        hash,
        realizations: list_realizations(store_root, name).unwrap_or_default(),
        needs: Vec::new(),
        description: String::new(),
        home: String::new(),
        source: None,
        components,
        advisories,
        verified: None,
    }
}

fn is_old_layout(slot: &Path) -> bool {
    let bin = slot.join("bin");
    match fs::symlink_metadata(&bin) {
        Ok(m) => m.file_type().is_dir(),
        Err(_) => false,
    }
}

fn skip_slot_entry(name: &str) -> bool {
    name == LIVE_NAME || is_realization_id(name)
}

/// Pack-file children of a slot (not `live` / hash dirs).
fn pack_entries(slot: &Path) -> Result<Vec<String>> {
    if !slot.is_dir() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for e in fs::read_dir(slot)? {
        let name = e?.file_name().to_string_lossy().into_owned();
        if skip_slot_entry(&name) {
            continue;
        }
        names.push(name);
    }
    names.sort();
    Ok(names)
}

pub fn list_realization_ids(catalog_root: &Path, name: &str) -> Result<Vec<String>> {
    let slot = slot_dir(catalog_root, name);
    if !slot.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for e in fs::read_dir(&slot)? {
        let e = e?;
        let name = e.file_name().to_string_lossy().into_owned();
        if is_realization_id(&name) && e.path().is_dir() {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

pub fn list_realizations(catalog_root: &Path, name: &str) -> Result<Vec<PkgRealization>> {
    let live = live_hash(catalog_root, name)?;
    let mut ids = list_realization_ids(catalog_root, name)?;
    if ids.is_empty() && is_old_layout(&slot_dir(catalog_root, name)) {
        if let Ok(h) = hash_tree(&slot_dir(catalog_root, name)) {
            ids.push(h);
        }
    }
    let mut out = Vec::new();
    for hash in ids {
        let tree =
            if is_realization_id(&hash) && realization_dir(catalog_root, name, &hash).is_dir() {
                realization_dir(catalog_root, name, &hash)
            } else {
                slot_dir(catalog_root, name)
            };
        let bins = bin_names(&tree.join("bin")).unwrap_or_default();
        let index = index_blurb(&tree.join("INDEX.md"));
        let components = read_components(&tree).unwrap_or_default();
        let linked = live.as_ref().map(|l| l == &hash).unwrap_or(false);
        out.push(PkgRealization { hash, linked, bins, index, components });
    }
    Ok(out)
}

fn index_blurb(path: &Path) -> String {
    let Ok(s) = fs::read_to_string(path) else {
        return String::new();
    };
    let mut lines = s.lines().take(3).collect::<Vec<_>>().join("\n");
    if lines.len() > 240 {
        lines.truncate(240);
    }
    lines
}

/// Hash realization `hash` of `name` again.
pub fn verify_tree(store_root: &Path, name: &str, hash: &str) -> Result<PkgVerified> {
    let got = hash_tree(&realization_dir(store_root, name, hash))?;
    let ok = got == hash;
    Ok(PkgVerified { ok, at: crate::now_rfc3339(), got: if ok { String::new() } else { got } })
}

fn live_hash(catalog_root: &Path, name: &str) -> Result<Option<String>> {
    let p = slot_dir(catalog_root, name).join(LIVE_NAME);
    if let Ok(t) = fs::read_link(&p) {
        let s = t.to_string_lossy().into_owned();
        let s = s.trim_end_matches('/').rsplit('/').next().unwrap_or(&s).to_string();
        if is_realization_id(&s) {
            return Ok(Some(s));
        }
    }
    Ok(None)
}

fn tree_hash_id(store_root: &Path, name: &str, tree: &Path) -> Result<String> {
    let slot = slot_dir(store_root, name);
    if let Ok(rel) = tree.strip_prefix(&slot) {
        let rel = rel.to_string_lossy();
        if is_realization_id(&rel) {
            return Ok(rel.into_owned());
        }
        if rel.is_empty() && is_old_layout(&slot) {
            return hash_tree(tree);
        }
    }
    if tree.is_dir() {
        return hash_tree(tree);
    }
    Ok(String::new())
}

/// Locate the pack tree to link. Empty `hash` is discovery: unique
/// realization, or the pre-T32 layout (`store/pkg/<name>/bin` as a
/// real directory).
pub fn resolve_tree(catalog_root: &Path, name: &str, hash: &str) -> Result<PathBuf> {
    let slot = slot_dir(catalog_root, name);
    let pin = hash.trim();
    if !pin.is_empty() {
        parse_pin(pin)?;
        let hashed = realization_dir(catalog_root, name, pin);
        if hashed.is_dir() {
            return Ok(hashed);
        }
        if is_old_layout(&slot) {
            let got = hash_tree(&slot)?;
            if got == pin {
                return Ok(slot);
            }
            return Err(Error::hint(
                format!("pkg:{name} hash {pin} does not match store ({got})"),
                "oath schema pkg",
            ));
        }
        return Err(Error::hint(
            format!("no store for pkg:{name} hash {pin}"),
            format!("oath get pkg:{name}"),
        ));
    }
    let ids = list_realization_ids(catalog_root, name)?;
    match ids.len() {
        1 => Ok(realization_dir(catalog_root, name, &ids[0])),
        0 => {
            if is_old_layout(&slot) {
                Ok(slot)
            } else {
                Err(Error::hint(format!("no store for pkg:{name}"), format!("oath get pkg:{name}")))
            }
        }
        _ => Err(Error::hint(
            format!("pkg:{name} has {} realizations; set hash=", ids.len()),
            "oath schema pkg",
        )),
    }
}

fn parse_pin(pin: &str) -> Result<()> {
    if is_realization_id(pin) {
        Ok(())
    } else {
        Err(Error::hint(format!("not a pack hash: {pin}"), "oath schema pkg"))
    }
}

/// Put the pack tree `src` into `slot` (a `store/pkg/<name>` directory) as
/// realization `hash`, whole or not at all: it is copied beside the slot
/// (`.<name>.<hash>.part`), hashed again, and renamed into place. A
/// realization that exists is never written into.
pub fn put_tree(slot: &Path, hash: &str, src: &Path) -> Result<PathBuf> {
    parse_pin(hash)?;
    let dest = slot.join(hash);
    if dest.is_dir() {
        return Ok(dest);
    }
    let (Some(parent), Some(name)) = (slot.parent(), slot.file_name()) else {
        return Err(Error::Msg(format!("bad store slot {}", slot.display())));
    };
    let part = parent.join(format!(".{}.{hash}.part", name.to_string_lossy()));
    let _ = fs::remove_dir_all(&part);
    copy_tree(src, &part)?;
    let got = hash_tree(&part)?;
    if got != hash {
        let _ = fs::remove_dir_all(&part);
        return Err(Error::security(
            format!("a copy of {} hashes to {got}, not {hash}", src.display()),
            "the tree changed while it was copied; nothing was stored",
        ));
    }
    fs::create_dir_all(slot)?;
    fs::rename(&part, &dest)?;
    Ok(dest)
}

/// The package realization a program runs from: `path` (a service's
/// `exec[0]`, usually `/bin/<file>`) followed through its links into
/// `<catalog_root>/store/pkg/<name>/<hash>/`. `None` for a program outside
/// the store, or one that is not there.
pub fn program_realization(catalog_root: &Path, path: &Path) -> Option<(String, String)> {
    let real = fs::canonicalize(path).ok()?;
    let store = fs::canonicalize(catalog_root.join("store/pkg")).ok()?;
    let mut parts = real.strip_prefix(&store).ok()?.components();
    let name = parts.next()?.as_os_str().to_str()?.to_string();
    let hash = parts.next()?.as_os_str().to_str()?.to_string();
    is_realization_id(&hash).then_some((name, hash))
}

/// `oath store`: put a tree made elsewhere (a directory, or a tar of one)
/// into the store as a realization of `pkg:<name>`, with the refusals a
/// fetch has, and return its hash. No object and no link changes: the
/// caller pins the hash (`oath set pkg:<name> hash=…`) and applies.
pub fn store_pkg(catalog_root: &Path, name: &str, src: &Path) -> Result<String> {
    let _lock = crate::Catalog::open(catalog_root)?.lock()?;
    let slot = slot_dir(catalog_root, name);
    if src.is_dir() {
        let hash = hash_tree(src)?;
        put_tree(&slot, &hash, src)?;
        return Ok(hash);
    }
    if !src.is_file() || !looks_like_tar(src) {
        return Err(Error::hint(
            format!("not a directory or a tar: {}", src.display()),
            "oath store pkg:<name> <dir|file.tar>",
        ));
    }
    let tmp = catalog_root.join("store/pkg").join(format!(".{name}.store"));
    let _ = fs::remove_dir_all(&tmp);
    let tree = tmp.join("tree");
    fs::create_dir_all(&tree)?;
    let got = extract_tar(src, &tree)
        .and_then(|()| hash_tree(&tree))
        .and_then(|hash| put_tree(&slot, &hash, &tree).map(|_| hash));
    let _ = fs::remove_dir_all(&tmp);
    got
}

/// Copy `src` (a pack directory) into the hashed store. Also writes
/// `live` + slot compat links. Optional host cache gets the same tree.
pub fn install_tree(
    catalog_root: &Path,
    name: &str,
    src: &Path,
    cache: Option<&Path>,
    pin: &str,
) -> Result<String> {
    let (got, dest) = store_tree(catalog_root, name, src, pin)?;
    if let Some(cache) = cache {
        put_tree(&cache.join("pkg").join(name), &got, &dest)?;
    }
    write_slot_links(catalog_root, name, &got)?;
    if !pin.is_empty() {
        pin_desired_hash(catalog_root, name, &got)?;
    }
    Ok(got)
}

/// Put a pack tree in the store under its hash, checked against `pin`
/// when there is one. Nothing links it and no object changes.
fn store_tree(catalog_root: &Path, name: &str, src: &Path, pin: &str) -> Result<(String, PathBuf)> {
    if !src.is_dir() {
        return Err(Error::hint(
            format!("not a pack directory: {}", src.display()),
            "oath schema pkg",
        ));
    }
    let got = hash_tree(src)?;
    if !pin.is_empty() && pin != got {
        return Err(Error::hint(
            format!("pkg:{name} hash {pin} does not match bits ({got})"),
            "oath schema pkg",
        ));
    }
    let dest = put_tree(&slot_dir(catalog_root, name), &got, src)?;
    Ok((got, dest))
}

/// Hash an old-layout slot (`store/pkg/<name>/{bin,lib,…}`) into
/// `store/pkg/<name>/<hash>/`, then write live/compat links.
pub fn promote_slot(catalog_root: &Path, name: &str, cache: Option<&Path>) -> Result<String> {
    let slot = slot_dir(catalog_root, name);
    if !slot.is_dir() {
        return Err(Error::hint(
            format!("no store for pkg:{name}"),
            format!("oath get pkg:{name}"),
        ));
    }
    let entries = pack_entries(&slot)?;
    if entries.is_empty() {
        let ids = list_realization_ids(catalog_root, name)?;
        if ids.len() == 1 {
            write_slot_links(catalog_root, name, &ids[0])?;
            pin_desired_hash(catalog_root, name, &ids[0])?;
            if let Some(cache) = cache {
                let tree = realization_dir(catalog_root, name, &ids[0]);
                put_tree(&cache.join("pkg").join(name), &ids[0], &tree)?;
            }
            return Ok(ids[0].clone());
        }
        if let Some(h) = live_hash(catalog_root, name)? {
            write_slot_links(catalog_root, name, &h)?;
            pin_desired_hash(catalog_root, name, &h)?;
            return Ok(h);
        }
        return Err(Error::hint(
            format!("no store for pkg:{name}"),
            format!("oath get pkg:{name}"),
        ));
    }
    let stage = slot.parent().unwrap_or(Path::new(".")).join(format!(".{name}.promote"));
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage)?;
    for e in &entries {
        fs::rename(slot.join(e), stage.join(e))?;
    }
    let got = hash_tree(&stage)?;
    let dest = realization_dir(catalog_root, name, &got);
    if dest.is_dir() {
        let _ = fs::remove_dir_all(&stage);
    } else {
        fs::rename(&stage, &dest)?;
    }
    if let Some(cache) = cache {
        put_tree(&cache.join("pkg").join(name), &got, &dest)?;
    }
    write_slot_links(catalog_root, name, &got)?;
    pin_desired_hash(catalog_root, name, &got)?;
    Ok(got)
}

/// Promote every old-layout slot under `store/pkg`.
pub fn promote_store(catalog_root: &Path, cache: Option<&Path>) -> Result<Vec<(String, String)>> {
    let root = catalog_root.join("store").join("pkg");
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for e in fs::read_dir(&root)? {
        let e = e?;
        if e.path().is_dir() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.starts_with('.') {
                continue;
            }
            names.push(n);
        }
    }
    names.sort();
    let mut out = Vec::new();
    for name in names {
        match promote_slot(catalog_root, &name, cache) {
            Ok(h) => out.push((name, h)),
            Err(e) => {
                return Err(Error::Msg(format!("promote pkg:{name}: {e}")));
            }
        }
    }
    Ok(out)
}

pub fn write_slot_links(catalog_root: &Path, name: &str, hash: &str) -> Result<()> {
    parse_pin(hash)?;
    let slot = slot_dir(catalog_root, name);
    let dest = realization_dir(catalog_root, name, hash);
    if !dest.is_dir() {
        return Err(Error::hint(
            format!("no store for pkg:{name} hash {hash}"),
            format!("oath get pkg:{name}"),
        ));
    }
    fs::create_dir_all(&slot)?;
    let live = slot.join(LIVE_NAME);
    replace_symlink(&live, hash)?;
    let mut top = Vec::new();
    for e in fs::read_dir(&dest)? {
        top.push(e?.file_name().to_string_lossy().into_owned());
    }
    top.sort();
    for n in top {
        if skip_slot_entry(&n) {
            continue;
        }
        replace_symlink(&slot.join(&n), &format!("{LIVE_NAME}/{n}"))?;
    }
    Ok(())
}

fn replace_symlink(path: &Path, target: &str) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() || meta.file_type().is_file() {
            fs::remove_file(path)?;
        } else if meta.file_type().is_dir() {
            return Err(Error::hint(
                format!("refusing to replace directory {} with a symlink", path.display()),
                "oath schema pkg",
            ));
        }
    }
    symlink(target, path)
        .map_err(|e| Error::Msg(format!("symlink {} -> {target}: {e}", path.display())))?;
    Ok(())
}

/// After packs are pinned, copy each needed slot's live hash onto
/// `needs[].hash`. Pack-time lockfile; apply does not do this.
pub fn sync_need_hashes(catalog_root: &Path) -> Result<()> {
    use serde_json::{json, Value};
    let dir = catalog_root.join("objects").join("pkg");
    if !dir.is_dir() {
        return Ok(());
    }
    let mut live: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for e in fs::read_dir(&dir)? {
        let e = e?;
        if !e.path().is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        let desired =
            crate::read_json::<Value>(&e.path().join("desired.json")).unwrap_or(Value::Null);
        let applied =
            crate::read_json::<Value>(&e.path().join("applied.json")).unwrap_or(Value::Null);
        let h = desired
            .get("hash")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| applied.get("hash").and_then(|x| x.as_str()).filter(|s| !s.is_empty()))
            .unwrap_or("")
            .to_string();
        if !h.is_empty() {
            live.insert(format!("pkg:{name}"), h.clone());
            live.insert(name, h);
        }
    }
    for e in fs::read_dir(&dir)? {
        let e = e?;
        if !e.path().is_dir() {
            continue;
        }
        for file in ["desired.json", "applied.json"] {
            let p = e.path().join(file);
            let Ok(mut v) = crate::read_json::<Value>(&p) else {
                continue;
            };
            let Some(arr) = v.get_mut("needs").and_then(|n| n.as_array_mut()) else {
                continue;
            };
            let mut changed = false;
            for item in arr.iter_mut() {
                let id = if let Some(s) = item.as_str() {
                    normalize_need(s)
                } else {
                    item.get("id").and_then(|x| x.as_str()).map(normalize_need).unwrap_or_default()
                };
                if id.is_empty() {
                    continue;
                }
                let Some(h) = live.get(&id) else {
                    continue;
                };
                if item.is_string() {
                    *item = json!({ "id": id, "hash": h });
                    changed = true;
                } else if let Some(o) = item.as_object_mut() {
                    if o.get("hash").and_then(|x| x.as_str()) != Some(h.as_str()) {
                        o.insert("id".into(), json!(id));
                        o.insert("hash".into(), json!(h));
                        changed = true;
                    }
                }
            }
            if changed {
                write_json(&p, &v)?;
            }
        }
    }
    Ok(())
}

/// Pin `pkg:<name>` to `hash` in desired, and in applied when it has been
/// applied (the image packer links what it pins).
pub fn pin_desired_hash(catalog_root: &Path, name: &str, hash: &str) -> Result<()> {
    let dir = catalog_root.join("objects").join("pkg").join(name);
    for file in ["desired.json", "applied.json"] {
        let p = dir.join(file);
        if !p.is_file() {
            continue;
        }
        let mut v: serde_json::Value = crate::read_json(&p)?;
        v["hash"] = serde_json::Value::String(hash.to_string());
        write_json(&p, &v)?;
    }
    Ok(())
}

/// Copy a pack tree. A file gets the mode its hash records (0755 when any
/// exec bit is set, else 0644), so the store never keeps a setuid,
/// setgid or writable bit the hash cannot see.
pub fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if !src.is_dir() {
        return Err(Error::hint(format!("not a directory: {}", src.display()), "oath schema pkg"));
    }
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        let ft = e.file_type()?;
        if ft.is_dir() {
            copy_tree(&e.path(), &to)?;
        } else if ft.is_symlink() {
            let target = fs::read_link(e.path())?;
            if to.symlink_metadata().is_ok() {
                fs::remove_file(&to)?;
            }
            symlink(&target, &to).map_err(|err| {
                Error::Msg(format!("symlink {} -> {}: {err}", to.display(), target.display()))
            })?;
        } else if !ft.is_file() {
            return Err(Error::hint(
                format!("pack contains a special file: {}", e.path().display()),
                "oath schema pkg",
            ));
        } else {
            if let Some(p) = to.parent() {
                fs::create_dir_all(p)?;
            }
            fs::copy(e.path(), &to)?;
            let exec = fs::metadata(&to)?.permissions().mode() & 0o111 != 0;
            fs::set_permissions(&to, fs::Permissions::from_mode(if exec { 0o755 } else { 0o644 }))?;
        }
    }
    Ok(())
}

/// True when the slot already has the pin (or any tree, if unpinned).
/// The names two of `pkgs` (present packages, `(name, hash)`) would both
/// link: `(/bin/<n> or /lib/<n>, one, the other)`. Linking refuses the
/// second, so an update checks the packages as it will leave them before
/// it changes anything. A package whose tree is not in the store yet is
/// not checked.
pub fn link_clashes(
    catalog_root: &Path,
    pkgs: &[(String, String)],
) -> Vec<(String, String, String)> {
    let mut owner: BTreeMap<String, String> = BTreeMap::new();
    let mut clashes = Vec::new();
    for (name, hash) in pkgs {
        let tree = realization_dir(catalog_root, name, hash);
        if hash.is_empty() || !tree.is_dir() {
            continue;
        }
        let bins = bin_names(&tree.join("bin")).unwrap_or_default();
        let libs = lib_names(&tree.join("lib")).unwrap_or_default();
        let bins = bins.iter().map(|n| format!("/bin/{n}"));
        for path in bins.chain(libs.iter().map(|n| format!("/lib/{n}"))) {
            match owner.get(&path) {
                Some(other) if other != name => {
                    clashes.push((path, other.clone(), name.clone()));
                }
                _ => {
                    owner.insert(path, name.clone());
                }
            }
        }
    }
    clashes
}

pub fn store_present(catalog_root: &Path, name: &str, hash: &str) -> bool {
    if !hash.is_empty() {
        return realization_dir(catalog_root, name, hash).is_dir()
            || (is_old_layout(&slot_dir(catalog_root, name))
                && hash_tree(&slot_dir(catalog_root, name)).ok().as_deref() == Some(hash));
    }
    !list_realization_ids(catalog_root, name).ok().unwrap_or_default().is_empty()
        || is_old_layout(&slot_dir(catalog_root, name))
}

pub fn adv_path(catalog_root: &Path, name: &str, hash: &str) -> PathBuf {
    adv_path_store(&catalog_root.join("store"), name, hash)
}

/// Store-root helper: guest `/oath/store`, host `~/.local/share/oath/store`.
pub fn adv_path_store(store: &Path, name: &str, hash: &str) -> PathBuf {
    store.join("adv").join(name).join(format!("{hash}.json"))
}

/// Upstream components from a pack tree. `None` if the file is absent.
pub fn read_components(tree: &Path) -> Option<Vec<PkgComponent>> {
    let p = tree.join(COMPONENTS_NAME);
    if !p.is_file() {
        return None;
    }
    parse_components(&fs::read_to_string(&p).ok()?)
}

/// `{"components": [...]}`, or the bare list `cargo make pkgs` writes.
fn parse_components(s: &str) -> Option<Vec<PkgComponent>> {
    let v: serde_json::Value = serde_json::from_str(s).ok()?;
    let arr = match v.as_array() {
        Some(arr) => arr,
        None => v.get("components")?.as_array()?,
    };
    let mut out = Vec::new();
    for e in arr {
        let name = e.get("name")?.as_str()?.trim();
        let version = e.get("version")?.as_str()?.trim();
        let purl = e.get("purl")?.as_str()?.trim();
        if name.is_empty() || version.is_empty() || !purl.starts_with("pkg:") {
            return None;
        }
        let cpe = e.get("cpe").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        out.push(PkgComponent {
            name: name.to_string(),
            version: version.to_string(),
            purl: purl.to_string(),
            cpe,
        });
    }
    Some(out)
}

/// Advisories for a pack hash. `None` if never fetched.
pub fn read_advisories(catalog_root: &Path, name: &str, hash: &str) -> Option<Vec<PkgAdvisory>> {
    read_advisories_store(&catalog_root.join("store"), name, hash)
}

pub fn read_advisories_store(store: &Path, name: &str, hash: &str) -> Option<Vec<PkgAdvisory>> {
    if hash.is_empty() || !is_realization_id(hash) {
        return None;
    }
    let p = adv_path_store(store, name, hash);
    if !p.is_file() {
        return None;
    }
    parse_advisories(&fs::read_to_string(&p).ok()?, name, hash)
}

fn parse_advisories(s: &str, name: &str, hash: &str) -> Option<Vec<PkgAdvisory>> {
    let v: serde_json::Value = serde_json::from_str(s).ok()?;
    let file_hash = v.get("hash").and_then(|x| x.as_str()).unwrap_or("");
    if !file_hash.is_empty() && file_hash != hash {
        return None;
    }
    let file_pkg = v.get("pkg").and_then(|x| x.as_str()).unwrap_or("");
    let expect = format!("pkg:{name}");
    if !file_pkg.is_empty() && file_pkg != expect {
        return None;
    }
    let arr = v.get("advisories")?.as_array()?;
    let mut out = Vec::new();
    for e in arr {
        let id = e.get("id")?.as_str()?.trim();
        let url = e.get("url").and_then(|x| x.as_str()).unwrap_or("").trim();
        if id.is_empty() {
            continue;
        }
        let summary = e.get("summary").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        out.push(PkgAdvisory { id: id.to_string(), url: url.to_string(), summary });
    }
    Some(out)
}

/// Write a canonical advisory file for a pack hash (T44). Sorted by
/// `id`. Empty list is a successful "none at last query", not unknown.
pub fn write_advisories(
    catalog_root: &Path,
    name: &str,
    hash: &str,
    advisories: Vec<PkgAdvisory>,
) -> Result<()> {
    write_advisories_store(&catalog_root.join("store"), name, hash, advisories)
}

pub fn write_advisories_store(
    store: &Path,
    name: &str,
    hash: &str,
    advisories: Vec<PkgAdvisory>,
) -> Result<()> {
    if !is_realization_id(hash) {
        return Err(Error::hint(format!("not a pack hash: {hash}"), "oath schema pkg"));
    }
    let mut advisories = advisories;
    advisories.sort_by(|a, b| a.id.cmp(&b.id));
    advisories.dedup_by(|a, b| a.id == b.id);
    #[derive(serde::Serialize)]
    struct File<'a> {
        hash: &'a str,
        pkg: String,
        advisories: &'a [PkgAdvisory],
    }
    crate::write_json(
        &adv_path_store(store, name, hash),
        &File { hash, pkg: format!("pkg:{name}"), advisories: &advisories },
    )
}

/// T44 fetch: curl `{origin}/adv/{name}/{hash}.json`. Failure is Ok
/// (apply still converges; previous file stays).
pub fn fetch_adv(catalog_root: &Path, name: &str, url: &str, hash: &str) -> Result<()> {
    let adv_url = fetch_adv_url(url, name, hash);
    if adv_url.is_empty() {
        return Ok(());
    }
    let tmpdir = catalog_root.join("store/adv").join(format!(".{name}.fetch"));
    let _ = fs::remove_dir_all(&tmpdir);
    if let Err(e) = fs::create_dir_all(&tmpdir) {
        let _ = fs::remove_dir_all(&tmpdir);
        return Err(Error::Msg(format!("adv tmp: {e}")));
    }
    let blob = tmpdir.join("blob");
    // Quiet: most packs have no advisory file, and its 404 is not a failure.
    let st = Command::new(find_bin("curl"))
        .args(["-fsL", "-o", blob.to_str().unwrap(), &adv_url])
        .stdout(std::io::stderr())
        .status()
        .map_err(|e| Error::Msg(format!("curl: {e}")));
    let st = match st {
        Ok(s) => s,
        Err(_) => {
            let _ = fs::remove_dir_all(&tmpdir);
            return Ok(());
        }
    };
    if !st.success() {
        let _ = fs::remove_dir_all(&tmpdir);
        return Ok(());
    }
    let Ok(body) = fs::read_to_string(&blob) else {
        let _ = fs::remove_dir_all(&tmpdir);
        return Ok(());
    };
    if parse_advisories(&body, name, hash).is_none() {
        let _ = fs::remove_dir_all(&tmpdir);
        return Ok(());
    }
    let dest = adv_path(catalog_root, name, hash);
    if let Some(p) = dest.parent() {
        let _ = fs::create_dir_all(p);
    }
    let _ = fs::write(&dest, body);
    let _ = fs::remove_dir_all(&tmpdir);
    Ok(())
}

/// T20 fetch: curl `url` (or `{url}/pkg/{name}/{hash}.tar`) into the store.
/// A `.tar` / gzip is a pack tree; a single file becomes `bin/<name>`.
/// With `file` (R23), `url` is one file and the tree is that file alone at
/// the path `file` names. Plan pre-build uses this with `present` still
/// false.
pub fn fetch_pkg(catalog_root: &Path, name: &str, url: &str, file: &str, hash: &str) -> Result<()> {
    fetch_tree(catalog_root, name, url, file, hash, true)
}

/// `fetch_pkg`; with `link` false the pack only goes in the store, and
/// neither `live` nor the object's pin moves.
fn fetch_tree(
    catalog_root: &Path,
    name: &str,
    url: &str,
    file: &str,
    hash: &str,
    link: bool,
) -> Result<()> {
    if store_present(catalog_root, name, hash) {
        return Ok(());
    }
    if !file.is_empty() && !is_tree_path(file) {
        return Err(Error::hint(
            format!("pkg:{name} file {file:?} is not a relative path in the tree"),
            "oath schema pkg",
        ));
    }
    let url = if file.is_empty() { fetch_url(url, name, hash) } else { url.trim().to_string() };
    if url.is_empty() {
        return Ok(());
    }
    let tmpdir = catalog_root.join("store/pkg").join(format!(".{name}.fetch"));
    let _ = fs::remove_dir_all(&tmpdir);
    fs::create_dir_all(&tmpdir)?;
    let blob = tmpdir.join("blob");
    // A network error, or a stall under 1 KB/s for 30 s, is tried again
    // (canto's link reset a 99 MB pack twice on 2026-10-28); an HTTP error
    // is the origin's answer.
    let mut tries = 0;
    let st = loop {
        let st = Command::new(find_bin("curl"))
            .args(["-fsSL", "--speed-limit", "1024", "--speed-time", "30", "-o"])
            .arg(&blob)
            .arg(&url)
            .stdout(std::io::stderr())
            .status()
            .map_err(|e| Error::Msg(format!("curl: {e}")))?;
        tries += 1;
        if st.success() || tries == FETCH_TRIES || !curl_transient(st.code()) {
            break st;
        }
        eprintln!("fetch pkg:{name}: curl exit {}, trying again", st.code().unwrap_or(-1));
        std::thread::sleep(std::time::Duration::from_secs(2 * tries));
    };
    if !st.success() {
        let _ = fs::remove_dir_all(&tmpdir);
        return Err(Error::hint(format!("fetch pkg:{name} failed"), "oath schema pkg"));
    }
    let result = if !file.is_empty() {
        let tree = tmpdir.join("tree");
        let dest = tree.join(file);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::rename(&blob, &dest)?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o644))?;
        tree
    } else if looks_like_tar(&blob) {
        let tree = tmpdir.join("tree");
        fs::create_dir_all(&tree)?;
        extract_tar(&blob, &tree)?;
        tree
    } else {
        // A single file is the pack's `bin/<name>`.
        let tree = tmpdir.join("tree");
        let dest = tree.join("bin").join(name);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::rename(&blob, &dest)?;
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))?;
        tree
    };
    let result = if link {
        install_tree(catalog_root, name, &result, None, hash)
    } else {
        store_tree(catalog_root, name, &result, hash).map(|(got, _)| got)
    };
    let _ = fs::remove_dir_all(&tmpdir);
    result.map(|_| ())
}

const FETCH_TRIES: u64 = 3;

/// curl's exit codes for a network that failed partway (connect, partial
/// file, timeout or stall, TLS connect, empty reply, send, receive,
/// HTTP/2 stream), not an answer from the origin.
fn curl_transient(code: Option<i32>) -> bool {
    matches!(code, Some(7 | 18 | 28 | 35 | 52 | 55 | 56 | 92))
}

/// `host:local.origins` from its spec, in order; empty when it has none.
pub fn host_origins(catalog_root: &Path) -> Vec<String> {
    let id = crate::ObjectId::new(crate::KIND_HOST, "local");
    let Ok(obj) = crate::Catalog::open(catalog_root).and_then(|c| c.get(&id)) else {
        return Vec::new();
    };
    obj.desired
        .get("origins")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|o| o.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

/// Fetch a pack the store does not have from where its consumer looks:
/// the pkg's own `url`, else (with a pin, no `file`) each of
/// `host:local.origins` in order until one serves the pinned tree.
/// Returns where it came from; `None` when the store already had it or
/// there is nowhere to look.
pub fn fetch_pkg_any(
    catalog_root: &Path,
    name: &str,
    url: &str,
    file: &str,
    hash: &str,
) -> Result<Option<String>> {
    fetch_any(catalog_root, name, url, file, hash, true)
}

/// `fetch_pkg_any` into the store alone: `live` and the object's pin stay
/// where they are until apply converges the package. `oath update` fetches
/// every pack this way before it changes anything.
pub fn prefetch_pkg_any(
    catalog_root: &Path,
    name: &str,
    url: &str,
    file: &str,
    hash: &str,
) -> Result<Option<String>> {
    fetch_any(catalog_root, name, url, file, hash, false)
}

fn fetch_any(
    catalog_root: &Path,
    name: &str,
    url: &str,
    file: &str,
    hash: &str,
    link: bool,
) -> Result<Option<String>> {
    if store_present(catalog_root, name, hash) {
        return Ok(None);
    }
    if !url.trim().is_empty() {
        fetch_tree(catalog_root, name, url, file, hash, link)?;
        return Ok(Some(url.trim().to_string()));
    }
    if !file.is_empty() || !is_realization_id(hash) {
        return Ok(None);
    }
    let origins = host_origins(catalog_root);
    let mut failed = Vec::new();
    for o in &origins {
        match fetch_tree(catalog_root, name, o, "", hash, link) {
            Ok(()) if store_present(catalog_root, name, hash) => return Ok(Some(o.clone())),
            Ok(()) => failed.push(format!("{o}: no tree")),
            Err(e) => failed.push(format!("{o}: {e}")),
        }
    }
    if failed.is_empty() {
        return Ok(None);
    }
    Err(Error::hint(
        format!("fetch pkg:{name} {hash}: no origin served it ({})", failed.join("; ")),
        "oath get host:local (origins); oath schema host",
    ))
}

/// A relative path of plain components (`src/claude`).
fn is_tree_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && p.split('/').all(|c| !c.is_empty() && c != "." && c != "..")
}

/// A program in /bin, /usr/bin or PATH (bare name if none: the spawn
/// error says which).
pub fn find_bin(name: &str) -> PathBuf {
    for dir in ["/bin", "/usr/bin"] {
        let p = PathBuf::from(dir).join(name);
        if p.is_file() {
            return p;
        }
    }
    if let Some(p) = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path).find_map(|d| {
            let c = d.join(name);
            c.is_file().then_some(c)
        })
    }) {
        return p;
    }
    PathBuf::from(name)
}

fn looks_like_tar(path: &Path) -> bool {
    let Ok(mut fd) = fs::File::open(path) else {
        return false;
    };
    let mut hdr = [0u8; 262];
    let Ok(n) = fd.read(&mut hdr) else {
        return false;
    };
    if n >= 2 && hdr[0] == 0x1f && hdr[1] == 0x8b {
        return true;
    }
    n >= 262 && &hdr[257..262] == b"ustar"
}

fn looks_like_gzip(path: &Path) -> bool {
    let Ok(mut fd) = fs::File::open(path) else {
        return false;
    };
    let mut b = [0u8; 2];
    matches!(fd.read(&mut b), Ok(2) if b[0] == 0x1f && b[1] == 0x8b)
}

fn extract_tar(blob: &Path, dest: &Path) -> Result<()> {
    let gzip = looks_like_gzip(blob);
    let tar = find_bin("tar");
    let mut cmd = Command::new(&tar);
    cmd.arg("-C").arg(dest);
    if gzip {
        cmd.arg("-xzf");
    } else {
        cmd.arg("-xf");
    }
    cmd.arg(blob);
    let st = cmd.stdout(std::io::stderr()).status().map_err(|e| Error::Msg(format!("tar: {e}")))?;
    if !st.success() {
        return Err(Error::hint(
            format!("tar could not unpack {}", blob.display()),
            "oath schema pkg",
        ));
    }
    Ok(())
}

pub fn ingest_file(
    catalog_root: &Path,
    name: &str,
    bytes: &[u8],
    cache: Option<&Path>,
    pin: &str,
) -> Result<String> {
    let tmp = catalog_root.join("store").join("pkg").join(format!(".{name}.ingest"));
    let _ = fs::remove_dir_all(&tmp);
    let dest = tmp.join("bin").join(name);
    fs::create_dir_all(dest.parent().unwrap())?;
    fs::write(&dest, bytes)?;
    use std::os::unix::fs::PermissionsExt;
    let mut perm = fs::metadata(&dest)?.permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&dest, perm)?;
    let got = install_tree(catalog_root, name, &tmp, cache, pin);
    let _ = fs::remove_dir_all(&tmp);
    got
}

#[cfg(test)]
mod needs_tests {
    use super::*;
    use crate::kinds::Pkg;

    const H: &str = "sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const H2: &str = "sha256-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn p(present: bool, hash: &str, needs: &[(&str, &str)]) -> Pkg {
        Pkg {
            present,
            url: String::new(),
            file: String::new(),
            hash: hash.to_string(),
            needs: needs
                .iter()
                .map(|(id, h)| PkgNeed { id: (*id).into(), hash: (*h).into() })
                .collect(),
            description: String::new(),
            home: String::new(),
            source: None,
        }
    }

    #[test]
    fn needs_unknown_pack() {
        let pkgs = vec![
            ("pkg:curl".into(), p(true, H, &[])),
            ("pkg:app".into(), p(true, H, &[("pkg:libfoo", H)])),
        ];
        let err = check_needs(&pkgs).unwrap_err().to_string();
        assert!(err.contains("not in the catalog"), "{err}");
    }

    #[test]
    fn need_off_while_user_present() {
        let pkgs = vec![
            ("pkg:libgl".into(), p(false, H, &[("pkg:libc", H)])),
            ("pkg:libc".into(), p(true, H, &[])),
            ("pkg:game".into(), p(true, H, &[("pkg:libgl", H)])),
        ];
        let err = check_needs(&pkgs).unwrap_err().to_string();
        assert!(err.contains("needed by pkg:game"), "{err}");
    }

    #[test]
    fn both_off_ok() {
        let pkgs = vec![
            ("pkg:libgl".into(), p(false, H, &[("pkg:libc", H)])),
            ("pkg:libc".into(), p(true, H, &[])),
            ("pkg:game".into(), p(false, H, &[("pkg:libgl", H)])),
        ];
        check_needs(&pkgs).unwrap();
    }

    #[test]
    fn cycle_refuses() {
        let pkgs = vec![
            ("pkg:a".into(), p(true, H, &[("pkg:b", H)])),
            ("pkg:b".into(), p(true, H, &[("pkg:a", H)])),
        ];
        let err = check_needs(&pkgs).unwrap_err().to_string();
        assert!(err.contains("cycle"), "{err}");
    }

    #[test]
    fn hash_mismatch_refuses() {
        let pkgs = vec![
            ("pkg:libgl".into(), p(true, H2, &[])),
            ("pkg:game".into(), p(true, H, &[("pkg:libgl", H)])),
        ];
        let err = check_needs(&pkgs).unwrap_err().to_string();
        assert!(err.contains("live is"), "{err}");
    }

    #[test]
    fn missing_need_hash_refuses() {
        let pkgs = vec![
            ("pkg:libgl".into(), p(true, H, &[])),
            ("pkg:game".into(), p(true, H, &[("pkg:libgl", "")])),
        ];
        let err = check_needs(&pkgs).unwrap_err().to_string();
        assert!(err.contains("hash"), "{err}");
    }

    #[test]
    fn components_json_is_in_tree_hash() {
        let d = tempfile::tempdir().unwrap();
        let tree = d.path().join("tree");
        fs::create_dir_all(tree.join("bin")).unwrap();
        fs::write(tree.join("bin/hello"), b"hello\n").unwrap();
        let h1 = hash_tree(&tree).unwrap();
        fs::write(
            tree.join(COMPONENTS_NAME),
            "{\n  \"components\": [\n    {\n      \"name\": \"hello\",\n      \"version\": \"1.0\",\n      \"purl\": \"pkg:generic/hello@1.0\"\n    }\n  ]\n}\n",
        )
        .unwrap();
        let h2 = hash_tree(&tree).unwrap();
        assert_ne!(h1, h2, "components.json must change the pack hash");
        let c = read_components(&tree).expect("components");
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "hello");
        assert_eq!(c[0].version, "1.0");
        assert_eq!(c[0].purl, "pkg:generic/hello@1.0");
        assert!(read_components(&d.path().join("missing")).is_none());
        // Plan products from pkgs/ write the list without the wrapper.
        fs::write(
            tree.join(COMPONENTS_NAME),
            "[\n  {\n    \"name\": \"zlib\",\n    \"purl\": \"pkg:generic/zlib@1.3.2\",\n    \"version\": \"1.3.2\"\n  }\n]\n",
        )
        .unwrap();
        let c = read_components(&tree).expect("bare list");
        assert_eq!(c[0].purl, "pkg:generic/zlib@1.3.2");
    }

    #[test]
    fn fetch_a_vendor_file_into_its_path() {
        let d = tempfile::tempdir().unwrap();
        let vendor = d.path().join("claude");
        fs::write(&vendor, b"\x7fELF vendor bytes\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&vendor, fs::Permissions::from_mode(0o755)).unwrap();
        let want = d.path().join("want");
        fs::create_dir_all(want.join("src")).unwrap();
        fs::write(want.join("src/claude"), b"\x7fELF vendor bytes\n").unwrap();
        let pin = hash_tree(&want).unwrap();
        let cat = d.path().join("cat");
        fs::create_dir_all(cat.join("store/pkg")).unwrap();
        let url = format!("file://{}", vendor.display());
        // Not an origin: the URL is fetched as it is, and the file is data.
        fetch_pkg(&cat, "claude-code-src", &url, "src/claude", &pin).unwrap();
        assert!(store_present(&cat, "claude-code-src", &pin));
        let got = cat.join("store/pkg/claude-code-src").join(&pin).join("src/claude");
        assert_eq!(fs::metadata(&got).unwrap().permissions().mode() & 0o111, 0);
        let other = "sha256-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        assert!(fetch_pkg(&cat, "claude-code-src", &url, "src/claude", other).is_err());
        assert!(fetch_pkg(&cat, "x-src", &url, "../claude", &pin).is_err());
    }

    #[test]
    fn store_a_tree_built_elsewhere() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let cat = d.path().join("cat");
        crate::seed(&cat).unwrap();
        let tree = d.path().join("tree");
        fs::create_dir_all(tree.join("bin")).unwrap();
        fs::write(tree.join("bin/hello"), b"hello\n").unwrap();
        fs::set_permissions(tree.join("bin/hello"), fs::Permissions::from_mode(0o4777)).unwrap();
        fs::write(tree.join("README"), b"read me\n").unwrap();
        fs::set_permissions(tree.join("README"), fs::Permissions::from_mode(0o666)).unwrap();
        let pin = hash_tree(&tree).unwrap();
        assert_eq!(store_pkg(&cat, "hello", &tree).unwrap(), pin);
        let got = realization_dir(&cat, "hello", &pin);
        // The store keeps only the mode the hash records.
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode(&got.join("bin/hello")), 0o755);
        assert_eq!(mode(&got.join("README")), 0o644);
        assert_eq!(hash_tree(&got).unwrap(), pin);
        // Nothing is linked and no object is made: the caller pins and applies.
        assert!(fs::symlink_metadata(slot_dir(&cat, "hello").join(LIVE_NAME)).is_err());
        assert!(!cat.join("objects/pkg/hello").exists());
        // Again is the same hash; a tar of the tree is too, plain or gzip.
        assert_eq!(store_pkg(&cat, "hello", &tree).unwrap(), pin);
        for (flag, file) in [("-cf", "t.tar"), ("-czf", "t.tar.gz")] {
            let tar = d.path().join(file);
            let st = Command::new("tar")
                .args(["-C", tree.to_str().unwrap(), flag, tar.to_str().unwrap(), "."])
                .status()
                .unwrap();
            assert!(st.success());
            assert_eq!(store_pkg(&cat, "hello-tar", &tar).unwrap(), pin);
        }
        assert!(store_present(&cat, "hello-tar", &pin));
        let names: Vec<String> = fs::read_dir(cat.join("store/pkg"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| !n.starts_with('.')), "{names:?}");
        // Refused as a fetch is: a special file, or something that is no tree.
        let fifo = tree.join("pipe");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o644) }, 0);
        let e = store_pkg(&cat, "hello", &tree).unwrap_err().to_string();
        assert!(e.contains("special file"), "{e}");
        let e = store_pkg(&cat, "hello", &tree.join("README")).unwrap_err().to_string();
        assert!(e.contains("not a directory or a tar"), "{e}");
    }

    #[test]
    fn program_realization_follows_bin_links() {
        let d = tempfile::tempdir().unwrap();
        let cat = d.path().join("cat");
        let tree = d.path().join("tree");
        fs::create_dir_all(tree.join("bin")).unwrap();
        fs::write(tree.join("bin/hello"), b"hello\n").unwrap();
        let pin = install_tree(&cat, "hello", &tree, None, "").unwrap();
        let bin = d.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        symlink(realization_dir(&cat, "hello", &pin).join("bin/hello"), bin.join("hello")).unwrap();
        symlink(slot_dir(&cat, "hello").join("live/bin/hello"), bin.join("via-live")).unwrap();
        let want = Some(("hello".to_string(), pin.clone()));
        assert_eq!(program_realization(&cat, &bin.join("hello")), want);
        assert_eq!(program_realization(&cat, &bin.join("via-live")), want);
        assert_eq!(program_realization(&cat, &tree.join("bin/hello")), None);
        assert_eq!(program_realization(&cat, &bin.join("missing")), None);
    }

    #[test]
    fn a_name_the_new_tree_drops_is_unlinked() {
        let d = tempfile::tempdir().unwrap();
        let cat = d.path().join("cat");
        let (bin, lib) = (d.path().join("bin"), d.path().join("lib"));
        let tree = |files: &[&str]| {
            let t = tempfile::tempdir_in(d.path()).unwrap();
            for f in files {
                fs::create_dir_all(t.path().join(f).parent().unwrap()).unwrap();
                fs::write(t.path().join(f), f.as_bytes()).unwrap();
            }
            t
        };
        let old = tree(&["bin/app", "bin/gone", "lib/libgone.so"]);
        let old = install_tree(&cat, "app", old.path(), None, "").unwrap();
        let new = tree(&["bin/app", "bin/new"]);
        let new = install_tree(&cat, "app", new.path(), None, "").unwrap();
        let other = tree(&["bin/other"]);
        let other = install_tree(&cat, "app-other", other.path(), None, "").unwrap();
        converge_with_link_root(&cat, &bin, &cat, "app", true, &old).unwrap();
        converge_with_link_root(&cat, &bin, &cat, "app-other", true, &other).unwrap();
        symlink("../lib/oath/init", bin.join("reboot")).unwrap();
        assert!(bin.join("gone").symlink_metadata().is_ok());
        assert!(lib.join("libgone.so").symlink_metadata().is_ok());
        converge_with_link_root(&cat, &bin, &cat, "app", true, &new).unwrap();
        let names = |dir: &Path| {
            let mut v: Vec<String> = fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            v.sort();
            v
        };
        assert_eq!(names(&bin), ["app", "new", "other", "reboot"]);
        assert!(fs::read_link(bin.join("app"))
            .unwrap()
            .starts_with(realization_dir(&cat, "app", &new)));
        assert!(names(&lib).is_empty());
        // Removed: no link into its slot is left, from any realization.
        symlink(realization_dir(&cat, "app", &old).join("bin/gone"), bin.join("gone")).unwrap();
        converge_with_link_root(&cat, &bin, &cat, "app", false, &new).unwrap();
        assert_eq!(names(&bin), ["other", "reboot"]);
    }

    #[test]
    fn a_name_one_package_drops_another_may_take() {
        let d = tempfile::tempdir().unwrap();
        let cat = d.path().join("cat");
        let (bin, lib) = (d.path().join("bin"), d.path().join("lib"));
        let tree = |files: &[&str]| {
            let t = tempfile::tempdir_in(d.path()).unwrap();
            for f in files {
                fs::create_dir_all(t.path().join(f).parent().unwrap()).unwrap();
                fs::write(t.path().join(f), f.as_bytes()).unwrap();
            }
            t
        };
        let install = |name: &str, files: &[&str]| {
            install_tree(&cat, name, tree(files).path(), None, "").unwrap()
        };
        let pin = |name: &str, hash: &str| {
            let dir = cat.join("objects/pkg").join(name);
            fs::create_dir_all(&dir).unwrap();
            let doc = serde_json::json!({ "present": true, "hash": hash });
            fs::write(dir.join("desired.json"), doc.to_string()).unwrap();
        };
        let old = install("mesa", &["bin/mesa", "lib/libEGL.so.1", "lib/libgbm.so.1"]);
        let new = install("mesa", &["bin/mesa", "lib/libEGL_mesa.so.0", "lib/libgbm.so.1"]);
        let glvnd = install("libglvnd", &["bin/glvnd", "lib/libEGL.so.1"]);
        converge_with_link_root(&cat, &bin, &cat, "mesa", true, &old).unwrap();
        // While Mesa's desired tree still has it, the name stays Mesa's.
        pin("mesa", &old);
        let e = converge_with_link_root(&cat, &bin, &cat, "libglvnd", true, &glvnd).unwrap_err();
        assert!(e.to_string().contains("/lib/libEGL.so.1 exists and is not pkg:libglvnd"), "{e}");
        // An update moves Mesa off it: libglvnd takes it before Mesa converges.
        pin("mesa", &new);
        converge_with_link_root(&cat, &bin, &cat, "libglvnd", true, &glvnd).unwrap();
        converge_with_link_root(&cat, &bin, &cat, "mesa", true, &new).unwrap();
        let egl = fs::read_link(lib.join("libEGL.so.1")).unwrap();
        assert!(egl.starts_with(realization_dir(&cat, "libglvnd", &glvnd)), "{egl:?}");
        assert!(lib.join("libEGL_mesa.so.0").symlink_metadata().is_ok());
        assert!(lib.join("libgbm.so.1").symlink_metadata().is_ok());
    }

    #[test]
    fn fetch_tries_each_origin_in_order() {
        let d = tempfile::tempdir().unwrap();
        let tree = d.path().join("tree");
        fs::create_dir_all(tree.join("bin")).unwrap();
        fs::write(tree.join("bin/hello"), b"hello\n").unwrap();
        let pin = hash_tree(&tree).unwrap();
        let good = d.path().join("good");
        fs::create_dir_all(good.join("pkg/hello")).unwrap();
        let tar = good.join("pkg/hello").join(format!("{pin}.tar"));
        let st = Command::new("tar")
            .args(["-C", tree.to_str().unwrap(), "-cf", tar.to_str().unwrap(), "."])
            .status()
            .unwrap();
        assert!(st.success());
        let cat = d.path().join("cat");
        crate::seed(&cat).unwrap();
        // The consumer lists where it looks: one origin without the pack, then one with it.
        let host = cat.join("objects/host/local/desired.json");
        let mut h: serde_json::Value = crate::read_json(&host).unwrap();
        let (gone, good) =
            (format!("file://{}/gone", d.path().display()), format!("file://{}", good.display()));
        h["origins"] = serde_json::json!([gone, good]);
        write_json(&host, &h).unwrap();
        assert_eq!(host_origins(&cat), vec![gone.clone(), good.clone()]);
        assert_eq!(fetch_pkg_any(&cat, "hello", "", "", &pin).unwrap(), Some(good.clone()));
        assert!(store_present(&cat, "hello", &pin));
        // The store wins; nothing is fetched again.
        assert_eq!(fetch_pkg_any(&cat, "hello", "", "", &pin).unwrap(), None);
        let other = "sha256-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
        let e = fetch_pkg_any(&cat, "hello", "", "", other).unwrap_err().to_string();
        assert!(e.contains("no origin served it") && e.contains(&gone), "{e}");
        // An origin needs a pin, and a vendor's file is never looked for on one.
        assert_eq!(fetch_pkg_any(&cat, "hello", "", "", "").unwrap(), None);
        assert_eq!(fetch_pkg_any(&cat, "x-src", "", "src/x", other).unwrap(), None);
    }

    #[test]
    fn prefetch_stores_but_neither_links_nor_pins() {
        let d = tempfile::tempdir().unwrap();
        let tree = d.path().join("tree");
        fs::create_dir_all(tree.join("bin")).unwrap();
        fs::write(tree.join("bin/hello"), b"hello\n").unwrap();
        let pin = hash_tree(&tree).unwrap();
        let origin = d.path().join("origin");
        fs::create_dir_all(origin.join("pkg/hello")).unwrap();
        let tar = origin.join("pkg/hello").join(format!("{pin}.tar"));
        let st = Command::new("tar")
            .args(["-C", tree.to_str().unwrap(), "-cf", tar.to_str().unwrap(), "."])
            .status()
            .unwrap();
        assert!(st.success());
        let cat = d.path().join("cat");
        crate::seed(&cat).unwrap();
        let host = cat.join("objects/host/local/desired.json");
        let mut h: serde_json::Value = crate::read_json(&host).unwrap();
        h["origins"] = serde_json::json!([format!("file://{}", origin.display())]);
        write_json(&host, &h).unwrap();
        // An object applied at another pin (canto's pkg:oath, 2026-10-28):
        // the fetch must leave its spec and its links for apply to move.
        let obj = cat.join("objects/pkg/hello");
        fs::create_dir_all(&obj).unwrap();
        let spec = serde_json::json!({ "present": true, "hash": H });
        for f in ["desired.json", "applied.json"] {
            write_json(&obj.join(f), &spec).unwrap();
        }
        let from = format!("file://{}", origin.display());
        assert_eq!(prefetch_pkg_any(&cat, "hello", "", "", &pin).unwrap(), Some(from));
        assert!(store_present(&cat, "hello", &pin));
        assert!(slot_dir(&cat, "hello").join(LIVE_NAME).symlink_metadata().is_err());
        for f in ["desired.json", "applied.json"] {
            let v: serde_json::Value = crate::read_json(&obj.join(f)).unwrap();
            assert_eq!(v["hash"], H, "{f}");
        }
        // Apply's own fetch finds it in the store and fetches nothing.
        assert_eq!(fetch_pkg_any(&cat, "hello", "", "", &pin).unwrap(), None);
    }

    #[test]
    fn fetch_tries_again_after_a_reset() {
        use std::io::Write;
        let d = tempfile::tempdir().unwrap();
        let tree = d.path().join("tree");
        fs::create_dir_all(tree.join("bin")).unwrap();
        fs::write(tree.join("bin/hello"), b"hello\n").unwrap();
        let pin = hash_tree(&tree).unwrap();
        let tar = d.path().join("hello.tar");
        let st = Command::new("tar")
            .args(["-C", tree.to_str().unwrap(), "-cf", tar.to_str().unwrap(), "."])
            .status()
            .unwrap();
        assert!(st.success());
        let body = fs::read(&tar).unwrap();
        // The origin drops the first connection unanswered, then serves.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", l.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for n in 0..2 {
                let (mut s, _) = l.accept().unwrap();
                let mut req = [0u8; 4096];
                let _ = s.read(&mut req);
                if n == 1 {
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    s.write_all(head.as_bytes()).unwrap();
                    s.write_all(&body).unwrap();
                }
            }
        });
        let cat = d.path().join("cat");
        crate::seed(&cat).unwrap();
        fetch_pkg(&cat, "hello", &origin, "", &pin).unwrap();
        server.join().unwrap();
        assert!(store_present(&cat, "hello", &pin));
    }

    #[test]
    fn advisories_keyed_by_hash() {
        let d = tempfile::tempdir().unwrap();
        let h = H;
        let p = adv_path(d.path(), "curl", h);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(
            &p,
            format!(
                "{{\n  \"hash\": \"{h}\",\n  \"pkg\": \"pkg:curl\",\n  \"advisories\": [\n    {{\n      \"id\": \"CVE-TEST-1\",\n      \"url\": \"https://example.invalid/CVE-TEST-1\",\n      \"summary\": \"fixture\"\n    }}\n  ]\n}}\n"
            ),
        )
        .unwrap();
        let a = read_advisories(d.path(), "curl", h).expect("advisories");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].id, "CVE-TEST-1");
        assert!(read_advisories(d.path(), "curl", H2).is_none());
        assert!(read_advisories(d.path(), "hello", h).is_none());
    }

    #[test]
    fn write_advisories_canonical_and_sorted() {
        let d = tempfile::tempdir().unwrap();
        write_advisories(
            d.path(),
            "curl",
            H,
            vec![
                PkgAdvisory {
                    id: "CVE-TEST-2".into(),
                    url: "https://example.invalid/2".into(),
                    summary: String::new(),
                },
                PkgAdvisory {
                    id: "CVE-TEST-1".into(),
                    url: "https://example.invalid/1".into(),
                    summary: "first".into(),
                },
            ],
        )
        .unwrap();
        let body = fs::read_to_string(adv_path(d.path(), "curl", H)).unwrap();
        assert!(body.starts_with("{\n  \"hash\":"), "{body}");
        let a = read_advisories(d.path(), "curl", H).unwrap();
        assert_eq!(a[0].id, "CVE-TEST-1");
        assert_eq!(a[1].id, "CVE-TEST-2");
        assert_eq!(a[0].summary, "first");
    }
}
