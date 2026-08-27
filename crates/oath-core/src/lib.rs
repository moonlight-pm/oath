//! Catalog, kinds, apply/undo. The live tree is `/oath` on an appliance.

mod catalog;
mod error;
mod hooks;
mod id;
mod index;
mod kinds;
mod seed;
mod tel;

pub use catalog::{diff_values, Catalog, Drift, Object};
pub use error::{Error, Result};
pub use hooks::{Actor, ApplyHooks, ApplyReport, NullHooks};
pub use id::ObjectId;
pub use kinds::{Host, HostPower, Meta, Svc, SvcActual, SvcRestart};
pub use seed::seed;
pub use tel::tel;

pub const DEFAULT_ROOT: &str = "/oath";
pub const EXIT_CONFIRM: i32 = 3;

pub const KIND_HOST: &str = "host";
pub const KIND_SVC: &str = "svc";
pub const KIND_SNAP: &str = "snap";

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn write_json(path: &std::path::Path, value: &impl serde::Serialize) -> Result<()> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    let mut s = serde_json::to_string_pretty(value)?;
    if !s.ends_with('\n') {
        s.push('\n');
    }
    std::fs::write(path, s)?;
    Ok(())
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Result<T> {
    let s = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&s)?)
}
