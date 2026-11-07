//! The libraries an ELF file loads (`DT_NEEDED`), read from its program
//! headers, which the loader itself reads (section headers may be
//! stripped). 64-bit little-endian only: x86_64, Oath's one target. Used by
//! `oath update` to check that a package still finds every library it
//! loads before re-pointing one of its needs (R72).

use std::collections::BTreeSet;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::path::Path;

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const DT_NULL: u64 = 0;
const DT_NEEDED: u64 = 1;
const DT_STRTAB: u64 = 5;
const DT_STRSZ: u64 = 10;

/// The `DT_NEEDED` names of the file at `path`, or None when it is not a
/// 64-bit little-endian ELF file (a script, data, a 32-bit library). A
/// static program loads nothing: `Some(vec![])`.
pub fn needed(path: &Path) -> Option<Vec<String>> {
    let f = File::open(path).ok()?;
    let mut hdr = [0u8; 64];
    f.read_exact_at(&mut hdr, 0).ok()?;
    if &hdr[..4] != b"\x7fELF" || hdr[4] != 2 || hdr[5] != 1 {
        return None;
    }
    let phoff = u64le(&hdr[0x20..]);
    let phentsize = u16le(&hdr[0x36..]) as u64;
    let phnum = u16le(&hdr[0x38..]) as u64;
    if phentsize < 56 || phnum > 4096 {
        return None;
    }
    let mut ph = vec![0u8; (phentsize * phnum) as usize];
    f.read_exact_at(&mut ph, phoff).ok()?;
    let segs: Vec<(u32, u64, u64, u64)> = ph
        .chunks(phentsize as usize)
        .map(|p| (u32le(p), u64le(&p[8..]), u64le(&p[0x10..]), u64le(&p[0x20..])))
        .collect();
    let Some(&(_, dyn_off, _, dyn_size)) = segs.iter().find(|s| s.0 == PT_DYNAMIC) else {
        return Some(Vec::new());
    };
    if dyn_size > 1 << 20 {
        return None;
    }
    let mut dynamic = vec![0u8; dyn_size as usize];
    f.read_exact_at(&mut dynamic, dyn_off).ok()?;
    let (mut names, mut strtab, mut strsz) = (Vec::new(), None, 0u64);
    for e in dynamic.chunks_exact(16) {
        let (tag, val) = (u64le(e), u64le(&e[8..]));
        match tag {
            DT_NULL => break,
            DT_NEEDED => names.push(val),
            DT_STRTAB => strtab = Some(val),
            DT_STRSZ => strsz = val,
            _ => {}
        }
    }
    if names.is_empty() {
        return Some(Vec::new());
    }
    // DT_STRTAB is an address: the PT_LOAD segment holding it says where
    // in the file it is.
    let vaddr = strtab?;
    let &(_, off, base, _) = segs
        .iter()
        .find(|&&(t, _, v, size)| t == PT_LOAD && v <= vaddr && vaddr < v.saturating_add(size))?;
    if strsz == 0 || strsz > 16 << 20 {
        return None;
    }
    let mut table = vec![0u8; strsz as usize];
    f.read_exact_at(&mut table, vaddr - base + off).ok()?;
    names
        .into_iter()
        .map(|i| {
            let s = table.get(i as usize..)?;
            let end = s.iter().position(|&b| b == 0)?;
            String::from_utf8(s[..end].to_vec()).ok()
        })
        .collect()
}

/// What a package's tree loads from elsewhere: every `DT_NEEDED` name of
/// its ELF files that no file in the tree itself is named (a library it
/// carries, which `$ORIGIN` finds, CEF's).
pub fn loads(tree: &Path) -> BTreeSet<String> {
    let (mut want, mut own) = (BTreeSet::new(), BTreeSet::new());
    walk(tree, &mut |p, is_file| {
        if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
            own.insert(n.to_string());
        }
        if is_file {
            want.extend(needed(p).unwrap_or_default());
        }
    });
    want.retain(|n| !own.contains(n));
    want
}

/// The names a package's tree puts in `/lib`: what is directly in its
/// `lib/` and not a directory (`/lib` links each entry of every linked
/// package's `lib/`; a library is a file or a link to one).
pub fn provides(tree: &Path) -> BTreeSet<String> {
    let Ok(rd) = std::fs::read_dir(tree.join("lib")) else { return BTreeSet::new() };
    rd.flatten()
        .filter(|e| !e.path().is_dir())
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect()
}

/// Every entry under `dir`, symlinks not followed; `is_file` for a regular
/// file.
fn walk(dir: &Path, f: &mut dyn FnMut(&Path, bool)) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(t) = e.file_type() else { continue };
        let p = e.path();
        f(&p, t.is_file());
        if t.is_dir() {
            walk(&p, f);
        }
    }
}

