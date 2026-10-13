use std::sync::Mutex;

use oath_core::{
    converge_pkg, hash_tree, ingest_file, install_tree, is_realization_id, promote_slot, seed,
    Actor, ApplyHooks, Catalog, Error, Host, HostPower, NullHooks, ObjectId, Pkg, PkgActual,
    Result, EXIT_CONFIRM,
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
            timezone: desired.timezone.clone(),
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
                hash: desired.hash.clone(),
                realizations: Vec::new(),
                needs: desired.needs.clone(),
                description: desired.description.clone(),
                home: desired.home.clone(),
                components: Vec::new(),
                advisories: None,
            });
        }
        converge_pkg(&self.root, &self.root.join("bin"), &id.name, desired.present, &desired.hash)
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
    let ids: Vec<String> = cat.ls(None).unwrap().iter().map(|i| i.to_string()).collect();
    for want in [
        "host:local",
        "svc:serial",
        "svc:sshd",
        "svc:hold",
        "pkg:busybox",
        "pkg:btrfs",
        "pkg:oath",
        "pkg:dropbear",
        "pkg:curl",
        "pkg:hello",
        "pkg:fetchme",
        "net:net0",
        "ssh:local",
        "dev:vda",
        "dev:net0",
        "dev:ttyS0",
        "snap:current",
    ] {
        assert!(ids.iter().any(|i| i == want), "missing {want}");
    }
    assert_eq!(ids.len(), 17, "{ids:?}");
    let idx = cat.index_text().unwrap();
    assert!(idx.contains("You are on **Oath**"));
    assert!(idx.contains("`pkg`"));
    let host = cat.get(&"host:local".parse().unwrap()).unwrap();
    assert_eq!(host.desired, json!({"hostname": "oath", "power": "run"}));
    let net = cat.get(&"net:net0".parse().unwrap()).unwrap();
    assert_eq!(net.desired["ipv4"], "dhcp");
    let serial = cat.get(&"svc:serial".parse().unwrap()).unwrap();
    assert_eq!(serial.desired["exec"][0], "/lib/oath/serial-login");
    let sshd = cat.get(&"svc:sshd".parse().unwrap()).unwrap();
    let exec = sshd.desired["exec"].as_array().unwrap();
    assert!(exec.iter().any(|v| v == "-w"));
    assert!(!exec.iter().any(|v| v.as_str().unwrap_or("").contains("/root/.ssh")));
    let oath = cat.get(&"pkg:oath".parse().unwrap()).unwrap();
    assert_eq!(oath.actual["removable"], false);
    let curl = cat.get(&"pkg:curl".parse().unwrap()).unwrap();
    assert_eq!(curl.actual["removable"], true);
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

#[test]
fn pack_hash_in_path_and_pin() {
    let (d, cat) = tmp();
    write_hello_store(d.path());
    let hash = promote_slot(d.path(), "hello", None).unwrap();
    assert!(is_realization_id(&hash), "{hash}");
    assert!(d.path().join("store/pkg/hello").join(&hash).join("bin/hello").is_file());
    assert!(d
        .path()
        .join("store/pkg/hello/live")
        .symlink_metadata()
        .unwrap()
        .file_type()
        .is_symlink());
    let obj = cat.get(&"pkg:hello".parse().unwrap()).unwrap();
    assert_eq!(obj.desired["hash"], json!(hash));

    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "pkg:hello".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("present".into(), json!(true));
    cat.set_fields(&id, fields).unwrap();
    cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap();
    let target = std::fs::read_link(d.path().join("bin/hello")).unwrap();
    assert!(target.ends_with(format!("store/pkg/hello/{hash}/bin/hello")), "{target:?}");
    let obj = cat.get(&id).unwrap();
    assert_eq!(obj.actual["hash"], json!(hash));
    assert_eq!(obj.actual["present"], json!(true));
}

#[test]
fn pack_second_tree_stays_unlinked() {
    let (d, cat) = tmp();
    write_hello_store(d.path());
    let h1 = promote_slot(d.path(), "hello", None).unwrap();
    let other = d.path().join("hello-wip");
    std::fs::create_dir_all(other.join("bin")).unwrap();
    std::fs::write(other.join("bin/hello"), "other\n").unwrap();
    let h2 = install_tree(d.path(), "hello", &other, None, "").unwrap();
    assert_ne!(h1, h2);
    assert!(d.path().join("store/pkg/hello").join(&h1).join("bin/hello").is_file());
    assert!(d.path().join("store/pkg/hello").join(&h2).join("bin/hello").is_file());

    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "pkg:hello".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("present".into(), json!(true));
    fields.insert("hash".into(), json!(h1));
    cat.set_fields(&id, fields).unwrap();
    cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap();
    let target = std::fs::read_link(d.path().join("bin/hello")).unwrap();
    assert!(target.to_string_lossy().contains(&h1), "{target:?}");
    assert!(!target.to_string_lossy().contains(&h2), "{target:?}");

    let mut fields = Map::new();
    fields.insert("hash".into(), json!(h2));
    cat.set_fields(&id, fields).unwrap();
    cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap();
    let target = std::fs::read_link(d.path().join("bin/hello")).unwrap();
    assert!(target.to_string_lossy().contains(&h2), "{target:?}");
    assert!(d.path().join("store/pkg/hello").join(&h1).join("bin/hello").is_file());
}

