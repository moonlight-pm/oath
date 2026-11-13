//! Overlays (R47): a package this machine runs at a pin other than the one
//! it took: the head it took of that package (`/oath/heads.json`, R72),
//! or else the pin the release it runs set (`/oath/output.json`). `oath
//! get` and `oath diff` name them; `oath update` replaces them unless told
//! `--keep`.

use std::path::Path;

use oath_core::update as up;
use oath_core::{Catalog, Object, ObjectId, Result};
use serde_json::Value;

/// `{head, signer, live, source?}` when `obj` is a present package whose
/// live pin is not the head it took; else `{output, release, live,
/// source?}` when it is not the release's pin.
pub fn of(root: &Path, obj: &Object) -> Option<Value> {
    up::overlay_of(root, obj)
}

/// One line for a person: what runs, and what the head or release has.
pub fn line(id: &ObjectId, ov: &Value) -> String {
    let live = ov["live"].as_str().unwrap_or("");
    let what = up::source_label(ov).unwrap_or_else(|| up::short(live).to_string());
    let s = |k: &str| ov[k].as_str().unwrap_or("");
    let not = match ov.get("head") {
        Some(h) => format!(
            "its head {} (signed by {})",
            up::short(h.as_str().unwrap_or("")),
            s("signer")
        ),
        None => format!("the release's {} (output {})", up::short(s("release")), s("output")),
    };
    format!("overlay: {id} is {what}, not {not}; `oath update` replaces it")
}

/// Every overlay on this machine, or `only`'s.
pub fn all(root: &Path, cat: &Catalog, only: Option<&ObjectId>) -> Result<Vec<(ObjectId, Value)>> {
    let mut out = Vec::new();
    for id in cat.ls(Some("pkg"))? {
        if only.is_some_and(|o| *o != id) {
            continue;
        }
        if let Some(v) = of(root, &cat.get(&id)?) {
            out.push((id, v));
        }
    }
    Ok(out)
}
