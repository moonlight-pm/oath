//! Content hash of a pack tree (T32).
//!
//! Encoding `oath-tree-v1`: SHA-256 of a canonical catalog of every
//! directory, regular file, and symlink under the pack root. File
//! payloads are digested separately (so a 1G tree is not held in
//! memory). uid, gid, mtime, xattrs, and the live/hash slot wrappers
//! are not part of the hash.
//!
//! Realization id is `sha256-` plus 64 lowercase hex. Store path is
//! `/oath/store/pkg/<name>/<hash>/`.

use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

pub const TREE_MAGIC: &[u8] = b"oath-tree-v1\n";
pub const HASH_PREFIX: &str = "sha256-";
pub const LIVE_NAME: &str = "live";

const KIND_DIR: u8 = 1;
const KIND_FILE: u8 = 2;
const KIND_SYMLINK: u8 = 3;

pub fn is_realization_id(s: &str) -> bool {
    let Some(hex) = s.strip_prefix(HASH_PREFIX) else {
        return false;
    };
    hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// SHA-256 realization id of the directory `root`.
pub fn hash_tree(root: &Path) -> Result<String> {
    if !root.is_dir() {
        return Err(Error::hint(
            format!("not a pack directory: {}", root.display()),
            "oath schema pkg",
        ));
    }
    let mut catalog = Vec::from(TREE_MAGIC);
    walk(root, "", &mut catalog)?;
    Ok(format!("{HASH_PREFIX}{}", hex(Sha256::digest(&catalog).as_ref())))
}

fn walk(dir: &Path, prefix: &str, catalog: &mut Vec<u8>) -> Result<()> {
    let mut names: Vec<String> = Vec::new();
    for e in fs::read_dir(dir)? {
        let e = e?;
        let name = e.file_name();
        let name = name.to_str().ok_or_else(|| {
            Error::hint(format!("non-utf8 path in pack: {}", e.path().display()), "oath schema pkg")
        })?;
        if name == "." || name == ".." {
            continue;
        }
        if name.contains('\0') || name.contains('\n') || name.contains('\t') {
            return Err(Error::hint(
                format!("illegal path component in pack: {name}"),
                "oath schema pkg",
            ));
        }
        names.push(name.to_string());
    }
    names.sort();
    for name in names {
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        if rel.len() > u16::MAX as usize {
            return Err(Error::hint(format!("path too long: {rel}"), "oath schema pkg"));
        }
        let path = dir.join(&name);
        let meta = fs::symlink_metadata(&path)?;
        let ft = meta.file_type();
        put_path(catalog, &rel);
        if ft.is_symlink() {
            catalog.push(KIND_SYMLINK);
            let target = fs::read_link(&path)?;
            let t = target.to_str().ok_or_else(|| {
                Error::hint(
                    format!("non-utf8 symlink target: {}", path.display()),
                    "oath schema pkg",
                )
            })?;
            put_path(catalog, t);
        } else if ft.is_dir() {
            catalog.push(KIND_DIR);
            walk(&path, &rel, catalog)?;
        } else if ft.is_file() {
            catalog.push(KIND_FILE);
            let exec = meta.permissions().mode() & 0o111 != 0;
            catalog.push(if exec { 1 } else { 0 });
            catalog.extend_from_slice(&file_digest(&path)?);
        } else if ft.is_fifo() || ft.is_socket() || ft.is_block_device() || ft.is_char_device() {
            return Err(Error::hint(
                format!("pack contains a special file: {rel}"),
                "oath schema pkg",
            ));
        } else {
            return Err(Error::hint(
                format!("pack contains an unknown file type: {rel}"),
                "oath schema pkg",
            ));
        }
    }
    Ok(())
}

fn file_digest(path: &Path) -> Result<[u8; 32]> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    let d = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&d);
    Ok(out)
}

fn put_path(buf: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    let n = bytes.len() as u16;
    buf.extend_from_slice(&n.to_be_bytes());
    buf.extend_from_slice(bytes);
}

fn hex(bytes: &[u8]) -> String {
    const T: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(T[(b >> 4) as usize] as char);
        s.push(T[(b & 0xf) as usize] as char);
    }
    s
}

pub fn slot_dir(catalog_root: &Path, name: &str) -> PathBuf {
    catalog_root.join("store").join("pkg").join(name)
}

pub fn realization_dir(catalog_root: &Path, name: &str, hash: &str) -> PathBuf {
    slot_dir(catalog_root, name).join(hash)
}

