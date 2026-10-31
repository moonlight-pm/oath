//! Overlays (R47): a package this machine runs at a pin other than the one
//! the release it runs set (`/oath/output.json`). `oath get` and `oath
//! diff` name them; `oath update` replaces them unless told `--keep`.

use std::path::Path;

use oath_core::update::{self as up, Ran, Release};
use oath_core::{Catalog, Object, ObjectId, Result};
use serde_json::{json, Value};

/// `{output, release, live, source?}` when `obj` is a present package
/// whose live pin is not the release's.
pub fn of(root: &Path, obj: &Object) -> Option<Value> {
    if obj.id.kind != "pkg" || obj.actual["present"] != json!(true) {
        return None;
    }
    let record: Release = oath_core::read_json(&root.join(up::RECORD)).ok()?;
    let live = obj.actual["hash"].as_str()?;
    let pin = Ran::of(Some(&record)).overlay(&obj.id.name, live)?.to_string();
    let mut v = json!({ "output": record.output, "release": pin, "live": live });
    if let Some(src) = obj.actual.get("source").filter(|s| !s.is_null()) {
        v["source"] = src.clone();
    }
    Some(v)
}

/// One line for a person: what runs, and what the release has.
pub fn line(id: &ObjectId, ov: &Value) -> String {
    let live = ov["live"].as_str().unwrap_or("");
    let what = up::source_label(ov).unwrap_or_else(|| up::short(live).to_string());
    format!(
        "overlay: {id} is {what}, not the release's {} (output {}); `oath update` replaces it",
        up::short(ov["release"].as_str().unwrap_or("")),
        ov["output"].as_str().unwrap_or("")
    )
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
