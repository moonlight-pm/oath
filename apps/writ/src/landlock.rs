//! Landlock: the box gives up every path it does not need, for good, on top of the mount
//! plan. It holds where a mount does not, inside a nested namespace, so it is what a box that
//! runs `oath build` will rest on. What it can do depends on the kernel's ABI version.

use crate::sys;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

const EXECUTE: u64 = 1 << 0;
const WRITE_FILE: u64 = 1 << 1;
const READ_FILE: u64 = 1 << 2;
const READ_DIR: u64 = 1 << 3;
const MAKE_CHAR: u64 = 1 << 6;
const MAKE_BLOCK: u64 = 1 << 11;
const REFER: u64 = 1 << 13; // ABI 2
const TRUNCATE: u64 = 1 << 14; // ABI 3
const IOCTL_DEV: u64 = 1 << 15; // ABI 5
const RESOLVE_UNIX: u64 = 1 << 16; // ABI 9
/// EXECUTE through MAKE_SYM, what every version knows.
const ABI1: u64 = (1 << 13) - 1;

const SCOPE_ABSTRACT_UNIX: u64 = 1 << 0; // ABI 6
const SCOPE_SIGNAL: u64 = 1 << 1; // ABI 6

const CREATE_RULESET_VERSION: u32 = 1;
const RULE_PATH_BENEATH: u32 = 1;
const O_PATH: i32 = 0o10000000;
const O_CLOEXEC: i32 = 0o2000000;

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
    scoped: u64,
}

#[repr(C, packed)]
struct PathBeneath {
    allowed_access: u64,
    parent_fd: i32,
}

extern "C" {
    fn open(path: *const std::ffi::c_char, flags: i32, ...) -> i32;
}

/// The kernel's Landlock ABI, or 0 if it has none.
pub fn abi() -> i64 {
    let r = unsafe {
        sys::syscall(
            sys::SYS_LANDLOCK_CREATE_RULESET,
            std::ptr::null::<RulesetAttr>(),
            0usize,
            CREATE_RULESET_VERSION,
        )
    };
    r.max(0)
}

/// What this ABI can restrict on files.
fn handled(abi: i64) -> u64 {
    let mut h = ABI1;
    if abi >= 2 {
        h |= REFER;
    }
    if abi >= 3 {
        h |= TRUNCATE;
    }
    if abi >= 5 {
        h |= IOCTL_DEV;
    }
    if abi >= 9 {
        h |= RESOLVE_UNIX;
    }
    h
}

fn add_rule(ruleset: i32, path: &Path, allowed: u64) -> Result<(), String> {
    let c = CString::new(path.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    let fd = unsafe { open(c.as_ptr(), O_PATH | O_CLOEXEC) };
    if fd < 0 {
        return Err(format!("landlock: open {}: errno {}", path.display(), sys::errno()));
    }
    let rule = PathBeneath { allowed_access: allowed, parent_fd: fd };
    let r = unsafe {
        sys::syscall(
            sys::SYS_LANDLOCK_ADD_RULE,
            ruleset,
            RULE_PATH_BENEATH,
            &rule as *const PathBeneath,
            0u32,
        )
    };
    let e = sys::errno();
    unsafe { sys::close(fd) };
    if r != 0 {
        return Err(format!("landlock: rule {}: errno {e}", path.display()));
    }
    Ok(())
}

/// Read and execute anywhere in the box, change only `writable`. `readable` is for a file handed
/// in from outside the box's tree, which no rule on its root covers. `no_new_privs` must be set.
/// Returns the ABI used, or 0 on a kernel without Landlock.
pub fn restrict(writable: &[&Path], readable: &[&Path]) -> Result<i64, String> {
    let abi = abi();
    if abi == 0 {
        return Ok(0);
    }
    let h = handled(abi);
    let attr = RulesetAttr {
        handled_access_fs: h,
        // Not handled: the namespace has no network, and loopback is for tests.
        handled_access_net: 0,
        scoped: if abi >= 6 { SCOPE_ABSTRACT_UNIX | SCOPE_SIGNAL } else { 0 },
    };
    // A kernel takes the size it knows the fields for, and older ones know fewer.
    let size = if abi >= 6 {
        24usize
    } else if abi >= 4 {
        16
    } else {
        8
    };
    let ruleset = unsafe {
        sys::syscall(sys::SYS_LANDLOCK_CREATE_RULESET, &attr as *const RulesetAttr, size, 0u32)
    };
    if ruleset < 0 {
        return Err(format!("landlock: create ruleset: errno {}", sys::errno()));
    }
    let ruleset = ruleset as i32;

    add_rule(ruleset, Path::new("/"), EXECUTE | READ_FILE | READ_DIR)?;
    // Devices the box was given, and /proc/self's files, which toolchains write to.
    add_rule(ruleset, Path::new("/dev"), (READ_FILE | WRITE_FILE | READ_DIR | TRUNCATE | IOCTL_DEV) & h)?;
    add_rule(ruleset, Path::new("/proc"), (READ_FILE | WRITE_FILE | READ_DIR) & h)?;
    for w in writable {
        add_rule(ruleset, w, h & !(MAKE_CHAR | MAKE_BLOCK | IOCTL_DEV))?;
    }
    for r in readable {
        add_rule(ruleset, r, READ_FILE)?;
    }

    let r = unsafe { sys::syscall(sys::SYS_LANDLOCK_RESTRICT_SELF, ruleset, 0u32) };
    let e = sys::errno();
    unsafe { sys::close(ruleset) };
    if r != 0 {
        return Err(format!("landlock: restrict: errno {e}"));
    }
    Ok(abi)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handled_grows_with_the_abi() {
        assert_eq!(handled(1), ABI1);
        assert_eq!(handled(2), ABI1 | REFER);
        assert_eq!(handled(4), ABI1 | REFER | TRUNCATE);
        assert_eq!(handled(5), ABI1 | REFER | TRUNCATE | IOCTL_DEV);
        assert_eq!(handled(11), ABI1 | REFER | TRUNCATE | IOCTL_DEV | RESOLVE_UNIX);
    }

    #[test]
    fn the_rule_is_the_size_the_kernel_expects() {
        assert_eq!(std::mem::size_of::<PathBeneath>(), 12);
        assert_eq!(std::mem::size_of::<RulesetAttr>(), 24);
    }
}