/// Origin URL for a hashed pack tarball.
///
/// A full `.tar` / `.tgz` URL is used as-is. An origin prefix plus a
/// pin becomes `{origin}/pkg/{name}/{hash}.tar`. A URL with no pin
/// (fetchme canary) is used as-is.
pub fn fetch_url(url: &str, name: &str, hash: &str) -> String {
    let u = url.trim();
    if u.is_empty() {
        return String::new();
    }
    let lower = u.to_ascii_lowercase();
    if lower.ends_with(".tar") || lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        return u.to_string();
    }
    if hash.is_empty() || !is_realization_id(hash) {
        return u.to_string();
    }
    format!("{}/pkg/{name}/{hash}.tar", u.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    /// `bin/hello` containing `hello\n` mode 0755. Frozen `oath-tree-v1`.
    const GOLDEN_HELLO: &str =
        "sha256-0457a8981dc3688addb3cfc0320aa72c1d6226ed1c11240074d4be0a0ff0545a";

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write_exec(p: &Path, body: &[u8]) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
        let mut perm = fs::metadata(p).unwrap().permissions();
        perm.set_mode(0o755);
        fs::set_permissions(p, perm).unwrap();
    }

    #[test]
    fn realization_id_shape() {
        assert!(is_realization_id(
            "sha256-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(!is_realization_id("sha256-abc"));
        assert!(!is_realization_id(
            "SHA256-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(!is_realization_id("hello"));
    }

    #[test]
    fn same_tree_same_hash() {
        let a = tmp();
        let b = tmp();
        write_exec(&a.path().join("bin/hello"), b"hello\n");
        write_exec(&b.path().join("bin/hello"), b"hello\n");
        fs::create_dir_all(a.path().join("share/empty")).unwrap();
        fs::create_dir_all(b.path().join("share/empty")).unwrap();
        assert_eq!(hash_tree(a.path()).unwrap(), hash_tree(b.path()).unwrap());
    }

    #[test]
    fn content_changes_hash() {
        let a = tmp();
        let b = tmp();
        write_exec(&a.path().join("bin/hello"), b"hello\n");
        write_exec(&b.path().join("bin/hello"), b"HELLO\n");
        assert_ne!(hash_tree(a.path()).unwrap(), hash_tree(b.path()).unwrap());
    }

    #[test]
    fn exec_bit_changes_hash() {
        let a = tmp();
        fs::create_dir_all(a.path().join("bin")).unwrap();
        fs::write(a.path().join("bin/hello"), b"hello\n").unwrap();
        let mut perm = fs::metadata(a.path().join("bin/hello")).unwrap().permissions();
        perm.set_mode(0o644);
        fs::set_permissions(a.path().join("bin/hello"), perm).unwrap();
        let h1 = hash_tree(a.path()).unwrap();
        let mut perm = fs::metadata(a.path().join("bin/hello")).unwrap().permissions();
        perm.set_mode(0o755);
        fs::set_permissions(a.path().join("bin/hello"), perm).unwrap();
        let h2 = hash_tree(a.path()).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn symlink_target_changes_hash() {
        let a = tmp();
        fs::create_dir_all(a.path().join("bin")).unwrap();
        symlink("one", a.path().join("bin/cc")).unwrap();
        let h1 = hash_tree(a.path()).unwrap();
        fs::remove_file(a.path().join("bin/cc")).unwrap();
        symlink("two", a.path().join("bin/cc")).unwrap();
        let h2 = hash_tree(a.path()).unwrap();
        assert_ne!(h1, h2);
    }

    #[test]
    fn golden_hello_tree() {
        let a = tmp();
        write_exec(&a.path().join("bin/hello"), b"hello\n");
        let h = hash_tree(a.path()).unwrap();
        assert!(is_realization_id(&h), "{h}");
        // Frozen encoding. Bump only with a documented format change.
        assert_eq!(h, GOLDEN_HELLO);
    }

    #[test]
    fn fetch_url_origin_and_file() {
        let h = "sha256-0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            fetch_url("https://store.oath.wicket.cloud/", "hello", h),
            format!("https://store.oath.wicket.cloud/pkg/hello/{h}.tar")
        );
        assert_eq!(
            fetch_url("https://bucket/pkg/hello/foo.tar", "hello", h),
            "https://bucket/pkg/hello/foo.tar"
        );
        assert_eq!(
            fetch_url("http://10.0.2.2:18765/fetchme", "fetchme", ""),
            "http://10.0.2.2:18765/fetchme"
        );
    }
}
