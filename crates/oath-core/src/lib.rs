//! Catalog, kinds, apply/undo. The live tree is `/oath` on an appliance.

mod catalog;
mod dev;
mod error;
mod hooks;
mod id;
mod index;
mod kinds;
mod layout;
mod net;
mod packhash;
mod pkg;
mod plan;
mod plan_build;
pub mod probation;
pub mod schema;
pub mod seat;
mod seed;
mod ssh;
mod svc_order;
mod svc_run;
mod tel;

pub use catalog::{copy_objects, diff_values, Catalog, CatalogLock, Drift, Object, Verified};
pub use dev::{converge as converge_dev, discover as discover_devices};
pub use error::{Error, Result};
pub use hooks::{Actor, ApplyHooks, ApplyReport, NullHooks};
pub use id::ObjectId;
pub use kinds::{
    Dev, DevActual, DhcpLease, Host, HostPower, Meta, Net, Pkg, PkgActual, PkgAdvisory,
    PkgComponent, PkgNeed, PkgRealization, PkgVerified, Plan, PlanActual, Snap, Ssh, SshActual,
    Svc, SvcActual, SvcRestart, SvcUser,
};
pub use layout::{
    boot_subvol, boot_subvol_name, gen_subvol_name, parse_boot_subvol, parse_gen_subvol,
    rotate_boot_ids, BOOT_ARCHIVES, BTRFS_TOP, LIVE_SUBVOL,
};
pub use net::{
    appliance_desired as net_appliance_desired, converge as converge_net, record_lease, LEASE_FILE,
};
pub use packhash::{
    fetch_adv_url, fetch_url, hash_tree, is_realization_id, realization_dir, slot_dir, HASH_PREFIX,
    LIVE_NAME,
};
pub use pkg::{
    check_needs as check_pkg_needs, converge as converge_pkg, converge_with_link_root, fetch_adv,
    fetch_pkg, ingest_file, install_tree, list_realization_ids, list_realizations,
    normalize_need as normalize_pkg_need, pin_desired_hash, promote_slot, promote_store, put_tree,
    read_advisories, read_advisories_store, read_components, resolve_tree, store_present,
    sync_need_hashes, verify_tree, write_advisories, write_advisories_store, write_slot_links,
    COMPONENTS_NAME,
};
pub use plan::{
    file_hash, fmt_plan, input_set_hash, lint_bytes, lint_path, parse_plan, PlanFile, PLAN_NAME,
};
pub use plan_build::{build_file, build_log, build_pinned, file_plan, BuildReport};
pub use probation::Probation;
pub use seed::{
    apply_overlay, merge_patch, seed, seed_profile, write_schemas, Profile, DEFAULT_PROFILE,
    PROFILES,
};
pub use ssh::{converge as converge_ssh, HOST_KEY as SSH_HOST_KEY};
pub use svc_order::{
    start_order as svc_start_order, start_order_lenient as svc_start_order_lenient,
    stop_layers as svc_stop_layers, stuck_reasons as svc_stuck_reasons,
};
pub use svc_run::{apply_svc_cred, svc_env_pairs, svc_spawn, SvcCred, SvcSpawn};
pub use tel::tel;

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
pub const EXIT_SECURITY: i32 = 4;

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
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

pub fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Result<T> {
    let s = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&s)?)
}
