//! Catalog, kinds, apply/undo. The live tree is `/oath` on an appliance.

mod catalog;
pub mod cell;
mod dev;
mod error;
pub mod efi;
pub mod elf;
pub mod esp;
mod hooks;
pub mod head;
pub mod hid;
mod id;
mod index;
mod kinds;
mod layout;
pub mod login;
pub mod mounts;
pub mod origin;
mod net;
mod packhash;
mod pkg;
mod plan;
mod plan_build;
pub mod probation;
pub mod reap;
pub mod schedule;
pub mod schema;
pub mod seat;
mod seed;
mod ssh;
pub mod sshsig;
mod svc_order;
mod svc_run;
mod tel;
pub mod update;

pub use catalog::{
    copy_objects, diff_values, status_path, Catalog, CatalogLock, Drift, Object, Verified,
    STATUS_DIR,
};
pub use dev::{converge as converge_dev, discover as discover_devices};
pub use error::{Error, Result};
pub use hooks::{Actor, ApplyHooks, ApplyReport, NullHooks, Slept};
pub use id::ObjectId;
pub use kinds::{
    Cell, CellActual, Dev, DevActual, DhcpLease, Host, HostPower, Meta, Mount, Net, Pkg, PkgActual,
    PkgAdvisory, PkgComponent, PkgNeed, PkgRealization, PkgSource, PkgVerified, Plan, PlanActual, Snap, Ssh,
    SshActual,
    Svc, SvcActual, SvcRestart, SvcSeat, SvcUser,
};
pub use layout::{
    boot_subvol, boot_subvol_name, gen_subvol_name, parse_boot_subvol, parse_gen_subvol,
    rotate_boot_ids, BOOT_ARCHIVES, BTRFS_TOP, LIVE_SUBVOL, MACHINE_ID,
};
pub use net::{
    appliance_desired as net_appliance_desired, converge as converge_net, dhcpcd_args,
    dhcpcd_oneshot, record_lease, DHCP_DIRS, DHCP_HOOK, LEASE_FILE,
};
pub use packhash::{
    fetch_adv_url, fetch_url, hash_tree, is_origin, is_realization_id, realization_dir, slot_dir,
    tree_size, HASH_PREFIX, LIVE_NAME,
};
pub use pkg::{
    check_needs as check_pkg_needs, converge as converge_pkg, converge_with_link_root, fetch_adv,
    fetch_and_converge as fetch_and_converge_pkg, fetch_pkg, fetch_pkg_any, find_bin, host_origins,
    ingest_file, install_tree, link_clashes,
    list_realization_ids, list_realizations, normalize_need as normalize_pkg_need,
    pin_desired_hash, prefetch_pkg_any, program_realization, promote_slot, promote_store, put_tree,
    own_version, read_advisories, read_advisories_store, read_components, resolve_tree, store_pkg,
    store_present, sync_need_hashes, verify_tree, write_advisories, write_advisories_store,
    write_slot_links, COMPONENTS_NAME,
};
pub use plan::{
    file_hash, fmt_plan, input_set_hash, lint_bytes, lint_path, parse_plan, PlanFile, PLAN_NAME,
};
pub use plan_build::{
    build_file, build_file_concurrent, build_log, build_pinned, file_plan, record_build,
    BuildReport,
};
pub use probation::Probation;
pub use seed::{
    apply_overlay, is_profile_name, merge_patch, profiles, seed, seed_dir, seed_profile,
    write_schemas, Profile, DEFAULT_PROFILE, SEED_DIR, SEED_DIR_ENV,
};
pub use ssh::{converge as converge_ssh, DROPBEAR_HOST_KEY, HOST_KEY as SSH_HOST_KEY, SSHD_CONFIG};
pub use svc_order::{
    start_order as svc_start_order, start_order_lenient as svc_start_order_lenient,
    stop_layers as svc_stop_layers, stuck_reasons as svc_stuck_reasons,
};
pub use svc_run::{apply_svc_cred, svc_env_pairs, svc_spawn, SvcCred, SvcSpawn};
pub use tel::{append_rotated, tel};

pub const DEFAULT_ROOT: &str = "/oath";
pub const EXIT_CONFIRM: i32 = 3;

pub const KIND_HOST: &str = "host";
pub const KIND_SVC: &str = "svc";
pub const KIND_SNAP: &str = "snap";
pub const KIND_PKG: &str = "pkg";
pub const KIND_NET: &str = "net";
pub const KIND_SSH: &str = "ssh";
pub const KIND_DEV: &str = "dev";
pub const KIND_PLAN: &str = "plan";
pub const KIND_CELL: &str = "cell";
pub const EXIT_SECURITY: i32 = 4;

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// `s`, an RFC 3339 time, in UTC to the second (`2026-11-06T12:00:00Z`), as
/// [`now_rfc3339`] writes it: such times order as text. `None` if it is not one.
pub fn rfc3339_utc(s: &str) -> Option<String> {
    let t = chrono::DateTime::parse_from_rfc3339(s).ok()?;
    Some(t.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

/// Pretty JSON, written atomically: temp file, fsync, rename, fsync dir.
/// A reader (or a crash) sees the old file or the new one, never half.
pub fn write_json(path: &std::path::Path, value: &impl serde::Serialize) -> Result<()> {
    let mut s = serde_json::to_string_pretty(value)?;
    if !s.ends_with('\n') {
        s.push('\n');
    }
    write_atomic(path, s.as_bytes())
}

/// Replace `path` with `bytes` atomically (see [`write_json`]).
pub fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.tmp.{}", std::process::id()));
    let result = (|| -> Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Write all of `bytes` to a child's stdin or a socket, where a reader that
/// has gone is an error and not the end of `oath`, which leaves SIGPIPE at
/// its default so that `oath … | head` ends quietly.
pub fn write_to_peer(w: &mut impl std::io::Write, bytes: &[u8]) -> std::io::Result<()> {
    let old = unsafe { libc::signal(libc::SIGPIPE, libc::SIG_IGN) };
    let r = w.write_all(bytes);
    unsafe { libc::signal(libc::SIGPIPE, old) };
    r
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Result<T> {
    let s = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&s)?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_reader_that_went_is_an_error_and_sigpipe_stays_as_it_was() {
        let mut fds = [0; 2];
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        unsafe { libc::close(fds[0]) };
        let mut w = unsafe { <std::fs::File as std::os::fd::FromRawFd>::from_raw_fd(fds[1]) };
        let before = unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
        let r = super::write_to_peer(&mut w, b"x");
        let after = unsafe { libc::signal(libc::SIGPIPE, before) };
        assert_eq!(r.unwrap_err().kind(), std::io::ErrorKind::BrokenPipe);
        assert_eq!(after, libc::SIG_DFL);
    }
}
