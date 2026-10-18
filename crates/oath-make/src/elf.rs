//! What an ELF program needs from the loader (R17): its interpreter
//! (`PT_INTERP`) and the libraries it names (`DT_NEEDED`). The packer reads
//! them to put a ramdisk's programs there with exactly what they load.

use anyhow::{bail, Context, Result};

const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const DT_NULL: u64 = 0;
const DT_NEEDED: u64 = 1;
const DT_STRTAB: u64 = 5;
const DT_RPATH: u64 = 15;
const DT_RUNPATH: u64 = 29;

#[derive(Debug, Default, PartialEq)]
pub struct Needs {
    /// The loader, for a dynamically linked program.
    pub interp: Option<String>,
    /// `DT_NEEDED` sonames, in order.
    pub needed: Vec<String>,
    /// `DT_RUNPATH` (or `DT_RPATH`) entries, as written (`$ORIGIN`, …).
    pub runpath: Vec<String>,
}

/// Whether a file starting with `head` (64 bytes or more) is an x86_64
/// program or shared library: what the loader loads. Kernel modules,
/// objects and firmware for other cores are not.
pub fn is_loadable(head: &[u8]) -> bool {
    head.len() >= 20
        && &head[..4] == b"\x7fELF"
        && head[4] == 2
        && head[5] == 1
        && matches!(u16::from_le_bytes([head[16], head[17]]), ET_EXEC | ET_DYN)
        && u16::from_le_bytes([head[18], head[19]]) == EM_X86_64
}

/// The interpreter and needed libraries of an x86_64 ELF file. A static
/// program needs neither.
pub fn needs(data: &[u8]) -> Result<Needs> {
    if data.len() < 64 || &data[..4] != b"\x7fELF" {
        bail!("not an ELF file");
    }
    if data[4] != 2 || data[5] != 1 {
        bail!("not a 64-bit little-endian ELF file");
    }
    let u16_at = |o: usize| -> Result<u16> {
        Ok(u16::from_le_bytes(data.get(o..o + 2).context("truncated ELF")?.try_into()?))
    };
    let u32_at = |o: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(data.get(o..o + 4).context("truncated ELF")?.try_into()?))
    };
    let u64_at = |o: usize| -> Result<u64> {
        Ok(u64::from_le_bytes(data.get(o..o + 8).context("truncated ELF")?.try_into()?))
    };
    let (phoff, phentsize, phnum) =
        (u64_at(0x20)? as usize, u16_at(0x36)? as usize, u16_at(0x38)? as usize);
    let mut loads = Vec::new();
    let mut out = Needs::default();
    let mut dynamic = None;
    for i in 0..phnum {
        let ph = phoff + i * phentsize;
        let (offset, vaddr, filesz) =
            (u64_at(ph + 8)? as usize, u64_at(ph + 16)?, u64_at(ph + 32)? as usize);
        match u32_at(ph)? {
            PT_LOAD => loads.push((vaddr, offset, filesz)),
            PT_DYNAMIC => dynamic = Some((offset, filesz)),
            PT_INTERP => {
                let s = data.get(offset..offset + filesz).context("truncated PT_INTERP")?;
                out.interp = Some(cstr(s));
            }
            _ => {}
        }
    }
    let Some((doff, dsize)) = dynamic else { return Ok(out) };
    let mut names = Vec::new();
    let mut paths = Vec::new();
    let mut strtab = None;
    for e in (doff..doff + dsize).step_by(16) {
        match u64_at(e)? {
            DT_NULL => break,
            DT_NEEDED => names.push(u64_at(e + 8)? as usize),
            DT_RPATH | DT_RUNPATH => paths.push(u64_at(e + 8)? as usize),
            DT_STRTAB => strtab = Some(u64_at(e + 8)?),
            _ => {}
        }
    }
    if names.is_empty() && paths.is_empty() {
        return Ok(out);
    }
    // DT_STRTAB is an address: find the file offset of its segment.
    let addr = strtab.context("DT_NEEDED without DT_STRTAB")?;
    let (vaddr, offset, _) = loads
        .iter()
        .find(|(v, _, size)| addr >= *v && addr < v + *size as u64)
        .context("DT_STRTAB outside every PT_LOAD")?;
    let base = offset + (addr - vaddr) as usize;
    for n in names {
        out.needed.push(cstr(data.get(base + n..).context("bad DT_NEEDED")?));
    }
    for p in paths {
        let list = cstr(data.get(base + p..).context("bad DT_RUNPATH")?);
        out.runpath.extend(list.split(':').filter(|d| !d.is_empty()).map(str::to_string));
    }
    Ok(out)
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_build_hosts_own_programs() {
        // oath-make itself: dynamically linked against glibc on any host
        // that builds it, so it names a loader and libc.
        let me = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        let n = needs(&me).unwrap();
        assert!(n.interp.as_deref().is_some_and(|i| i.contains("ld-linux")), "{n:?}");
        assert!(n.needed.iter().any(|l| l.starts_with("libc.so")), "{n:?}");
        assert!(needs(b"#!/bin/sh\n").is_err());
        assert!(is_loadable(&me[..64]));
        assert!(!is_loadable(b"#!/bin/sh\n"));
    }
}
