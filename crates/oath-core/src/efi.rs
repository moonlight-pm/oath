//! UEFI boot entries: the firmware's `Boot####` load options and its
//! `BootOrder`, kept in its variable store and read and written through
//! efivarfs. These are settings, as BIOS setup's boot order is; nothing
//! here writes the firmware itself. `oath install` adds one entry for the
//! disk it wrote, puts it first, and removes the other systems' entries
//! (those that load a file from a disk partition); the firmware's own
//! entries (its shell, setup, network and removable-media boots) and every
//! other variable stay as they were.

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;

use crate::{Error, Result};

/// EFI_GLOBAL_VARIABLE, the vendor GUID of `Boot####` and `BootOrder`.
pub const GLOBAL_GUID: &str = "8be4df61-93ca-11d2-aa0d-00e098032b8c";
/// Where the kernel's efivarfs is mounted.
pub const EFIVARS: &str = "/sys/firmware/efi/efivars";
/// NON_VOLATILE | BOOTSERVICE_ACCESS | RUNTIME_ACCESS, as every boot
/// variable has.
pub const BOOT_ATTRS: u32 = 0x7;
/// LOAD_OPTION_ACTIVE: the firmware may boot it.
pub const LOAD_OPTION_ACTIVE: u32 = 0x1;
/// The removable-media path every UEFI firmware looks for, and the only
/// loader an Oath ESP has (`oath-efi`).
pub const OATH_LOADER: &str = r"\EFI\BOOT\BOOTX64.EFI";

/// One `Boot####` variable's EFI_LOAD_OPTION.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadOption {
    pub attributes: u32,
    pub description: String,
    /// The device path list, end node included.
    pub path: Vec<u8>,
    pub optional: Vec<u8>,
}

impl LoadOption {
    pub fn parse(data: &[u8]) -> Option<Self> {
        let attributes = u32::from_le_bytes(data.get(0..4)?.try_into().ok()?);
        let path_len = usize::from(u16::from_le_bytes(data.get(4..6)?.try_into().ok()?));
        let mut i = 6;
        let mut units = Vec::new();
        loop {
            let u = u16::from_le_bytes(data.get(i..i + 2)?.try_into().ok()?);
            i += 2;
            if u == 0 {
                break;
            }
            units.push(u);
        }
        let path = data.get(i..i + path_len)?.to_vec();
        let optional = data[i + path_len..].to_vec();
        Some(Self { attributes, description: String::from_utf16_lossy(&units), path, optional })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.attributes.to_le_bytes());
        out.extend_from_slice(&(self.path.len() as u16).to_le_bytes());
        out.extend(ucs2(&self.description));
        out.extend_from_slice(&self.path);
        out.extend_from_slice(&self.optional);
        out
    }

    /// An operating system's entry: it loads a file from a disk partition
    /// (a hard-drive node, then a file-path node). The firmware's own
    /// entries name a firmware volume, a whole device or a network card.
    pub fn loads_from_disk(&self) -> bool {
        let nodes = nodes(&self.path);
        nodes.iter().any(|n| n.0 == 4 && n.1 == 1) && nodes.iter().any(|n| n.0 == 4 && n.1 == 4)
    }

    /// The file it loads, if it names one.
    pub fn file(&self) -> Option<String> {
        nodes(&self.path).iter().find(|n| n.0 == 4 && n.1 == 4).map(|n| from_ucs2(n.2))
    }

    /// The GPT partition GUID it loads from, if it names one.
    pub fn partition_guid(&self) -> Option<String> {
        let hd = nodes(&self.path).into_iter().find(|n| n.0 == 4 && n.1 == 1)?;
        // number u32, start u64, size u64, signature [16], format u8, type u8
        let b = hd.2;
        if b.len() < 38 || b[37] != 0x02 {
            return None;
        }
        Some(guid_string(b[20..36].try_into().ok()?))
    }
}

/// A device path's nodes as (type, subtype, data), up to the end node.
fn nodes(path: &[u8]) -> Vec<(u8, u8, &[u8])> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= path.len() {
        let (t, s) = (path[i], path[i + 1]);
        let len = usize::from(u16::from_le_bytes([path[i + 2], path[i + 3]]));
        if t == 0x7f || len < 4 || i + len > path.len() {
            break;
        }
        out.push((t, s, &path[i + 4..i + len]));
        i += len;
    }
    out
}