#[test]
fn pack_hash_mismatch_refuses() {
    let (d, _cat) = tmp();
    write_hello_store(d.path());
    let tree = d.path().join("store/pkg/hello");
    let err = install_tree(
        d.path(),
        "hello",
        &tree,
        None,
        "sha256-0000000000000000000000000000000000000000000000000000000000000000",
    )
    .unwrap_err();
    match err {
        Error::Hint { message, .. } => assert!(message.contains("does not match"), "{message}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn pack_components_and_advisories_on_actual() {
    let (d, cat) = tmp();
    let stage = d.path().join("stage-hello");
    std::fs::create_dir_all(stage.join("bin")).unwrap();
    std::fs::write(stage.join("bin/hello"), b"hello\n").unwrap();
    std::fs::write(
        stage.join("components.json"),
        "{\n  \"components\": [\n    {\n      \"name\": \"hello\",\n      \"version\": \"1.0\",\n      \"purl\": \"pkg:generic/hello@1.0\"\n    }\n  ]\n}\n",
    )
    .unwrap();
    let h = install_tree(d.path(), "hello", &stage, None, "").unwrap();
    let adv = d.path().join("store/adv/hello").join(format!("{h}.json"));
    std::fs::create_dir_all(adv.parent().unwrap()).unwrap();
    std::fs::write(
        &adv,
        format!(
            "{{\n  \"hash\": \"{h}\",\n  \"pkg\": \"pkg:hello\",\n  \"advisories\": [\n    {{\n      \"id\": \"CVE-TEST-1\",\n      \"url\": \"https://example.invalid/CVE-TEST-1\"\n    }}\n  ]\n}}\n"
        ),
    )
    .unwrap();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "pkg:hello".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("present".into(), json!(true));
    fields.insert("hash".into(), json!(h.clone()));
    cat.set_fields(&id, fields).unwrap();
    cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap();
    let obj = cat.get(&id).unwrap();
    let comps = obj.actual["components"].as_array().unwrap();
    assert_eq!(comps[0]["name"], "hello");
    assert_eq!(comps[0]["version"], "1.0");
    assert_eq!(comps[0]["purl"], "pkg:generic/hello@1.0");
    let advs = obj.actual["advisories"].as_array().unwrap();
    assert_eq!(advs[0]["id"], "CVE-TEST-1");
}

#[test]
fn seed_pkg_has_description_and_home() {
    let (_d, cat) = tmp();
    let obj = cat.get(&"pkg:curl".parse().unwrap()).unwrap();
    let desc = obj.desired["description"].as_str().unwrap();
    let home = obj.desired["home"].as_str().unwrap();
    assert!(desc.contains("curl"), "{desc}");
    assert!(home.starts_with("https://"), "{home}");
}

#[test]
fn ingest_file_reports_hash() {
    let (d, _cat) = tmp();
    let h = ingest_file(d.path(), "fetchme", b"fetched\n", None, "").unwrap();
    assert!(is_realization_id(&h), "{h}");
    assert_eq!(hash_tree(&d.path().join("store/pkg/fetchme").join(&h)).unwrap(), h);
    assert!(d.path().join("store/pkg/fetchme").join(&h).join("bin/fetchme").is_file());
}

/// Seed plus `pkg:libfoo` and `pkg:app`, where app needs libfoo.
fn tmp_with_needs() -> (tempfile::TempDir, Catalog) {
    let (d, cat) = tmp();
    let h = format!("sha256-{}", "a".repeat(64));
    oath_core::apply_overlay(
        d.path(),
        &json!({"objects": {
            "pkg:libfoo": {"present": true, "hash": h},
            "pkg:app": {"present": true, "hash": h, "needs": [{"id": "pkg:libfoo", "hash": h}]}
        }}),
    )
    .unwrap();
    (d, cat)
}

#[test]
fn pkg_needs_refuses_need_off_while_user_present() {
    let (d, cat) = tmp_with_needs();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let lib: ObjectId = "pkg:libfoo".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("present".into(), json!(false));
    cat.set_fields(&lib, fields).unwrap();
    let err = cat.apply(Some(vec![lib.clone()]), false, &Actor::unknown(), &hooks).unwrap_err();
    match err {
        Error::Hint { message, hint } => {
            assert!(message.contains("needed by"), "{message}");
            assert!(hint.contains("app") || hint.contains("present=false"), "{hint}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(cat.get(&lib).unwrap().actual["present"], json!(true));
}

#[test]
fn pkg_needs_allows_need_off_after_user() {
    let (d, cat) = tmp_with_needs();
    let hooks = MemHooks::new(d.path().to_path_buf());
    for name in ["app", "libfoo"] {
        let id: ObjectId = format!("pkg:{name}").parse().unwrap();
        let mut fields = Map::new();
        fields.insert("present".into(), json!(false));
        cat.set_fields(&id, fields).unwrap();
        cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap();
        assert_eq!(cat.get(&id).unwrap().actual["present"], json!(false));
    }
}
