use std::path::Path;

use serde_json::json;

use crate::kinds::Meta;
use crate::{write_json, ObjectId, Result, KIND_HOST, KIND_SNAP, KIND_SVC};

pub const HOST_SCHEMA: &str = include_str!("../schema/host.json");
pub const HOST_MD: &str = include_str!("../schema/host.md");
pub const SVC_SCHEMA: &str = include_str!("../schema/svc.json");
pub const SVC_MD: &str = include_str!("../schema/svc.md");
pub const SNAP_SCHEMA: &str = include_str!("../schema/snap.json");
pub const SNAP_MD: &str = include_str!("../schema/snap.md");

pub fn seed(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root.join("schema"))?;
    std::fs::create_dir_all(root.join("objects"))?;
    std::fs::create_dir_all(root.join("log"))?;
    std::fs::create_dir_all(root.join("run"))?;

    write(root, "schema/host.json", HOST_SCHEMA);
    write(root, "schema/host.md", HOST_MD);
    write(root, "schema/svc.json", SVC_SCHEMA);
    write(root, "schema/svc.md", SVC_MD);
    write(root, "schema/snap.json", SNAP_SCHEMA);
    write(root, "schema/snap.md", SNAP_MD);

    let host = ObjectId::new(KIND_HOST, "local");
    let host_val = json!({ "hostname": "oath", "power": "run" });
    write_object(root, &host, "mutate", &host_val, &host_val)?;

    let serial = ObjectId::new(KIND_SVC, "serial");
    let serial_desired = json!({
        "exec": ["/usr/lib/oath/serial-login"],
        "wants": [],
        "restart": "always",
        "enabled": true
    });
    let serial_actual = json!({
        "state": "stopped",
        "pid": null,
        "restarts": 0
    });
    write_object(root, &serial, "mutate", &serial_desired, &serial_actual)?;
    write_json(&root.join("objects/svc/serial/applied.json"), &serial_desired)?;

    let cur = ObjectId::new(KIND_SNAP, "current");
    let gen0 = json!({ "generation": 0 });
    write_object(root, &cur, "mutate", &gen0, &gen0)?;

    std::fs::write(root.join("log/apply.jsonl"), "")?;

    let cat = crate::Catalog::open(root)?;
    cat.write_index()?;
    Ok(())
}

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let mut b = body.to_string();
    if !b.ends_with('\n') {
        b.push('\n');
    }
    let _ = std::fs::write(p, b);
}

fn write_object(
    root: &Path,
    id: &ObjectId,
    safety: &str,
    desired: &serde_json::Value,
    actual: &serde_json::Value,
) -> Result<()> {
    let dir = root.join("objects").join(&id.kind).join(&id.name);
    std::fs::create_dir_all(&dir)?;
    write_json(&dir.join("desired.json"), desired)?;
    write_json(&dir.join("actual.json"), actual)?;
    write_json(&dir.join("meta.json"), &Meta::new(&id.kind, &id.name, safety))?;
    Ok(())
}