fn ucs2(s: &str) -> Vec<u8> {
    s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
}

fn from_ucs2(b: &[u8]) -> String {
    let units: Vec<u16> = b
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// A GUID string as UEFI stores it: the first three fields little-endian.
pub fn guid_bytes(s: &str) -> Option<[u8; 16]> {
    let hex: String = s.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 || s.len() != 36 {
        return None;
    }
    let mut raw = [0u8; 16];
    for (i, b) in raw.iter_mut().enumerate() {
        *b = u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    let mut out = raw;
    out[0..4].copy_from_slice(&[raw[3], raw[2], raw[1], raw[0]]);
    out[4..6].copy_from_slice(&[raw[5], raw[4]]);
    out[6..8].copy_from_slice(&[raw[7], raw[6]]);
    Some(out)
}

pub fn guid_string(b: [u8; 16]) -> String {
    let order = [3, 2, 1, 0, 5, 4, 7, 6, 8, 9, 10, 11, 12, 13, 14, 15];
    let hex: Vec<String> = order.iter().map(|&i| format!("{:02x}", b[i])).collect();
    [&hex[0..4], &hex[4..6], &hex[6..8], &hex[8..10], &hex[10..16]]
        .map(|g| g.concat())
        .join("-")
}

/// HD(part, GPT, guid, start, size)/File(file): partition `part` of a GPT
/// disk, its start and size in the disk's logical blocks.
pub fn disk_file_path(part: u32, start: u64, size: u64, guid: &str, file: &str) -> Option<Vec<u8>> {
    let mut p = vec![0x04, 0x01, 42, 0];
    p.extend_from_slice(&part.to_le_bytes());
    p.extend_from_slice(&start.to_le_bytes());
    p.extend_from_slice(&size.to_le_bytes());
    p.extend_from_slice(&guid_bytes(guid)?);
    p.extend_from_slice(&[0x02, 0x02]);
    let name = ucs2(file);
    p.extend_from_slice(&[0x04, 0x04]);
    p.extend_from_slice(&((4 + name.len()) as u16).to_le_bytes());
    p.extend(name);
    p.extend_from_slice(&[0x7f, 0xff, 0x04, 0x00]);
    Some(p)
}

pub fn parse_boot_order(data: &[u8]) -> Vec<u16> {
    data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
}

pub fn encode_boot_order(order: &[u16]) -> Vec<u8> {
    order.iter().flat_map(|n| n.to_le_bytes()).collect()
}

/// `Boot000A`'s efivarfs file name.
pub fn var_file(name: &str) -> String {
    format!("{name}-{GLOBAL_GUID}")
}

pub fn boot_name(n: u16) -> String {
    format!("Boot{n:04X}")
}

/// The number of a `Boot####-<global guid>` efivarfs file name.
pub fn parse_boot_file(file: &str) -> Option<u16> {
    let rest = file.strip_prefix("Boot")?;
    let (num, guid) = rest.split_once('-')?;
    if num.len() != 4 || !guid.eq_ignore_ascii_case(GLOBAL_GUID) {
        return None;
    }
    u16::from_str_radix(num, 16).ok()
}

/// What `oath install` does to the boot entries: `add` a new entry,
/// `remove` the other systems', `keep` the firmware's, and the new order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootPlan {
    pub add: u16,
    pub remove: Vec<(u16, String)>,
    pub keep: Vec<(u16, String)>,
    pub order: Vec<u16>,
}

pub fn plan_boot(entries: &[(u16, LoadOption)], order: &[u16]) -> BootPlan {
    let add = (0..=u16::MAX).find(|n| !entries.iter().any(|(e, _)| e == n)).unwrap_or(0);
    let mut remove = Vec::new();
    let mut keep = Vec::new();
    for (n, o) in entries {
        if o.loads_from_disk() {
            remove.push((*n, o.description.clone()));
        } else {
            keep.push((*n, o.description.clone()));
        }
    }
    let mut new_order = vec![add];
    new_order.extend(order.iter().filter(|n| keep.iter().any(|(k, _)| k == *n)));
    BootPlan { add, remove, keep, order: new_order }
}

/// Every `Boot####` in `dir` (an efivarfs), by number.
pub fn read_entries(dir: &Path) -> Result<Vec<(u16, LoadOption)>> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir)? {
        let name = e?.file_name().to_string_lossy().to_string();
        let Some(n) = parse_boot_file(&name) else { continue };
        if let Some((_, data)) = read_var(dir, &boot_name(n))? {
            if let Some(o) = LoadOption::parse(&data) {
                out.push((n, o));
            }
        }
    }
    out.sort_by_key(|(n, _)| *n);
    Ok(out)
}

