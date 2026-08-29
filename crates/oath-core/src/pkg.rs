//! Link farm: `/oath/store/pkg/<name>/bin/*` ↔ `/bin/<basename>`.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::kinds::PkgActual;

pub fn store_bin(catalog_root: &Path, name: &str) -> PathBuf {
    catalog_root.join("store").join("pkg").join(name).join("bin")
}

/// Create or remove this package’s `/bin` symlinks. Never clobber a
/// name that does not already point at this package’s store.
pub fn converge(
    catalog_root: &Path,
    bin_dir: &Path,
    name: &str,
    present: bool,
) -> Result<PkgActual> {
    let store = store_bin(catalog_root, name);
    if present && !store.is_dir() {
        return Err(Error::hint(
            format!("no store for pkg:{name}"),
            format!("oath get pkg:{name}"),
        ));
    }
    fs::create_dir_all(bin_dir)?;
    let names = bin_names(&store)?;
    if present {
        let mut links = Vec::new();
        for n in &names {
            let target = store_target(catalog_root, name, n);
            let dest = bin_dir.join(n);
            if dest.symlink_metadata().is_ok() {
                if is_our_link(&dest, &target, &store.join(n)) {
                    links.push(n.clone());
                    continue;
                }
                return Err(Error::hint(
                    format!("/bin/{n} exists and is not pkg:{name}"),
                    "oath schema pkg",
                ));
            }
            symlink(&target, &dest).map_err(|e| {
                Error::Msg(format!("symlink {} -> {}: {e}", dest.display(), target.display()))
            })?;
            links.push(n.clone());
        }
        Ok(PkgActual { present: true, links })
    } else {
        for n in &names {
            let dest = bin_dir.join(n);
            let target = store_target(catalog_root, name, n);
            if dest.symlink_metadata().is_ok() && is_our_link(&dest, &target, &store.join(n)) {
                fs::remove_file(&dest)?;
            }
        }
        Ok(PkgActual { present: false, links: Vec::new() })
    }
}

fn store_target(catalog_root: &Path, name: &str, file: &str) -> PathBuf {
    catalog_root.join("store").join("pkg").join(name).join("bin").join(file)
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

fn is_our_link(dest: &Path, target: &Path, store_file: &Path) -> bool {
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
    let resolved =
        if cur.is_absolute() { cur } else { dest.parent().unwrap_or(Path::new(".")).join(cur) };
    match (fs::canonicalize(&resolved), fs::canonicalize(store_file)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}
