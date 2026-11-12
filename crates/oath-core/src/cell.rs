//! `cell:<name>` (R78): one project's own software, made to run in a cell
//! and nowhere else. Its trees are under `store/cell/<name>/<hash>/`, apart
//! from every package's. A cell is not linked into `/bin` or `/lib`, comes
//! from no origin and is never published: those are a `pkg`'s paths, and a
//! cell is not on them. What it runs with are packages, named and not
//! pinned: `cell run` takes each at the hash this machine links, so moving
//! a package leaves no cell behind.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::kinds::{Cell, CellActual, PkgVerified};
use crate::packhash::{hash_tree, LIVE_NAME};
use crate::pkg::{replace_symlink, store_in};

/// Where a cell's trees are: `<root>/store/cell/<name>/`.
pub fn slot_dir(root: &Path, name: &str) -> PathBuf {
    root.join("store").join("cell").join(name)
}

pub fn tree_dir(root: &Path, name: &str, hash: &str) -> PathBuf {
    slot_dir(root, name).join(hash)
}

/// `oath store cell:<name>`: put a tree made elsewhere into the store and
/// return its hash. No spec changes: the caller pins it and applies. A
/// cell applied at that hash records the check in its actual, so a damaged
/// or missing tree put back is no longer damage.
pub fn store(root: &Path, name: &str, src: &Path) -> Result<String> {
    store_in(root, "cell", name, src)
}

/// Refuse a present `cell` whose tree is not in the store or does not hash
/// to its pin. A cell comes from no origin, so `oath store` is the repair.
pub fn check(root: &Path, name: &str, cell: &Cell) -> Result<()> {
    if !cell.present {
        return Ok(());
    }
    let tree = tree_dir(root, name, &cell.hash);
    if !tree.is_dir() {
        return Err(Error::hint(
            format!("cell:{name} {} is not in the store", cell.hash),
            format!("a cell comes from no origin: sudo oath store cell:{name} <dir|file.tar>"),
        ));
    }
    let got = hash_tree(&tree)?;
    if got != cell.hash {
        return Err(Error::security(
            format!("cell:{name} hashes to {got}, not its pin {}", cell.hash),
            format!("sudo oath store cell:{name} <dir|file.tar> puts it back"),
        ));
    }
    Ok(())
}

/// Make `cell` what this machine starts: its tree is hashed against the
/// pin ([`check`]), unless `checked` says that was just done, and `live`
/// names it. Nothing else on the machine changes.
pub fn converge(root: &Path, name: &str, cell: &Cell, checked: bool) -> Result<CellActual> {
    let live = slot_dir(root, name).join(LIVE_NAME);
    if !cell.present {
        match fs::remove_file(&live) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        return Ok(actual(cell, false, None));
    }
    let began = crate::since_boot_ns();
    if !checked {
        check(root, name, cell)?;
    }
    replace_symlink(&live, &cell.hash)?;
    let verified = PkgVerified { ok: true, at: crate::now_rfc3339(), got: String::new(), began };
    Ok(actual(cell, true, Some(verified)))
}

/// What the store says of a cell whose status is gone (PID 1, at boot).
pub fn observe(root: &Path, name: &str, cell: &Cell) -> CellActual {
    let linked = fs::read_link(slot_dir(root, name).join(LIVE_NAME))
        .is_ok_and(|t| t == Path::new(&cell.hash));
    let present = cell.present && linked && tree_dir(root, name, &cell.hash).is_dir();
    actual(cell, present, None)
}

/// Hash realization `hash` of a cell again.
pub fn verify_tree(root: &Path, name: &str, hash: &str) -> Result<PkgVerified> {
    let began = crate::since_boot_ns();
    let got = hash_tree(&tree_dir(root, name, hash))?;
    let ok = got == hash;
    let got = if ok { String::new() } else { got };
    Ok(PkgVerified { ok, at: crate::now_rfc3339(), got, began })
}

fn actual(cell: &Cell, present: bool, verified: Option<PkgVerified>) -> CellActual {
    CellActual {
        present,
        hash: if present { cell.hash.clone() } else { String::new() },
        needs: cell.needs.clone(),
        description: cell.description.clone(),
        home: cell.home.clone(),
        source: cell.source.clone(),
        verified,
    }
}
