use std::sync::Mutex;

use oath_core::{
    apply_overlay, converge_pkg, copy_objects, hash_tree, ingest_file, install_tree,
    is_realization_id, promote_slot, seed, Actor, ApplyHooks, Catalog, Error, Host, HostPower, NullHooks, ObjectId,
    Pkg, PkgActual, Result, EXIT_CONFIRM,
};
use serde_json::{json, Map};

/// Hooks for tests: snapshots are copies of `objects/` under a scratch
/// directory, the hostname lives in memory, packages link for real.
struct MemHooks {
    hostname: Mutex<String>,
    root: std::path::PathBuf,
    snaps: tempfile::TempDir,
    reboots: Mutex<u32>,
}

impl MemHooks {
    fn new(root: std::path::PathBuf) -> Self {
        Self {
            hostname: Mutex::new("oath".into()),
            root,
            snaps: tempfile::tempdir().unwrap(),
            reboots: Mutex::new(0),
        }
    }
}

impl ApplyHooks for MemHooks {
    fn snapshot(&self, generation: u64) -> Result<()> {
        copy_objects(&self.root, &self.snaps.path().join(generation.to_string()))
    }
    fn snapshot_catalog(&self, generation: u64) -> Result<std::path::PathBuf> {
        let p = self.snaps.path().join(generation.to_string());
        if p.is_dir() {
            Ok(p)
        } else {
            Err(Error::Msg(format!("no snap {generation}")))
        }
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

/// A seeded catalog plus what these tests act on. The product seed has no
/// fixtures; a test adds them the way a machine overlay would.
fn tmp() -> (tempfile::TempDir, Catalog) {
    let (d, cat) = seeded();
    let fixtures = json!({"objects": {
        "svc:hold": {"exec": ["/bin/sleep", "86400000"], "wants": ["svc:serial"],
                     "restart": "always", "enabled": true},
        "pkg:hello": {"present": false, "description": "Canary. /bin/hello prints hello."}
    }});
    apply_overlay(d.path(), &fixtures).unwrap();
    (d, cat)
}

fn seeded() -> (tempfile::TempDir, Catalog) {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().to_path_buf();
    seed(&root).unwrap();
    let cat = Catalog::open(&root).unwrap();
    (d, cat)
}

#[test]
fn seed_lists_host() {
    let (_d, cat) = seeded();
    let ids: Vec<String> = cat.ls(None).unwrap().iter().map(|i| i.to_string()).collect();
    for want in [
        "host:local",
        "svc:serial",
        "svc:sshd",
        "svc:time",
        "pkg:busybox",
        "pkg:btrfs",
        "pkg:oath",
        "pkg:dropbear",
        "pkg:openssh",
        "pkg:util-linux",
        "pkg:dosfstools",
        "pkg:curl",
        "net:net0",
        "ssh:local",
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

/// Undoing every apply puts the object back exactly as seeded: no field
/// the seed did not have (a long-lived machine checks `oath diff` after).
#[test]
fn pkg_undo_to_start_is_in_sync() {
    let (d, cat) = tmp();
    write_hello_store(d.path());
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "pkg:hello".parse().unwrap();
    let seeded = cat.get(&id).unwrap().desired;
    for present in [true, false] {
        let mut fields = Map::new();
        fields.insert("present".into(), json!(present));
        cat.set_fields(&id, fields).unwrap();
        cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap();
    }
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    let drift = cat.diff(Some(&id)).unwrap();
    assert!(drift.is_empty(), "{:?}", drift.iter().map(|d| &d.fields).collect::<Vec<_>>());
    let desired = cat.get(&id).unwrap().desired;
    assert_eq!(desired.get("url"), seeded.get("url"));
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
    let err = cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap_err();
    assert_eq!(err.exit_code(), EXIT_CONFIRM, "net changes can cut off SSH");
    cat.apply(Some(vec![id.clone()]), true, &Actor::unknown(), &hooks).unwrap();
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
    let id: ObjectId = "dev:net0".parse().unwrap();
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

fn set(cat: &Catalog, id: &str, k: &str, v: serde_json::Value) -> Result<()> {
    let mut m = Map::new();
    m.insert(k.into(), v);
    cat.set_fields(&id.parse().unwrap(), m)
}

fn results(cat: &Catalog) -> Vec<(String, Option<u64>)> {
    cat.log_lines()
        .unwrap()
        .iter()
        .map(|r| (r["result"].as_str().unwrap().to_string(), r["generation"].as_u64()))
        .collect()
}

#[test]
fn set_refuses_a_value_of_the_wrong_type() {
    let (_d, cat) = tmp();
    assert!(set(&cat, "svc:hold", "restart", json!("sometimes")).is_err());
    assert!(set(&cat, "svc:hold", "enabled", json!("yes")).is_err());
    assert!(set(&cat, "host:local", "hostname", json!("no spaces")).is_err());
    assert!(set(&cat, "net:net0", "ipv4", json!("10.0.0.300/24")).is_err());
    assert!(set(&cat, "pkg:hello", "hash", json!("abc")).is_err());
    set(&cat, "net:net0", "ipv4", json!("10.0.0.3/24")).unwrap();
    let hold = cat.get(&"svc:hold".parse().unwrap()).unwrap();
    assert_eq!(hold.desired["restart"], "always");
}

#[test]
fn refused_apply_changes_nothing_and_is_logged() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    set(&cat, "host:local", "hostname", json!("bad")).unwrap();
    set(&cat, "svc:serial", "wants", json!(["svc:hold"])).unwrap();
    let err = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap_err();
    assert!(err.to_string().contains("cycle"), "{err}");
    assert_eq!(*hooks.hostname.lock().unwrap(), "oath");
    assert_eq!(results(&cat), vec![("refused".into(), None)]);
    let snap = cat.get(&"snap:current".parse().unwrap()).unwrap();
    assert_eq!(snap.actual["generation"], 0);
}

#[test]
fn failed_apply_is_logged_and_undo_reverts_it_not_the_one_before() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    set(&cat, "host:local", "hostname", json!("good")).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    // Valid spec, fails while converging: the hello tree is not in the store.
    write_hello_store(d.path());
    set(&cat, "pkg:hello", "present", json!(true)).unwrap();
    set(&cat, "pkg:hello", "hash", json!(format!("sha256-{}", "0".repeat(64)))).unwrap();
    assert!(cat.apply(None, false, &Actor::unknown(), &hooks).is_err());
    assert_eq!(results(&cat), vec![("ok".into(), Some(1)), ("error".into(), Some(2))]);
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert_eq!(*hooks.hostname.lock().unwrap(), "good");
    let hello = cat.get(&"pkg:hello".parse().unwrap()).unwrap();
    assert_eq!(hello.desired["present"], false);
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert_eq!(*hooks.hostname.lock().unwrap(), "oath");
    assert!(cat.undo(&Actor::unknown(), &hooks).is_err(), "nothing left to undo");
    // The log keeps every apply and every undo.
    let r = results(&cat);
    assert_eq!(r.len(), 4);
    assert_eq!(r[0], ("ok".into(), Some(1)));
}

#[test]
fn undo_restores_spec_not_log_or_other_objects() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    set(&cat, "host:local", "hostname", json!("one")).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    // A pending edit on an object the undone apply did not touch survives.
    set(&cat, "svc:hold", "restart", json!("never")).unwrap();
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    let hold = cat.get(&"svc:hold".parse().unwrap()).unwrap();
    assert_eq!(hold.desired["restart"], "never");
    let host = cat.get(&"host:local".parse().unwrap()).unwrap();
    assert_eq!(host.desired["hostname"], "oath");
    assert_eq!(cat.log_lines().unwrap().len(), 2);
}

#[test]
fn new_and_rm_go_through_apply_and_undo() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let tick: ObjectId = "svc:tick".parse().unwrap();
    let spec =
        json!({"exec": ["/bin/sleep", "9"], "wants": [], "restart": "always", "enabled": true});
    assert!(cat.new_object(&"host:other".parse().unwrap(), Map::new()).is_err());
    assert!(cat.new_object(&tick, json!({"exec": []}).as_object().unwrap().clone()).is_err());
    cat.new_object(&tick, spec.as_object().unwrap().clone()).unwrap();
    assert!(cat.new_object(&tick, spec.as_object().unwrap().clone()).is_err(), "exists");
    assert_eq!(cat.get(&tick).unwrap().meta.status, "new");
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&tick).unwrap().meta.status, "in-sync");

    // Undo of the apply that created it takes it out again.
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert!(cat.get(&tick).is_err());

    // rm of something applied waits for apply; undo brings it back.
    cat.new_object(&tick, spec.as_object().unwrap().clone()).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    cat.remove_object(&tick).unwrap();
    assert_eq!(cat.get(&tick).unwrap().meta.status, "removing");
    assert!(set(&cat, "svc:tick", "enabled", json!(false)).is_err());
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert!(cat.get(&tick).is_err());
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&tick).unwrap().desired["exec"][1], "9");
    assert_eq!(cat.get(&tick).unwrap().meta.status, "in-sync");

    // rm of something never applied is immediate; base packages refuse.
    let late: ObjectId = "svc:late".parse().unwrap();
    cat.new_object(&late, spec.as_object().unwrap().clone()).unwrap();
    cat.remove_object(&late).unwrap();
    assert!(cat.get(&late).is_err());
    assert!(cat.remove_object(&"pkg:busybox".parse().unwrap()).is_err());
    assert!(cat.remove_object(&"host:local".parse().unwrap()).is_err());
}

