//! Guest/host-debug telemetry. Lines on stderr: `oath-tel {json}`.
//! Also appended to `/oath/log/<src>.jsonl` once the catalog is mounted,
//! rotated as service logs are.

use std::io::Write;
use std::path::Path;

use serde_json::{json, Value};

use crate::now_rfc3339;

const PREFIX: &str = "oath-tel ";
const LOG_DIR: &str = "/oath/log";
/// A log past this size moves to `<file>.1` before the next line, as PID 1
/// rotates a service's log: one old file is kept.
pub const LOG_MAX: u64 = 256 * 1024;

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
    let dir = Path::new(LOG_DIR);
    if dir.is_dir() {
        append_rotated(&dir.join(format!("{src}.jsonl")), format!("{line}\n").as_bytes());
    }
}

/// Append to a log, rotating it to `.1` past [`LOG_MAX`] (one old file
/// kept).
pub fn append_rotated(path: &Path, bytes: &[u8]) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > LOG_MAX) {
        let mut old = path.as_os_str().to_owned();
        old.push(".1");
        let _ = std::fs::rename(path, old);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = f.write_all(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_telemetry_log_keeps_one_old_file() {
        let d = tempfile::tempdir().unwrap();
        let log = d.path().join("init.jsonl");
        let old = d.path().join("init.jsonl.1");
        let line = format!("{}\n", "x".repeat(1023));
        for _ in 0..=LOG_MAX / 1024 {
            append_rotated(&log, line.as_bytes());
        }
        assert!(!old.exists(), "not past the limit until this line");
        append_rotated(&log, b"next\n");
        assert_eq!(std::fs::metadata(&old).unwrap().len(), LOG_MAX + 1024);
        assert_eq!(std::fs::read(&log).unwrap(), b"next\n");
        // The next rotation replaces the old file: never more than two.
        for _ in 0..LOG_MAX / 1024 {
            append_rotated(&log, line.as_bytes());
        }
        append_rotated(&log, b"last\n");
        assert!(std::fs::read(&old).unwrap().starts_with(b"next\n"));
        assert_eq!(std::fs::read(&log).unwrap(), b"last\n");
        let files = std::fs::read_dir(d.path()).unwrap().count();
        assert_eq!(files, 2);
    }
}
