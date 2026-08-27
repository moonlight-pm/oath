//! Guest/host-debug telemetry. Lines on stderr: `oath-tel {json}`.
//! Also appended to `/oath/log/<src>.jsonl` once the catalog is mounted.

use std::io::Write;
use std::path::Path;

use serde_json::{json, Value};

use crate::now_rfc3339;

const PREFIX: &str = "oath-tel ";

pub fn tel(src: &str, event: &str, mut extra: Value) {
    if !extra.is_object() {
        extra = json!({});
    }
    let o = extra.as_object_mut().unwrap();
    o.entry("ts").or_insert_with(|| json!(now_rfc3339()));
    o.insert("src".into(), json!(src));
    o.insert("event".into(), json!(event));
    let line = serde_json::to_string(&extra).unwrap_or_else(|_| "{}".into());
    let _ = writeln!(std::io::stderr(), "{PREFIX}{line}");
    let path = Path::new("/oath/log").join(format!("{src}.jsonl"));
    if Path::new("/oath/log").is_dir() {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
}
