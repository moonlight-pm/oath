//! On-disk paths the guest and host tools share.

/// Top-level of the system btrfs (subvolid=0), mounted by PID 1.
/// Live root is `@`. Catalog undo gens are sibling `@gen-N`. Boot
/// archives are sibling `@boot-N`. None of these nest under `/`.
pub const BTRFS_TOP: &str = "/oath/run/fs";
pub const LIVE_SUBVOL: &str = "@";

/// Archived ESP boots kept besides current. Firmware menu shows these
/// plus the default entry.
pub const BOOT_ARCHIVES: usize = 5;

pub fn gen_subvol_name(n: u64) -> String {
    format!("@gen-{n}")
}

pub fn parse_gen_subvol(name: &str) -> Option<u64> {
    parse_numbered(name, "@gen-")
}

pub fn boot_subvol_name(n: u64) -> String {
    format!("@boot-{n}")
}

pub fn parse_boot_subvol(name: &str) -> Option<u64> {
    parse_numbered(name, "@boot-")
}

fn parse_numbered(name: &str, prefix: &str) -> Option<u64> {
    let rest = name.strip_prefix(prefix)?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// Kernel cmdline `oath.subvol=` value. Rejects path separators and
/// unknown names so a bad BLS line cannot mount an arbitrary subvol.
pub fn boot_subvol(name: &str) -> Option<&str> {
    let name = name.trim();
    if name == LIVE_SUBVOL {
        return Some(LIVE_SUBVOL);
    }
    if parse_boot_subvol(name).is_some() || parse_gen_subvol(name).is_some() {
        return Some(name);
    }
    None
}

/// Next archive id and the ids to delete so at most `keep` archives remain
/// after recording `next`.
pub fn rotate_boot_ids(existing: &[u64], keep: usize) -> (u64, Vec<u64>) {
    let next = existing.iter().copied().max().unwrap_or(0).saturating_add(1).max(1);
    let mut keep_ids: Vec<u64> = existing.iter().copied().collect();
    keep_ids.sort_unstable();
    keep_ids.push(next);
    let drop = if keep_ids.len() > keep { keep_ids.len() - keep } else { 0 };
    let prune = keep_ids.iter().copied().take(drop).collect();
    (next, prune)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gen_names() {
        assert_eq!(gen_subvol_name(3), "@gen-3");
        assert_eq!(parse_gen_subvol("@gen-3"), Some(3));
        assert_eq!(parse_gen_subvol("3"), None);
        assert_eq!(parse_gen_subvol("@"), None);
        assert_eq!(parse_gen_subvol("@gen-"), None);
        assert_eq!(parse_gen_subvol("@gen-3/x"), None);
    }

    #[test]
    fn boot_names() {
        assert_eq!(boot_subvol_name(4), "@boot-4");
        assert_eq!(parse_boot_subvol("@boot-4"), Some(4));
        assert_eq!(boot_subvol("@"), Some("@"));
        assert_eq!(boot_subvol("@boot-4"), Some("@boot-4"));
        assert_eq!(boot_subvol("@gen-2"), Some("@gen-2"));
        assert_eq!(boot_subvol("@boot-4/../x"), None);
        assert_eq!(boot_subvol("boot-4"), None);
        assert_eq!(boot_subvol(""), None);
    }

    #[test]
    fn rotate_keeps_last_five() {
        let (next, prune) = rotate_boot_ids(&[1, 2, 3, 4, 5], BOOT_ARCHIVES);
        assert_eq!(next, 6);
        assert_eq!(prune, vec![1]);
        let (first, prune0) = rotate_boot_ids(&[], BOOT_ARCHIVES);
        assert_eq!(first, 1);
        assert!(prune0.is_empty());
    }
}
