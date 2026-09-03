use std::sync::Mutex;

use oath_core::{
    converge_pkg, seed, Actor, ApplyHooks, Catalog, Error, Host, HostPower, NullHooks, ObjectId,
    Pkg, PkgActual, Result, EXIT_CONFIRM,
};
use serde_json::{json, Map};

struct MemHooks {
    hostname: Mutex<String>,
    snaps: Mutex<Vec<(u64, Vec<u8>)>>,
    root: std::path::PathBuf,
    reboots: Mutex<u32>,
}

impl MemHooks {
    fn new(root: std::path::PathBuf) -> Self {
        Self {
            hostname: Mutex::new("oath".into()),
            snaps: Mutex::new(Vec::new()),
            root,
            reboots: Mutex::new(0),
        }
    }

    fn tar(root: &std::path::Path) -> Vec<u8> {
        let mut buf = Vec::new();
        for e in walkdir(root) {
            let rel = e.strip_prefix(root).unwrap();
            if rel.as_os_str().is_empty() {
                continue;
            }
            if rel.components().next().is_some_and(|c| c.as_os_str() == "bin") {
                continue;
            }
            if e.is_file() {
                buf.extend_from_slice(rel.to_string_lossy().as_bytes());
                buf.push(0);
                let data = std::fs::read(&e).unwrap();
                buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
                buf.extend_from_slice(&data);
            }
        }
        buf
    }

    fn untar(root: &std::path::Path, buf: &[u8]) {
        let _ = std::fs::remove_dir_all(root.join("objects"));
        let _ = std::fs::remove_dir_all(root.join("log"));
        let mut i = 0;
        while i < buf.len() {
            let z = buf[i..].iter().position(|b| *b == 0).unwrap();
            let rel = std::str::from_utf8(&buf[i..i + z]).unwrap();
            i += z + 1;
            let len = u32::from_le_bytes(buf[i..i + 4].try_into().unwrap()) as usize;
            i += 4;
            let data = &buf[i..i + len];
            i += len;
            let p = root.join(rel);
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d).unwrap();
            }
            std::fs::write(p, data).unwrap();
        }
    }
}

fn walkdir(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    fn rec(p: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    rec(&p, out);
                } else {
                    out.push(p);
                }
            }
        }
    }
    rec(root, &mut out);
    out
}

impl ApplyHooks for MemHooks {
    fn snapshot(&self, generation: u64) -> Result<()> {
        self.snaps.lock().unwrap().push((generation, Self::tar(&self.root)));
        Ok(())
    }
    fn restore_snapshot(&self, generation: u64) -> Result<()> {
        let snaps = self.snaps.lock().unwrap();
        let buf = snaps
            .iter()
            .find(|(g, _)| *g == generation)
            .map(|(_, b)| b.clone())
            .ok_or_else(|| Error::Msg(format!("no snap {generation}")))?;
        drop(snaps);
        Self::untar(&self.root, &buf);
        Ok(())
    }
    fn converge_host(&self, desired: &Host) -> Result<Host> {
        *self.hostname.lock().unwrap() = desired.hostname.clone();
        Ok(Host {
            hostname: desired.hostname.clone(),
            power: HostPower::Run,
            env: desired.env.clone(),
        })
    }
    fn notify_init(&self) -> Result<()> {
        Ok(())
    }
    fn reboot(&self) -> Result<()> {
        *self.reboots.lock().unwrap() += 1;
        Ok(())
    }
    fn halt(&self) -> Result<()> {
        Ok(())
    }
    fn converge_pkg(&self, id: &ObjectId, desired: &Pkg) -> Result<PkgActual> {
        let store = self.root.join("store/pkg").join(&id.name).join("bin");
        if !store.is_dir() {
            return Ok(PkgActual {
                present: desired.present,
                links: Vec::new(),
                removable: true,
                url: desired.url.clone(),
            });
        }
        converge_pkg(&self.root, &self.root.join("bin"), &id.name, desired.present)
    }
}

fn tmp() -> (tempfile::TempDir, Catalog) {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().to_path_buf();
    seed(&root).unwrap();
    let cat = Catalog::open(&root).unwrap();
    (d, cat)
}

