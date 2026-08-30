use std::path::Path;

use serde_json::json;

use crate::kinds::Meta;
use crate::{
    write_json, ObjectId, Result, KIND_DEV, KIND_HOST, KIND_NET, KIND_PKG, KIND_SNAP, KIND_SSH,
    KIND_SVC,
};

pub const HOST_SCHEMA: &str = include_str!("../schema/host.json");
pub const HOST_MD: &str = include_str!("../schema/host.md");
pub const SVC_SCHEMA: &str = include_str!("../schema/svc.json");
pub const SVC_MD: &str = include_str!("../schema/svc.md");
pub const SNAP_SCHEMA: &str = include_str!("../schema/snap.json");
pub const SNAP_MD: &str = include_str!("../schema/snap.md");
pub const PKG_SCHEMA: &str = include_str!("../schema/pkg.json");
pub const PKG_MD: &str = include_str!("../schema/pkg.md");
pub const NET_SCHEMA: &str = include_str!("../schema/net.json");
pub const NET_MD: &str = include_str!("../schema/net.md");
pub const SSH_SCHEMA: &str = include_str!("../schema/ssh.json");
pub const SSH_MD: &str = include_str!("../schema/ssh.md");
pub const DEV_SCHEMA: &str = include_str!("../schema/dev.json");
pub const DEV_MD: &str = include_str!("../schema/dev.md");

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
    write(root, "schema/pkg.json", PKG_SCHEMA);
    write(root, "schema/pkg.md", PKG_MD);
    write(root, "schema/net.json", NET_SCHEMA);
    write(root, "schema/net.md", NET_MD);
    write(root, "schema/ssh.json", SSH_SCHEMA);
    write(root, "schema/ssh.md", SSH_MD);
    write(root, "schema/dev.json", DEV_SCHEMA);
    write(root, "schema/dev.md", DEV_MD);

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

    let hold = ObjectId::new(KIND_SVC, "hold");
    let hold_desired = json!({
        "exec": ["/bin/sleep", "86400000"],
        "wants": ["svc:serial"],
        "restart": "always",
        "enabled": true
    });
    let hold_actual = json!({
        "state": "stopped",
        "pid": null,
        "restarts": 0
    });
    write_object(root, &hold, "mutate", &hold_desired, &hold_actual)?;
    write_json(&root.join("objects/svc/hold/applied.json"), &hold_desired)?;

    seed_pkg(root, "hello", false, true)?;
    seed_pkg(root, "busybox", true, false)?;
    seed_pkg(root, "btrfs", true, false)?;
    seed_pkg(root, "oath", true, false)?;
    seed_pkg(root, "dropbear", true, false)?;
    seed_pkg(root, "glibc", true, false)?;
    seed_pkg(root, "river", true, true)?;
    write_object(
        root,
        &ObjectId::new(KIND_PKG, "fetchme"),
        "mutate",
        &json!({
            "present": false,
            "url": "http://10.0.2.2:18765/fetchme"
        }),
        &json!({
            "present": false,
            "links": [],
            "removable": true,
            "url": "http://10.0.2.2:18765/fetchme"
        }),
    )?;

    let sshd = ObjectId::new(KIND_SVC, "sshd");
    let sshd_desired = json!({
        "exec": ["/bin/dropbear", "-F", "-E", "-s", "-D", "/root/.ssh", "-r", "/oath/ssh/host_ed25519", "-p", "22"],
        "wants": [],
        "restart": "always",
        "enabled": true
    });
    let sshd_actual = json!({
        "state": "stopped",
        "pid": null,
        "restarts": 0
    });
    write_object(root, &sshd, "mutate", &sshd_desired, &sshd_actual)?;
    write_json(&root.join("objects/svc/sshd/applied.json"), &sshd_desired)?;

    let seatd = ObjectId::new(KIND_SVC, "seatd");
    let seatd_desired = json!({
        "exec": ["/bin/seatd"],
        "wants": [],
        "restart": "always",
        "enabled": true
    });
    let seatd_actual = json!({
        "state": "stopped",
        "pid": null,
        "restarts": 0
    });
    write_object(root, &seatd, "mutate", &seatd_desired, &seatd_actual)?;
    write_json(&root.join("objects/svc/seatd/applied.json"), &seatd_desired)?;

    let river = ObjectId::new(KIND_SVC, "river");
    let river_desired = json!({
        "exec": ["/bin/river"],
        "wants": ["svc:seatd"],
        "restart": "always",
        "enabled": true
    });
    let river_actual = json!({
        "state": "stopped",
        "pid": null,
        "restarts": 0
    });
    write_object(root, &river, "mutate", &river_desired, &river_actual)?;
    write_json(&root.join("objects/svc/river/applied.json"), &river_desired)?;

    let ssh = ObjectId::new(KIND_SSH, "local");
    let ssh_desired = json!({ "authorized": [] });
    let ssh_actual = json!({ "authorized": [], "host_key": false });
    write_object(root, &ssh, "mutate", &ssh_desired, &ssh_actual)?;

    let net = ObjectId::new(KIND_NET, "net0");
    let net_val = serde_json::to_value(crate::net::appliance_desired())?;
    write_object(root, &net, "mutate", &net_val, &net_val)?;

    seed_dev(root, "vda", "block", "/dev/vda")?;
    seed_dev(root, "net0", "net", "/sys/class/net/net0")?;
    seed_dev(root, "ttyS0", "tty", "/dev/ttyS0")?;
    seed_dev(root, "card0", "drm", "/dev/dri/card0")?;

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

fn seed_dev(root: &Path, name: &str, class: &str, node: &str) -> Result<()> {
    let id = ObjectId::new(KIND_DEV, name);
    write_object(
        root,
        &id,
        "mutate",
        &json!({ "present": true }),
        &json!({ "present": true, "class": class, "node": node }),
    )
}

fn seed_pkg(root: &Path, name: &str, present: bool, removable: bool) -> Result<()> {
    let id = ObjectId::new(KIND_PKG, name);
    write_object(
        root,
        &id,
        "mutate",
        &json!({ "present": present }),
        &json!({ "present": present, "links": [], "removable": removable }),
    )
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