#[test]
fn lockout_changes_need_confirm() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    set(&cat, "svc:sshd", "enabled", json!(false)).unwrap();
    let err = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap_err();
    assert_eq!(err.exit_code(), EXIT_CONFIRM);
    set(&cat, "svc:sshd", "enabled", json!(true)).unwrap();
    set(&cat, "ssh:local", "authorized", json!([])).unwrap();
    assert!(cat.diff(None).unwrap().is_empty(), "authorized was already empty");
    set(&cat, "ssh:local", "authorized", json!(["ssh-ed25519 AAAA one"])).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    set(&cat, "ssh:local", "authorized", json!([])).unwrap();
    let err = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap_err();
    assert_eq!(err.exit_code(), EXIT_CONFIRM);
}

#[test]
fn the_lock_excludes_a_second_writer() {
    let (_d, cat) = tmp();
    let held = cat.lock().unwrap();
    let other = cat.clone();
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = done.clone();
    let t = std::thread::spawn(move || {
        set(&other, "host:local", "hostname", json!("waited")).unwrap();
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(!done.load(std::sync::atomic::Ordering::SeqCst), "set ran while the lock was held");
    drop(held);
    t.join().unwrap();
    assert_eq!(cat.get(&"host:local".parse().unwrap()).unwrap().desired["hostname"], "waited");
}

#[test]
fn ids_cannot_leave_the_catalog() {
    assert!("svc:../../etc".parse::<ObjectId>().is_err());
}
