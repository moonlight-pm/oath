//! The root filesystem's device, from `oath.root=`: a device node
//! (`/dev/sda2`) or `UUID=<fsid>` of a btrfs filesystem. The boot initrd
//! has no blkid, so PID 1 reads the btrfs superblocks itself.

use std::fs;
use std::os::unix::fs::FileExt;
use std::path::Path;

/// Where the primary btrfs superblock sits on its device.
const SUPER_OFFSET: u64 = 0x10000;
const MAGIC: &[u8; 8] = b"_BHRfS_M";

/// The device `want` names, if it is there now.
pub fn find(want: &str) -> Option<String> {
    match want.strip_prefix("UUID=") {
        Some(uuid) => find_btrfs(uuid),
        None => Path::new(want).exists().then(|| want.to_string()),
    }
}

/// The block device that holds the btrfs filesystem `uuid` (its fsid, as
/// `mkfs.btrfs -U` takes it and `blkid` prints it).
fn find_btrfs(uuid: &str) -> Option<String> {
    let want = uuid.to_ascii_lowercase();
    let mut names: Vec<String> = fs::read_dir("/sys/class/block")
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !["loop", "ram", "zram", "sr", "fd"].iter().any(|p| n.starts_with(p)))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|n| format!("/dev/{n}"))
        .find(|dev| btrfs_fsid(Path::new(dev)).as_deref() == Some(want.as_str()))
}

fn btrfs_fsid(dev: &Path) -> Option<String> {
    let f = fs::File::open(dev).ok()?;
    let mut sb = [0u8; 0x48];
    f.read_exact_at(&mut sb, SUPER_OFFSET).ok()?;
    parse_fsid(&sb)
}

/// The fsid of a btrfs superblock's first bytes: 32 of checksum, the
/// 16-byte fsid, bytenr and flags, then the magic.
fn parse_fsid(sb: &[u8]) -> Option<String> {
    if sb.get(0x40..0x48)? != MAGIC {
        return None;
    }
    let hex: String = sb[0x20..0x30].iter().map(|b| format!("{b:02x}")).collect();
    Some(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn superblock(fsid: [u8; 16]) -> Vec<u8> {
        let mut sb = vec![0u8; 0x48];
        sb[0x20..0x30].copy_from_slice(&fsid);
        sb[0x40..0x48].copy_from_slice(MAGIC);
        sb
    }

    #[test]
    fn reads_the_fsid_as_blkid_prints_it() {
        let id = [
            0x3f, 0x1e, 0x9c, 0x1e, 0x5b, 0x7a, 0x4c, 0x1d, 0x9d, 0x3e, 0x1a, 0x2b, 0x3c, 0x4d,
            0x5e, 0x6f,
        ];
        assert_eq!(parse_fsid(&superblock(id)).unwrap(), "3f1e9c1e-5b7a-4c1d-9d3e-1a2b3c4d5e6f");
        let mut not_btrfs = superblock(id);
        not_btrfs[0x40] = b'X';
        assert_eq!(parse_fsid(&not_btrfs), None);
        assert_eq!(parse_fsid(&[0u8; 0x20]), None);
    }

    #[test]
    fn finds_the_superblock_at_64k() {
        let d = tempfile::tempdir().unwrap();
        let img = d.path().join("img");
        let mut bytes = vec![0u8; SUPER_OFFSET as usize];
        bytes.extend(superblock([0xab; 16]));
        fs::write(&img, &bytes).unwrap();
        assert_eq!(btrfs_fsid(&img).unwrap(), "abababab-abab-abab-abab-abababababab");
        fs::write(&img, &bytes[..SUPER_OFFSET as usize]).unwrap();
        assert_eq!(btrfs_fsid(&img), None, "too short to hold a superblock");
    }

    #[test]
    fn a_node_is_found_when_it_exists() {
        assert_eq!(find("/dev/null").as_deref(), Some("/dev/null"));
        assert_eq!(find("/dev/no-such-disk"), None);
    }
}