fn u16le(b: &[u8]) -> u16 {
    u16::from_le_bytes([b[0], b[1]])
}

fn u32le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn u64le(b: &[u8]) -> u64 {
    u64::from_le_bytes(b[..8].try_into().unwrap())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A minimal x86_64 ELF file whose dynamic segment loads `libs`: one
    /// PT_LOAD over the whole file, one PT_DYNAMIC, a string table.
    pub(crate) fn elf(libs: &[&str]) -> Vec<u8> {
        let mut strtab = vec![0u8];
        let mut offs = Vec::new();
        for l in libs {
            offs.push(strtab.len() as u64);
            strtab.extend_from_slice(l.as_bytes());
            strtab.push(0);
        }
        let (ph_off, n_ph) = (64u64, 2u64);
        let str_off = ph_off + 56 * n_ph;
        let dyn_off = (str_off + strtab.len() as u64 + 7) & !7;
        let mut dynamic: Vec<(u64, u64)> = offs.iter().map(|&o| (DT_NEEDED, o)).collect();
        dynamic.push((DT_STRTAB, 0x40_0000 + str_off));
        dynamic.push((DT_STRSZ, strtab.len() as u64));
        dynamic.push((DT_NULL, 0));
        let size = dyn_off + 16 * dynamic.len() as u64;
        let mut f = vec![0u8; size as usize];
        f[..4].copy_from_slice(b"\x7fELF");
        f[4] = 2;
        f[5] = 1;
        f[0x20..0x28].copy_from_slice(&ph_off.to_le_bytes());
        f[0x36..0x38].copy_from_slice(&56u16.to_le_bytes());
        f[0x38..0x3a].copy_from_slice(&(n_ph as u16).to_le_bytes());
        let mut ph = |i: u64, t: u32, off: u64, vaddr: u64, sz: u64| {
            let p = (ph_off + 56 * i) as usize;
            f[p..p + 4].copy_from_slice(&t.to_le_bytes());
            f[p + 8..p + 16].copy_from_slice(&off.to_le_bytes());
            f[p + 0x10..p + 0x18].copy_from_slice(&vaddr.to_le_bytes());
            f[p + 0x20..p + 0x28].copy_from_slice(&sz.to_le_bytes());
        };
        ph(0, PT_LOAD, 0, 0x40_0000, size);
        ph(1, PT_DYNAMIC, dyn_off, 0x40_0000 + dyn_off, 16 * dynamic.len() as u64);
        let s = str_off as usize;
        f[s..s + strtab.len()].copy_from_slice(&strtab);
        for (i, (t, v)) in dynamic.iter().enumerate() {
            let p = dyn_off as usize + 16 * i;
            f[p..p + 8].copy_from_slice(&t.to_le_bytes());
            f[p + 8..p + 16].copy_from_slice(&v.to_le_bytes());
        }
        f
    }

    #[test]
    fn reads_what_a_file_loads() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("prog");
        std::fs::write(&p, elf(&["libEGL.so.1", "libc.so.6"])).unwrap();
        assert_eq!(needed(&p).unwrap(), ["libEGL.so.1", "libc.so.6"]);
        std::fs::write(&p, "#!/bin/sh\n").unwrap();
        assert_eq!(needed(&p), None);
        std::fs::write(&p, elf(&[])).unwrap();
        assert_eq!(needed(&p).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn reads_a_real_program() {
        // The test runs on an Oath or a GNU/Linux host: its shell loads libc.
        let sh = std::fs::canonicalize("/bin/sh").unwrap();
        if let Some(libs) = needed(&sh) {
            assert!(libs.iter().any(|l| l.starts_with("libc.so")), "{libs:?}");
        }
    }

    #[test]
    fn a_tree_loads_what_it_does_not_carry() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("bin")).unwrap();
        std::fs::create_dir_all(d.path().join("lib/sola/dri")).unwrap();
        std::fs::write(d.path().join("bin/app"), elf(&["libcef.so", "libEGL.so.1"])).unwrap();
        std::fs::write(d.path().join("lib/sola/libcef.so"), elf(&["libc.so.6"])).unwrap();
        std::fs::write(d.path().join("lib/libsola.so.0"), elf(&[])).unwrap();
        std::os::unix::fs::symlink("libsola.so.0", d.path().join("lib/libsola.so")).unwrap();
        assert_eq!(
            loads(d.path()).into_iter().collect::<Vec<_>>(),
            ["libEGL.so.1", "libc.so.6"]
        );
        assert_eq!(
            provides(d.path()).into_iter().collect::<Vec<_>>(),
            ["libsola.so", "libsola.so.0"]
        );
    }
}
