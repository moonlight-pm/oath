//! Link farm: `/oath/store/pkg/<name>/<hash>/bin/*` ↔ `/bin/<basename>`.
//!
//! Slot wrappers (not part of the hash): `live` → `<hash>`, plus a
//! symlink per top-level pack entry (`bin` → `live/bin`, …) so RPATH
//! and env that still name `/oath/store/pkg/<name>/lib` keep working.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::gpu::drm_modifiers_available;
use crate::kinds::{Pkg, PkgActual, PkgRealization, PkgRequires};
use crate::packhash::{hash_tree, is_realization_id, realization_dir, slot_dir, LIVE_NAME};
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
    if present
        && is_realization_id(&live_hash)
        && realization_dir(store_root, name, &live_hash).is_dir()
    {
        write_slot_links(store_root, name, &live_hash)?;
    }
    fs::create_dir_all(bin_dir)?;
    let names = bin_names(&store)?;
    if present {
        let mut links = Vec::new();
        for n in &names {
            if skip_bin(name, n) {
                unlink_ours(bin_dir, link_root, store_root, name, n, &tree)?;
                continue;
            }
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
        Ok(pkg_actual(true, links, live_hash, store_root, name))
    } else {
        for n in &names {
            unlink_ours(bin_dir, link_root, store_root, name, n, &tree)?;
        }
        Ok(pkg_actual(false, Vec::new(), live_hash, store_root, name))
    }
}

/// Refuse `present=true` when the package needs GPU features this
/// machine does not have. Uninstall (`present=false`) is always ok.
pub fn check_requires(name: &str, desired: &Pkg) -> Result<()> {
    if !desired.present {
        return Ok(());
    }
    let needs = desired.requires.drm_modifiers || name == "gamescope";
    if !needs {
        return Ok(());
    }
    if drm_modifiers_available() {
        return Ok(());
    }
    Err(Error::hint(
        format!(
            "pkg:{name} needs a GPU with DRM format modifiers (Vulkan WSI); this card does not"
        ),
        "oath schema pkg",
    ))
}

fn skip_bin(pkg: &str, bin: &str) -> bool {
    pkg == "sola" && bin == "sola-arcade" && !drm_modifiers_available()
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
    PkgActual {
        present,
        links,
        removable: true,
        url: String::new(),
        hash,
        realizations: list_realizations(store_root, name).unwrap_or_default(),
        requires: PkgRequires::default(),
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
        let linked = live.as_ref().map(|l| l == &hash).unwrap_or(false);
        out.push(PkgRealization { hash, linked, bins, index });
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
    let dest = realization_dir(catalog_root, name, &got);
    if !dest.is_dir() {
        copy_tree(src, &dest)?;
    }
    if let Some(cache) = cache {
        let c = cache.join("pkg").join(name).join(&got);
        if !c.is_dir() {
            copy_tree(&dest, &c)?;
        }
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
                let c = cache.join("pkg").join(name).join(&ids[0]);
                if !c.is_dir() {
                    copy_tree(&realization_dir(catalog_root, name, &ids[0]), &c)?;
                }
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
        let c = cache.join("pkg").join(name).join(&got);
        if !c.is_dir() {
            copy_tree(&dest, &c)?;
        }
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