#[test]
fn seed_lists_host() {
    let (_d, cat) = tmp();
    let ids = cat.ls(None).unwrap();
    assert!(ids.iter().any(|i| i.to_string() == "host:local"));
    assert!(ids.iter().any(|i| i.to_string() == "svc:serial"));
    assert!(ids.iter().any(|i| i.to_string() == "svc:hold"));
    assert!(ids.iter().any(|i| i.to_string() == "pkg:hello"));
    assert!(ids.iter().any(|i| i.to_string() == "pkg:busybox"));
    assert!(ids.iter().any(|i| i.to_string() == "pkg:btrfs"));
    assert!(ids.iter().any(|i| i.to_string() == "pkg:oath"));
    assert!(ids.iter().any(|i| i.to_string() == "net:net0"));
    assert!(ids.iter().any(|i| i.to_string() == "ssh:local"));
    assert!(ids.iter().any(|i| i.to_string() == "svc:sshd"));
    assert!(ids.iter().any(|i| i.to_string() == "pkg:dropbear"));
    assert!(ids.iter().any(|i| i.to_string() == "pkg:glibc"));
    assert!(ids.iter().any(|i| i.to_string() == "pkg:river"));
    assert!(ids.iter().any(|i| i.to_string() == "svc:river"));
    assert!(ids.iter().any(|i| i.to_string() == "svc:seatd"));
    assert!(ids.iter().any(|i| i.to_string() == "pkg:sola"));
    assert!(ids.iter().any(|i| i.to_string() == "pkg:grok"));
    assert!(ids.iter().any(|i| i.to_string() == "svc:sola-bus"));
    assert!(ids.iter().any(|i| i.to_string() == "svc:sola-call"));
    assert!(ids.iter().any(|i| i.to_string() == "svc:sola-river"));
    assert!(ids.iter().any(|i| i.to_string() == "svc:sola-shell"));
    assert!(ids.iter().any(|i| i.to_string() == "svc:sola-session"));
    assert!(ids.iter().any(|i| i.to_string() == "dev:vda"));
    assert!(ids.iter().any(|i| i.to_string() == "dev:net0"));
    assert!(ids.iter().any(|i| i.to_string() == "dev:ttyS0"));
    assert!(ids.iter().any(|i| i.to_string() == "dev:card0"));
    assert!(ids.iter().any(|i| i.to_string() == "dev:kbd0"));
    assert!(ids.iter().any(|i| i.to_string() == "dev:mouse0"));
    let idx = cat.index_text().unwrap();
    assert!(idx.contains("You are on **Oath**"));
    assert!(idx.contains("`pkg`"));
    let host = cat.get(&"host:local".parse().unwrap()).unwrap();
    assert_eq!(host.desired["env"]["GROK_DISABLE_AUTOUPDATER"], "1");
    let serial = cat.get(&"svc:serial".parse().unwrap()).unwrap();
    assert_eq!(serial.desired["exec"][0], "/lib/oath/serial-login");
    let sshd = cat.get(&"svc:sshd".parse().unwrap()).unwrap();
    let exec = sshd.desired["exec"].as_array().unwrap();
    assert!(exec.iter().any(|v| v == "-w"));
    assert!(!exec.iter().any(|v| v.as_str().unwrap_or("").contains("/root/.ssh")));
    let seatd = cat.get(&"svc:seatd".parse().unwrap()).unwrap();
    let exec = seatd.desired["exec"].as_array().unwrap();
    assert!(exec.iter().any(|v| v == "-u"));
    assert!(exec.iter().any(|v| v == "-g"));
    assert!(exec.iter().any(|v| v == "home"));
}

fn write_hello_store(root: &std::path::Path) {
    let p = root.join("store/pkg/hello/bin/hello");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "hello-payload\n").unwrap();
}

#[test]
fn set_diff_apply_hostname() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "host:local".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("hostname".into(), json!("atlas"));
    cat.set_fields(&id, fields).unwrap();
    let drift = cat.diff(Some(&id)).unwrap();
    assert_eq!(drift.len(), 1);
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(*hooks.hostname.lock().unwrap(), "atlas");
    let obj = cat.get(&id).unwrap();
    assert_eq!(obj.actual["hostname"], "atlas");
    assert!(cat.diff(Some(&id)).unwrap().is_empty());
}

#[test]
fn reboot_needs_confirm() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "host:local".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("power".into(), json!("reboot"));
    cat.set_fields(&id, fields).unwrap();
    let err = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap_err();
    assert_eq!(err.exit_code(), EXIT_CONFIRM);
    cat.apply(None, true, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(*hooks.reboots.lock().unwrap(), 1);
    let obj = cat.get(&id).unwrap();
    assert_eq!(obj.desired["power"], "run");
}

#[test]
fn undo_restores_hostname() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "host:local".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("hostname".into(), json!("atlas"));
    cat.set_fields(&id, fields).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert_eq!(*hooks.hostname.lock().unwrap(), "oath");
}