pub fn read_boot_order(dir: &Path) -> Result<Vec<u16>> {
    Ok(read_var(dir, "BootOrder")?.map(|(_, d)| parse_boot_order(&d)).unwrap_or_default())
}

/// A variable's attributes and data, or None when it does not exist.
pub fn read_var(dir: &Path, name: &str) -> Result<Option<(u32, Vec<u8>)>> {
    match fs::read(dir.join(var_file(name))) {
        Ok(b) if b.len() >= 4 => {
            Ok(Some((u32::from_le_bytes([b[0], b[1], b[2], b[3]]), b[4..].to_vec())))
        }
        Ok(_) => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Set a variable in one write, as efivarfs requires. An existing
/// variable is immutable (efivarfs marks them so that a stray `rm -rf`
/// cannot delete them); only this one is made writable first.
pub fn write_var(dir: &Path, name: &str, attrs: u32, data: &[u8]) -> Result<()> {
    let path = dir.join(var_file(name));
    if path.exists() {
        clear_immutable(&path)?;
    }
    let mut buf = attrs.to_le_bytes().to_vec();
    buf.extend_from_slice(data);
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .mode(0o644)
        .open(&path)
        .map_err(|e| Error::Msg(format!("open {}: {e}", path.display())))?;
    f.write_all(&buf).map_err(|e| Error::Msg(format!("write {}: {e}", path.display())))?;
    Ok(())
}

pub fn delete_var(dir: &Path, name: &str) -> Result<()> {
    let path = dir.join(var_file(name));
    if !path.exists() {
        return Ok(());
    }
    clear_immutable(&path)?;
    fs::remove_file(&path).map_err(|e| Error::Msg(format!("delete {}: {e}", path.display())))
}

fn clear_immutable(path: &Path) -> Result<()> {
    const FS_IOC_GETFLAGS: libc::c_ulong = 0x8008_6601;
    const FS_IOC_SETFLAGS: libc::c_ulong = 0x4008_6602;
    const FS_IMMUTABLE_FL: libc::c_long = 0x10;
    let f = fs::File::open(path)?;
    let mut flags: libc::c_long = 0;
    // SAFETY: the ioctls read and write one long through a valid pointer.
    unsafe {
        if libc::ioctl(f.as_raw_fd(), FS_IOC_GETFLAGS as _, &mut flags) != 0 {
            return Err(Error::Msg(format!(
                "flags of {}: {}",
                path.display(),
                std::io::Error::last_os_error()
            )));
        }
        if flags & FS_IMMUTABLE_FL != 0 {
            flags &= !FS_IMMUTABLE_FL;
            if libc::ioctl(f.as_raw_fd(), FS_IOC_SETFLAGS as _, &flags) != 0 {
                return Err(Error::Msg(format!(
                    "make {} writable: {}",
                    path.display(),
                    std::io::Error::last_os_error()
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ESP_GUID: &str = "f1d10dcb-0c9c-46e9-9228-e952ac081e6e";

    fn oath_entry() -> LoadOption {
        LoadOption {
            attributes: LOAD_OPTION_ACTIVE,
            description: "Oath".into(),
            path: disk_file_path(1, 2048, 1_048_576, ESP_GUID, OATH_LOADER).unwrap(),
            optional: vec![],
        }
    }

    #[test]
    fn guid_is_mixed_endian() {
        let b = guid_bytes(ESP_GUID).unwrap();
        assert_eq!(&b[..8], &[0xcb, 0x0d, 0xd1, 0xf1, 0x9c, 0x0c, 0xe9, 0x46]);
        assert_eq!(&b[8..], &[0x92, 0x28, 0xe9, 0x52, 0xac, 0x08, 0x1e, 0x6e]);
        assert_eq!(guid_string(b), ESP_GUID);
        assert!(guid_bytes("not-a-guid").is_none());
    }

    #[test]
    fn load_option_round_trips() {
        let o = oath_entry();
        let bytes = o.encode();
        // attributes, path length, "Oath\0" in UCS-2, then the path.
        assert_eq!(&bytes[..4], &[1, 0, 0, 0]);
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]) as usize, o.path.len());
        assert_eq!(&bytes[6..16], &[b'O', 0, b'a', 0, b't', 0, b'h', 0, 0, 0]);
        // HD node 42 bytes, File node 4 + 22 chars * 2, end node 4.
        assert_eq!(o.path.len(), 42 + 4 + (OATH_LOADER.len() + 1) * 2 + 4);
        let back = LoadOption::parse(&bytes).unwrap();
        assert_eq!(back, o);
        assert!(back.loads_from_disk());
        assert_eq!(back.file().as_deref(), Some(OATH_LOADER));
        assert_eq!(back.partition_guid().as_deref(), Some(ESP_GUID));
    }

    #[test]
    fn firmware_entries_are_not_a_systems() {
        // OVMF's UiApp: a firmware volume, then a firmware file (4/7, 4/6).
        let mut fv = vec![0x04, 0x07, 20, 0];
        fv.extend([0u8; 16]);
        fv.extend([0x04, 0x06, 20, 0]);
        fv.extend([0u8; 16]);
        fv.extend([0x7f, 0xff, 4, 0]);
        let ui = LoadOption {
            attributes: LOAD_OPTION_ACTIVE,
            description: "UiApp".into(),
            path: fv,
            optional: vec![],
        };
        assert!(!ui.loads_from_disk());
        // A whole disk (PCI then SATA), the firmware's removable boot.
        let disk = LoadOption {
            attributes: LOAD_OPTION_ACTIVE,
            description: "UEFI QEMU HARDDISK".into(),
            path: vec![2, 1, 12, 0, 0xd0, 0x41, 3, 0x0a, 0, 0, 0, 0, 0x7f, 0xff, 4, 0],
            optional: vec![],
        };
        assert!(!disk.loads_from_disk());
        let nixos = LoadOption {
            description: "Linux Boot Manager".into(),
            path: disk_file_path(
                1,
                4096,
                4_194_304,
                "377a00de-6b10-4350-91db-f0dc79d49adb",
                r"\EFI\systemd\systemd-bootx64.efi",
            )
            .unwrap(),
            ..ui.clone()
        };
        let entries = vec![(0, ui), (1, disk), (3, nixos)];
        let plan = plan_boot(&entries, &[3, 0, 1, 9]);
        assert_eq!(plan.add, 2);
        assert_eq!(plan.remove, vec![(3, "Linux Boot Manager".to_string())]);
        assert_eq!(plan.keep.len(), 2);
        // Oath first, then the firmware's in their order; gone and unknown left out.
        assert_eq!(plan.order, vec![2, 0, 1]);
    }

    #[test]
    fn boot_order_and_names() {
        assert_eq!(parse_boot_order(&encode_boot_order(&[2, 0x1a, 0])), vec![2, 0x1a, 0]);
        assert_eq!(boot_name(0x1a), "Boot001A");
        assert_eq!(parse_boot_file(&var_file("Boot001A")), Some(0x1a));
        assert_eq!(parse_boot_file(&var_file("BootOrder")), None);
        assert_eq!(parse_boot_file("Boot0001-00000000-0000-0000-0000-000000000000"), None);
    }

    #[test]
    fn write_read_delete_in_a_plain_dir() {
        // tmpfs and ext4 have the immutable flag too; a fresh file has it clear.
        let d = tempfile::tempdir().unwrap();
        let o = oath_entry();
        write_var(d.path(), "Boot0002", BOOT_ATTRS, &o.encode()).unwrap();
        write_var(d.path(), "BootOrder", BOOT_ATTRS, &encode_boot_order(&[2])).unwrap();
        let entries = read_entries(d.path()).unwrap();
        assert_eq!(entries, vec![(2, o)]);
        assert_eq!(read_boot_order(d.path()).unwrap(), vec![2]);
        assert_eq!(read_var(d.path(), "BootOrder").unwrap().unwrap().0, BOOT_ATTRS);
        delete_var(d.path(), "Boot0002").unwrap();
        assert!(read_entries(d.path()).unwrap().is_empty());
        delete_var(d.path(), "Boot0002").unwrap();
    }
}
