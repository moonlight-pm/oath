//! POSIX newc cpio writer (no `cpio` binary).

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

struct Entry {
    name: String,
    mode: u32,
    data: Vec<u8>,
}

fn pad4(n: usize) -> usize {
    (4 - (n % 4)) % 4
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<Entry>) -> io::Result<()> {
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    let name = if rel.as_os_str().is_empty() {
        ".".to_string()
    } else {
        format!("./{}", rel.to_string_lossy())
    };
    let meta = fs::symlink_metadata(dir)?;
    if meta.is_dir() {
        out.push(Entry { name, mode: 0o040755, data: Vec::new() });
        let mut kids: Vec<PathBuf> =
            fs::read_dir(dir)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
        kids.sort();
        for k in kids {
            collect(root, &k, out)?;
        }
    } else if meta.file_type().is_symlink() {
        let target = fs::read_link(dir)?;
        out.push(Entry {
            name,
            mode: 0o120777,
            data: target.to_string_lossy().as_bytes().to_vec(),
        });
    } else {
        let mut mode = 0o100644;
        if meta.permissions().mode() & 0o111 != 0 {
            mode = 0o100755;
        }
        out.push(Entry { name, mode, data: fs::read(dir)? });
    }
    Ok(())
}

fn write_entry<W: Write>(w: &mut W, e: &Entry) -> io::Result<()> {
    let mut name = e.name.clone().into_bytes();
    if !name.ends_with(&[0]) {
        name.push(0);
    }
    let hdr = format!(
        "070701{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}",
        0u32,
        e.mode,
        0u32,
        0u32,
        1u32,
        0u32,
        e.data.len() as u32,
        0u32,
        0u32,
        0u32,
        0u32,
        name.len() as u32,
        0u32,
    );
    debug_assert_eq!(hdr.len(), 110);
    w.write_all(hdr.as_bytes())?;
    w.write_all(&name)?;
    w.write_all(&vec![0u8; pad4(110 + name.len())])?;
    w.write_all(&e.data)?;
    w.write_all(&vec![0u8; pad4(e.data.len())])?;
    Ok(())
}

pub fn write_tree<W: Write>(w: &mut W, root: &Path) -> io::Result<()> {
    let mut entries = Vec::new();
    collect(root, root, &mut entries)?;
    for e in &entries {
        write_entry(w, e)?;
    }
    write_entry(w, &Entry { name: "TRAILER!!!".into(), mode: 0, data: Vec::new() })?;
    Ok(())
}
