use std::sync::Mutex;

use oath_core::{
    seed, Actor, ApplyHooks, Catalog, Error, Host, HostPower, NullHooks, ObjectId, Result,
    EXIT_CONFIRM,
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
        Ok(Host { hostname: desired.hostname.clone(), power: HostPower::Run })
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
    assert!(cat.index_text().unwrap().contains("You are on **Oath**"));
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
