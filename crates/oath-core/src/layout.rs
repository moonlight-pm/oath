//! On-disk paths the guest and host tools share.

/// Top-level of the system btrfs (subvolid=0), mounted by PID 1.
/// Live root is `@`. Generations are sibling `@gen-N` subvolumes — not
/// nested under the live `/`.
pub const BTRFS_TOP: &str = "/oath/run/fs";
pub const LIVE_SUBVOL: &str = "@";

pub fn gen_subvol_name(n: u64) -> String {
    format!("@gen-{n}")
}

pub fn parse_gen_subvol(name: &str) -> Option<u64> {
    name.strip_prefix("@gen-")?.parse().ok()
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
    }
}
