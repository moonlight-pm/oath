//! Link farm: `/oath/store/pkg/<name>/<hash>/bin/*` ↔ `/bin/<basename>`.
//!
//! Slot wrappers (not part of the hash): `live` → `<hash>`, plus a
//! symlink per top-level pack entry (`bin` → `live/bin`, …) so RPATH
//! and env that still name `/oath/store/pkg/<name>/lib` keep working.

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

/// Create or remove this package’s `/bin` symlinks. Never clobber a
/// name that does not already point at this package’s store.
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
    let names = bin_names(&store)?;
    if present {
        let mut links = Vec::new();
        for n in &names {
            let target = store_target(link_root, name, n, &tree, store_root);
            let dest = bin_dir.join(n);
            if dest.symlink_metadata().is_ok() {
                if is_our_link(&dest, &target, &store.join(n), &slot_dir(store_root, name)) {
                    let cur = fs::read_link(&dest).ok();
                    if cur.as_deref() == Some(target.as_path()) {
                        links.push(n.clone());
                        continue;
                    }
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
        let mut actual = pkg_actual(true, links, live_hash, store_root, name);
        actual.verified = verified;
        Ok(actual)
    } else {
        for n in &names {
            unlink_ours(bin_dir, link_root, store_root, name, n, &tree)?;
        }
        Ok(pkg_actual(false, Vec::new(), live_hash, store_root, name))
    }
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

fn unlink_ours(
    bin_dir: &Path,
    link_root: &Path,
    store_root: &Path,
    name: &str,
    n: &str,
    tree: &Path,
) -> Result<()> {
    let dest = bin_dir.join(n);
    let store = tree.join("bin");
    let target = store_target(link_root, name, n, tree, store_root);
    if dest.symlink_metadata().is_ok()
        && is_our_link(&dest, &target, &store.join(n), &slot_dir(store_root, name))
    {
        fs::remove_file(&dest)?;
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
                .join("bin")
                .join(file);
        }
    }
    link_root.join("store").join("pkg").join(name).join("bin").join(file)
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
    Ok(PkgVerified {
        ok,
        at: crate::now_rfc3339(),
        got: if ok { String::new() } else { got },
    })
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

/// Copy `src` (a pack directory) into the hashed store. Also writes
/// `live` + slot compat links. Optional host cache gets the same tree.
pub fn install_tree(
    catalog_root: &Path,
    name: &str,
    src: &Path,
    cache: Option<&Path>,
    pin: &str,
) -> Result<String> {
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
    if let Some(cache) = cache {
        put_tree(&cache.join("pkg").join(name), &got, &dest)?;
    }
    write_slot_links(catalog_root, name, &got)?;
    if !pin.is_empty() {
        pin_desired_hash(catalog_root, name, &got)?;
    }
    Ok(got)
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
        let actual =
            crate::read_json::<Value>(&e.path().join("actual.json")).unwrap_or(Value::Null);
        let h = desired
            .get("hash")
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .or_else(|| actual.get("hash").and_then(|x| x.as_str()).filter(|s| !s.is_empty()))
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
        for file in ["desired.json", "actual.json"] {
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

pub fn pin_desired_hash(catalog_root: &Path, name: &str, hash: &str) -> Result<()> {
    let p = catalog_root.join("objects").join("pkg").join(name).join("desired.json");
    if !p.is_file() {
        return Ok(());
    }
    let mut v: serde_json::Value = crate::read_json(&p)?;
    v["hash"] = serde_json::Value::String(hash.to_string());
    write_json(&p, &v)?;
    Ok(())
}

pub fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
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
        } else {
            if let Some(p) = to.parent() {
                fs::create_dir_all(p)?;
            }
            fs::copy(e.path(), &to)?;
        }
    }
    Ok(())
}

/// True when the slot already has the pin (or any tree, if unpinned).
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

/// Store-root helper: guest `/oath/store`, host `.cache/oath/store`.
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

/// T44 fetch: wget `{origin}/adv/{name}/{hash}.json`. Failure is Ok
/// (apply still converges; previous file stays).
pub fn fetch_adv(catalog_root: &Path, name: &str, url: &str, hash: &str) -> Result<()> {
    let adv_url = fetch_adv_url(url, name, hash);
    if adv_url.is_empty() {
        return Ok(());
    }
    let tmpdir = catalog_root.join("store/adv").join(format!(".{name}.wget"));
    let _ = fs::remove_dir_all(&tmpdir);
    if let Err(e) = fs::create_dir_all(&tmpdir) {
        let _ = fs::remove_dir_all(&tmpdir);
        return Err(Error::Msg(format!("adv tmp: {e}")));
    }
    let blob = tmpdir.join("blob");
    let wget = find_bin("wget");
    let st = Command::new(&wget)
        .args(["-q", "-O", blob.to_str().unwrap(), &adv_url])
        .status()
        .map_err(|e| Error::Msg(format!("wget: {e}")));
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

/// T20 fetch: wget `url` (or `{url}/pkg/{name}/{hash}.tar`) into the store.
/// A `.tar` / gzip is a pack tree; a single file becomes `bin/<name>`.
/// Plan pre-build uses this with `present` still false.
pub fn fetch_pkg(catalog_root: &Path, name: &str, url: &str, hash: &str) -> Result<()> {
    if store_present(catalog_root, name, hash) {
        return Ok(());
    }
    let url = fetch_url(url, name, hash);
    if url.is_empty() {
        return Ok(());
    }
    let tmpdir = catalog_root.join("store/pkg").join(format!(".{name}.wget"));
    let _ = fs::remove_dir_all(&tmpdir);
    fs::create_dir_all(&tmpdir)?;
    let blob = tmpdir.join("blob");
    let wget = find_bin("wget");
    let st = Command::new(&wget)
        .args(["-q", "-O", blob.to_str().unwrap(), &url])
        .status()
        .map_err(|e| Error::Msg(format!("wget: {e}")))?;
    if !st.success() {
        let _ = fs::remove_dir_all(&tmpdir);
        return Err(Error::hint(format!("fetch pkg:{name} failed"), "oath schema pkg"));
    }
    let result = if looks_like_tar(&blob) {
        let tree = tmpdir.join("tree");
        fs::create_dir_all(&tree)?;
        extract_tar(&blob, &tree)?;
        install_tree(catalog_root, name, &tree, None, hash)
    } else {
        let bytes = fs::read(&blob)?;
        ingest_file(catalog_root, name, &bytes, None, hash)
    };
    let _ = fs::remove_dir_all(&tmpdir);
    result.map(|_| ())
}

fn find_bin(name: &str) -> PathBuf {
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
    let st = cmd.status().map_err(|e| Error::Msg(format!("tar: {e}")))?;
    if !st.success() {
        return Err(Error::hint("fetch pack tar extract failed", "oath schema pkg"));
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
            hash: hash.to_string(),
            needs: needs
                .iter()
                .map(|(id, h)| PkgNeed { id: (*id).into(), hash: (*h).into() })
                .collect(),
            description: String::new(),
            home: String::new(),
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
