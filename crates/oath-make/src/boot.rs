//! ESP boot generations: the rotation is oath-core's (`oath update`
//! writes a machine's ESP with it too).

use anyhow::{bail, Result};

pub use oath_core::esp::*;

pub fn require_confirm(confirm: bool) -> Result<()> {
    if !confirm {
        bail!("refusing to write the ESP without --confirm");
    }
    Ok(())
}

/// The root the running kernel was booted with (`oath.root=` on its
/// command line): the UUID an install wrote, so it names the same disk
/// whatever order the disks are found in.
pub fn booted_root(cmdline: &str) -> Option<String> {
    cmdline.split_whitespace().find_map(|t| t.strip_prefix("oath.root=")).map(str::to_string)
}

/// The options of the current entry that `kernel_options` does not write
/// itself, as `oath update` keeps them.
pub fn extra_options(current: &str) -> String {
    let own = kernel_options("", "", "");
    let own: Vec<&str> = own.split_whitespace().filter(|t| !t.starts_with("oath.")).collect();
    current
        .split_whitespace()
        .filter(|t| {
            !own.contains(t) && !t.starts_with("oath.root=") && !t.starts_with("oath.subvol=")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn booted_root_is_the_command_lines() {
        let line = "console=ttyS0 oath.root=UUID=3f1e9c1e-5b7a oath.subvol=@ quiet\n";
        assert_eq!(booted_root(line).as_deref(), Some("UUID=3f1e9c1e-5b7a"));
        assert_eq!(booted_root("console=ttyS0 quiet"), None);
    }

    #[test]
    fn extra_options_keep_only_what_the_entry_added() {
        let cur = kernel_options("UUID=3f1e9c1e-5b7a", "@", "quiet amdgpu.dc=0");
        assert_eq!(extra_options(&cur), "quiet amdgpu.dc=0");
        assert_eq!(extra_options(&kernel_options("/dev/sda2", "@", "")), "");
    }
}