#[test]
fn unknown_field_hints_schema() {
    let (_d, cat) = tmp();
    let id: ObjectId = "host:local".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("nope".into(), json!("x"));
    let err = cat.set_fields(&id, fields).unwrap_err();
    match err {
        Error::Hint { hint, .. } => assert!(hint.contains("schema")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn second_apply_after_undo_uses_new_generation() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "host:local".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("hostname".into(), json!("atlas"));
    cat.set_fields(&id, fields).unwrap();
    let r1 = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    let mut fields = Map::new();
    fields.insert("hostname".into(), json!("beta"));
    cat.set_fields(&id, fields).unwrap();
    let r2 = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert!(r2.generation > r1.generation, "{} vs {}", r2.generation, r1.generation);
}

#[test]
fn svc_undo_restores_enabled() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "svc:hold".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("enabled".into(), json!(false));
    cat.set_fields(&id, fields).unwrap();
    cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap();
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    let obj = cat.get(&id).unwrap();
    assert_eq!(obj.desired["enabled"], json!(true));
}

#[test]
fn apply_noop_on_in_sync() {
    let (_d, cat) = tmp();
    let r = cat.apply(None, false, &Actor::unknown(), &NullHooks).unwrap();
    assert!(r.ids.is_empty());
}

#[test]
fn pkg_present_links_and_undo() {
    let (d, cat) = tmp();
    write_hello_store(d.path());
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "pkg:hello".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("present".into(), json!(true));
    cat.set_fields(&id, fields).unwrap();
    cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap();
    let link = d.path().join("bin/hello");
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    let target = std::fs::read_link(&link).unwrap();
    assert!(target.ends_with("store/pkg/hello/bin/hello"), "{target:?}");
    let obj = cat.get(&id).unwrap();
    assert_eq!(obj.actual["present"], json!(true));
    assert_eq!(obj.actual["links"], json!(["hello"]));

    let mut fields = Map::new();
    fields.insert("present".into(), json!(false));
    cat.set_fields(&id, fields).unwrap();
    cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap();
    assert!(link.symlink_metadata().is_err());
    assert!(d.path().join("store/pkg/hello/bin/hello").is_file());
    let obj = cat.get(&id).unwrap();
    assert_eq!(obj.actual["present"], json!(false));

    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    let obj = cat.get(&id).unwrap();
    assert_eq!(obj.desired["present"], json!(true));
    assert_eq!(obj.actual["present"], json!(true));
}

#[test]
fn pkg_collision_refuses() {
    let (d, cat) = tmp();
    write_hello_store(d.path());
    std::fs::create_dir_all(d.path().join("bin")).unwrap();
    std::fs::write(d.path().join("bin/hello"), "busybox-or-other\n").unwrap();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "pkg:hello".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("present".into(), json!(true));
    cat.set_fields(&id, fields).unwrap();
    let err = cat.apply(Some(vec![id]), false, &Actor::unknown(), &hooks).unwrap_err();
    match err {
        Error::Hint { message, hint } => {
            assert!(message.contains("exists"), "{message}");
            assert!(hint.contains("schema"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(std::fs::read_to_string(d.path().join("bin/hello")).unwrap(), "busybox-or-other\n");
}

#[test]
fn pkg_not_removable_refuses_absent() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "pkg:busybox".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("present".into(), json!(false));
    let err = cat.set_fields(&id, fields).unwrap_err();
    match err {
        Error::Hint { message, .. } => assert!(message.contains("not removable"), "{message}"),
        other => panic!("{other:?}"),
    }
    let obj = cat.get(&id).unwrap();
    assert_eq!(obj.desired["present"], json!(true));

    write_json_present_false(d.path(), "busybox");
    let err = cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap_err();
    match err {
        Error::Hint { message, .. } => assert!(message.contains("not removable"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn net_up_down_undo() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "net:net0".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("up".into(), json!(false));
    cat.set_fields(&id, fields).unwrap();
    cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&id).unwrap().actual["up"], json!(false));
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&id).unwrap().desired["up"], json!(true));
    assert_eq!(cat.get(&id).unwrap().actual["up"], json!(true));
}

#[test]
fn svc_wants_cycle_refuses_apply() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let hold: ObjectId = "svc:hold".parse().unwrap();
    let sshd: ObjectId = "svc:sshd".parse().unwrap();
    let mut a = Map::new();
    a.insert("wants".into(), json!(["svc:sshd"]));
    cat.set_fields(&hold, a).unwrap();
    let mut b = Map::new();
    b.insert("wants".into(), json!(["svc:hold"]));
    cat.set_fields(&sshd, b).unwrap();
    let err = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap_err();
    match err {
        Error::Hint { message, .. } => assert!(message.contains("cycle"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn dev_not_removable() {
    let (_d, cat) = tmp();
    let id: ObjectId = "dev:vda".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("present".into(), json!(false));
    let err = cat.set_fields(&id, fields).unwrap_err();
    match err {
        Error::Hint { message, .. } => assert!(message.contains("not removable"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn ssh_authorized_undo() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "ssh:local".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("authorized".into(), json!(["ssh-ed25519 AAAATEST"]));
    cat.set_fields(&id, fields).unwrap();
    cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&id).unwrap().actual["authorized"], json!(["ssh-ed25519 AAAATEST"]));
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&id).unwrap().desired["authorized"], json!([]));
}

fn write_json_present_false(root: &std::path::Path, name: &str) {
    let p = root.join("objects/pkg").join(name).join("desired.json");
    std::fs::write(p, "{\n  \"present\": false\n}\n").unwrap();
}
