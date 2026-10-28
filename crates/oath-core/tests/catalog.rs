use std::sync::Mutex;

use oath_core::{
    apply_overlay, converge_pkg, copy_objects, hash_tree, ingest_file, install_tree,
    is_realization_id, promote_slot, seed, Actor, ApplyHooks, Catalog, Error, Host, HostPower,
    NullHooks, ObjectId, Pkg, PkgActual, Result, EXIT_CONFIRM,
};
use serde_json::{json, Map};

/// Hooks for tests: snapshots are copies of `objects/` under a scratch
/// directory, the hostname lives in memory, packages link for real.
struct MemHooks {
    hostname: Mutex<String>,
    root: std::path::PathBuf,
    snaps: tempfile::TempDir,
    reboots: Mutex<u32>,
    /// Services do not come up: PID 1 records them failed, and apply's
    /// wait for them is an error.
    no_start: Mutex<bool>,
}

impl MemHooks {
    fn new(root: std::path::PathBuf) -> Self {
        Self {
            hostname: Mutex::new("oath".into()),
            root,
            snaps: tempfile::tempdir().unwrap(),
            reboots: Mutex::new(0),
            no_start: Mutex::new(false),
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
    fn converge_host(&self, desired: &Host, _previous: Option<&Host>) -> Result<Host> {
        *self.hostname.lock().unwrap() = desired.hostname.clone();
        Ok(Host {
            hostname: desired.hostname.clone(),
            power: HostPower::Run,
            env: desired.env.clone(),
            timezone: desired.timezone.clone(),
            hosts: desired.hosts.clone(),
            origins: desired.origins.clone(),
            profile: desired.profile.clone(),
            publishers: desired.publishers.clone(),
            mounts: desired.mounts.clone(),
            module_options: desired.module_options.clone(),
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
    fn wait_converge(&self, id: &ObjectId, _enabled: bool) -> Result<()> {
        if !*self.no_start.lock().unwrap() {
            return Ok(());
        }
        let failed = json!({ "state": "failed", "reason": "cannot start /bin/slep" });
        oath_core::write_json(&oath_core::status_path(&self.root, id), &failed)?;
        Err(Error::hint(format!("{id} did not become running"), format!("oath get {id}")))
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
                verified: None,
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
        "pkg:bash",
        "pkg:glibc",
        "pkg:dhcpcd",
        "pkg:chrony",
        "pkg:btrfs",
        "pkg:oath",
        "pkg:libxcrypt",
        "pkg:openssh",
        "pkg:util-linux",
        "pkg:dosfstools",
        "pkg:curl",
        "pkg:zsh",
        "pkg:claude-code",
        "pkg:claude-code-src",
        "plan:claude-code",
        "pkg:codex",
        "pkg:codex-src",
        "plan:codex",
        "pkg:grok-build",
        "pkg:grok-build-src",
        "plan:grok-build",
        "net:net0",
        "ssh:local",
        "snap:current",
    ] {
        assert!(ids.iter().any(|i| i == want), "missing {want}");
    }
    // Devices are discovered on the machine, not seeded.
    assert!(!ids.iter().any(|i| i.starts_with("dev:")), "{ids:?}");
    // 5 svc and host, 39 packages, three recipes (two packages and a plan
    // each), net:net0, ssh:local, snap:current.
    assert_eq!(ids.len(), 56, "{ids:?}");
    let idx = cat.index_text().unwrap();
    assert!(idx.contains("You are on **Oath**"));
    assert!(idx.contains("`pkg`"));
    // The recipe (R23) is where an agent looks first.
    assert!(idx.contains("## Recipes"), "{idx}");
    for plan in ["claude-code", "codex", "grok-build"] {
        assert!(idx.contains(&format!("`sudo oath build plan:{plan}`")), "{idx}");
    }
    let host = cat.get(&"host:local".parse().unwrap()).unwrap();
    assert_eq!(
        host.desired,
        json!({
            "hostname": "oath",
            "power": "run",
            "origins": ["https://store.oath.moonlight.pm"],
            "profile": "base",
            "publishers": [
                "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIH+YKD3yXYsGJMxwpBIWSYP7VuQvjhzGG+LWvYhKTESD oath@buccina"
            ]
        })
    );
    let net = cat.get(&"net:net0".parse().unwrap()).unwrap();
    assert_eq!(net.desired["ipv4"], "dhcp");
    let serial = cat.get(&"svc:serial".parse().unwrap()).unwrap();
    assert_eq!(serial.desired["exec"][0], "/lib/oath/serial-login");
    let screen = cat.get(&"svc:screen".parse().unwrap()).unwrap();
    assert_eq!(screen.desired["exec"][0], "/lib/oath/screen-login");
    assert_eq!(screen.desired["user"], "home");
    let sshd = cat.get(&"svc:sshd".parse().unwrap()).unwrap();
    let exec = sshd.desired["exec"].as_array().unwrap();
    // OpenSSH (R36); sshd_config, not the argv, refuses root.
    assert_eq!(exec[0], "/bin/sshd");
    assert!(!exec.iter().any(|v| v.as_str().unwrap_or("").contains("/root/.ssh")));
    assert!(cat.get(&"pkg:openssh".parse().unwrap()).unwrap().meta.fixed, "sshd");
    let oath = cat.get(&"pkg:oath".parse().unwrap()).unwrap();
    assert!(oath.meta.fixed);
    assert!(cat.get(&"pkg:zsh".parse().unwrap()).unwrap().meta.fixed, "the login shell");
    let curl = cat.get(&"pkg:curl".parse().unwrap()).unwrap();
    assert!(!curl.meta.fixed);
    // Seeding writes spec; status is the machine's to observe.
    assert!(curl.actual.is_null());
    assert!(!cat.root().join("status").exists());
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
    let r = cat.apply(None, true, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(*hooks.reboots.lock().unwrap(), 1);
    let obj = cat.get(&id).unwrap();
    assert_eq!(obj.desired["power"], "run");
    // Power alone is not configuration: no generation, nothing to undo.
    assert!(r.power_only);
    assert_eq!(cat.get(&"snap:current".parse().unwrap()).unwrap().desired["generation"], 0);
    assert_eq!(results(&cat).last().unwrap(), &("ok".to_string(), None));
    assert!(cat.undo(&Actor::unknown(), &hooks).is_err(), "nothing to undo");
    // With a real change it is one generation like any other.
    set(&cat, "host:local", "hostname", json!("both")).unwrap();
    set(&cat, "host:local", "power", json!("reboot")).unwrap();
    let r = cat.apply(None, true, &Actor::unknown(), &hooks).unwrap();
    assert!(!r.power_only && r.generation == 1);
}

#[test]
fn the_log_names_the_tool_and_what_it_forced() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let agent = Actor { via: Some("mcp:claude-code".into()), ..Actor::unknown() };
    set(&cat, "host:local", "power", json!("halt")).unwrap();
    assert!(cat.apply(None, false, &agent, &hooks).is_err());
    set(&cat, "host:local", "power", json!("run")).unwrap();
    set(&cat, "host:local", "hostname", json!("atlas")).unwrap();
    cat.apply(None, false, &agent, &hooks).unwrap();
    sshd_env(&cat, "4");
    cat.apply(None, true, &agent, &hooks).unwrap();
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    let log = cat.log_lines().unwrap();
    let (refused, plain, forced, undo) = (&log[0], &log[1], &log[2], &log[3]);
    assert_eq!(refused["result"], "refused");
    assert_eq!(refused["actor"]["via"], "mcp:claude-code");
    assert!(refused.get("confirm").is_none());
    assert_eq!(plain["result"], "ok");
    assert!(plain.get("confirm").is_none());
    assert_eq!(forced["confirm"], true);
    assert_eq!(forced["actor"]["via"], "mcp:claude-code");
    assert!(undo["actor"].get("via").is_none(), "the owner ran undo");
    assert!(undo.get("confirm").is_none());
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
    std::fs::write(d.path().join("bin/hello"), "another-build\n").unwrap();
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
    assert_eq!(std::fs::read_to_string(d.path().join("bin/hello")).unwrap(), "another-build\n");
}

#[test]
fn pkg_not_removable_refuses_absent() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "pkg:bash".parse().unwrap();
    let mut fields = Map::new();
    fields.insert("present".into(), json!(false));
    let err = cat.set_fields(&id, fields).unwrap_err();
    match err {
        Error::Hint { message, .. } => assert!(message.contains("not removable"), "{message}"),
        other => panic!("{other:?}"),
    }
    let obj = cat.get(&id).unwrap();
    assert_eq!(obj.desired["present"], json!(true));

    write_json_present_false(d.path(), "bash");
    let err = cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap_err();
    match err {
        Error::Hint { message, .. } => assert!(message.contains("not removable"), "{message}"),
        other => panic!("{other:?}"),
    }
}

/// A package seeded fixed that this build's base no longer fixes (dropbear
/// after R36) can be removed: `meta.fixed` is what the seed said then.
#[test]
fn pkg_the_base_no_longer_fixes_is_removable() {
    let (d, cat) = tmp();
    let id: ObjectId = "pkg:curl".parse().unwrap();
    let meta_path = d.path().join("objects/pkg/curl/meta.json");
    let mut meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    meta["fixed"] = json!(true);
    oath_core::write_json(&meta_path, &meta).unwrap();
    assert!(cat.get(&id).unwrap().meta.fixed);
    let mut fields = Map::new();
    fields.insert("present".into(), json!(false));
    cat.set_fields(&id, fields).unwrap();
    assert_eq!(cat.get(&id).unwrap().desired["present"], json!(false));
}

/// A machine updated to an image whose base profile fixes a new package
/// (`pkg:zsh`, home's login shell) gets it through `oath new`: it is not
/// removable all the same, and undoing the apply that made it keeps it.
#[test]
fn base_fixed_package_made_by_new_stays() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "pkg:zsh".parse().unwrap();
    std::fs::remove_dir_all(d.path().join("objects/pkg/zsh")).unwrap();
    let mut fields = Map::new();
    fields.insert("present".into(), json!(true));
    cat.new_object(&id, fields).unwrap();
    cat.apply(Some(vec![id.clone()]), false, &Actor::unknown(), &hooks).unwrap();
    let obj = cat.get(&id).unwrap();
    assert!(!obj.meta.fixed);
    assert_eq!(obj.actual["removable"], json!(false));
    let err = cat.remove_object(&id).unwrap_err();
    assert!(err.to_string().contains("not removable"), "{err}");

    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&id).unwrap().desired["present"], json!(true));
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
    let found = vec![("net0".to_string(), oath_core::DevActual::default())];
    cat.write_devices(&cat.lock().unwrap(), &found).unwrap();
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

/// Seed plus `pkg:libfoo` and `pkg:app`, where app needs libfoo, both
/// applied present.
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
    let hooks = MemHooks::new(d.path().to_path_buf());
    let ids = vec!["pkg:libfoo".parse().unwrap(), "pkg:app".parse().unwrap()];
    cat.apply(Some(ids), false, &Actor::unknown(), &hooks).unwrap();
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
    assert_eq!(cat.applied(&lib).unwrap().unwrap()["present"], json!(true));
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
fn host_names_other_machines() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "host:local".parse().unwrap();
    for bad in [
        json!({"novus": "10.0.0.300"}),
        json!({"no vus": "10.0.0.5"}),
        json!({"localhost": "10.0.0.5"}),
        json!({"oath": "10.0.0.5"}),
        json!(["novus"]),
    ] {
        assert!(set(&cat, "host:local", "hosts", bad.clone()).is_err(), "{bad}");
    }
    set(&cat, "host:local", "hosts", json!({"novus": "10.0.0.5", "nas": "fd00::12"})).unwrap();
    assert_eq!(cat.diff(Some(&id)).unwrap().len(), 1);
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    let obj = cat.get(&id).unwrap();
    assert_eq!(obj.actual["hosts"]["novus"], "10.0.0.5");
    assert!(cat.diff(Some(&id)).unwrap().is_empty());
    set(&cat, "host:local", "hosts", json!({})).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&id).unwrap().actual["hosts"], json!({}));
    assert!(cat.diff(Some(&id)).unwrap().is_empty());
}

#[test]
fn host_mounts_name_a_filesystem_under_mnt_or_home() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "host:local".parse().unwrap();
    // A machine without mounts says nothing about them: no drift from an
    // empty map an older applied.json never had.
    assert!(cat.get(&id).unwrap().desired.get("mounts").is_none());
    for bad in [
        json!({"/etc/x": {"source": "LABEL=a"}}),
        json!({"/mnt": {"source": "LABEL=a"}}),
        json!({"/mnt/../etc": {"source": "LABEL=a"}}),
        json!({"/mnt/a": {"source": "/dev/sdb1"}}),
        json!({"/mnt/a": {"source": "LABEL="}}),
        json!({"/mnt/a": {"source": "LABEL=a", "options": "bind"}}),
        json!({"/mnt/a": {"source": "LABEL=a", "options": "ro;rm"}}),
        json!({"/mnt/a": {"options": "ro"}}),
        json!({"/mnt/a": "LABEL=a"}),
    ] {
        assert!(set(&cat, "host:local", "mounts", bad.clone()).is_err(), "{bad}");
    }
    let warehouse = json!({"/mnt/warehouse": {"source": "LABEL=warehouse", "options": "noatime,compress=zstd:1"}});
    set(&cat, "host:local", "mounts", warehouse.clone()).unwrap();
    assert_eq!(cat.diff(Some(&id)).unwrap().len(), 1);
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&id).unwrap().actual["mounts"], warehouse);
    assert!(cat.diff(Some(&id)).unwrap().is_empty());
    // A left-out `options` is written down as empty, so it is no drift.
    set(&cat, "host:local", "mounts", json!({"/home/games": {"source": "UUID=a8fa"}})).unwrap();
    assert_eq!(
        cat.get(&id).unwrap().desired["mounts"],
        json!({"/home/games": {"source": "UUID=a8fa", "options": ""}})
    );
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert!(cat.diff(Some(&id)).unwrap().is_empty());
    set(&cat, "host:local", "mounts", json!({})).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&id).unwrap().actual["mounts"], json!({}));
    assert!(cat.diff(Some(&id)).unwrap().is_empty());
}

#[test]
fn host_lists_origins_in_order() {
    let (_d, cat) = tmp();
    let id: ObjectId = "host:local".parse().unwrap();
    assert_eq!(
        cat.get(&id).unwrap().desired["origins"],
        json!(["https://store.oath.moonlight.pm"])
    );
    for bad in [
        json!(["ftp://store.example"]),
        json!(["https://"]),
        json!(["https://store example"]),
        json!(["https://store.example/pkg/hello/x.tar"]),
        json!(["https://store.example/?x"]),
        json!(["https://store.example", "https://store.example"]),
        json!("https://store.example"),
    ] {
        assert!(set(&cat, "host:local", "origins", bad.clone()).is_err(), "{bad}");
    }
    let two = json!(["http://10.0.0.5:8080/oath", "https://store.oath.moonlight.pm"]);
    set(&cat, "host:local", "origins", two.clone()).unwrap();
    assert_eq!(cat.get(&id).unwrap().desired["origins"], two);
    set(&cat, "host:local", "origins", json!([])).unwrap();
    assert_eq!(cat.get(&id).unwrap().desired["origins"], json!([]));
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
    assert_eq!(snap.desired["generation"], 0);
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
fn pkg_undo_puts_url_and_prose_back() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "pkg:hello".parse().unwrap();
    let before = cat.get(&id).unwrap().desired;
    set(&cat, "pkg:hello", "description", json!("changed")).unwrap();
    set(&cat, "pkg:hello", "url", json!("https://origin.example/")).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&id).unwrap().actual["url"], "https://origin.example/");
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&id).unwrap().desired, before);
    assert!(cat.diff(None).unwrap().is_empty());
}

#[test]
fn a_service_that_does_not_start_is_applied_and_an_error() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    *hooks.no_start.lock().unwrap() = true;
    let broken: ObjectId = "svc:broken".parse().unwrap();
    cat.new_object(&broken, json!({"exec": ["/bin/slep", "9"]}).as_object().unwrap().clone())
        .unwrap();
    assert!(cat.apply(None, false, &Actor::unknown(), &hooks).is_err());
    // Its spec is applied: not new, no drift, and PID 1's failure shows.
    let obj = cat.get(&broken).unwrap();
    assert_eq!(obj.meta.status, "error");
    assert_eq!(obj.actual["reason"], "cannot start /bin/slep");
    assert!(cat.diff(None).unwrap().is_empty());
    // Fixing it is an ordinary set and apply.
    *hooks.no_start.lock().unwrap() = false;
    set(&cat, "svc:broken", "exec", json!(["/bin/sleep", "9"])).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    // Undo takes back the fix, then the apply that created it.
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&broken).unwrap().desired["exec"][0], "/bin/slep");
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert!(cat.get(&broken).is_err());
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

    // A package made absent is in-sync from the start, but it is still new
    // until an apply takes it; undo of that apply removes it.
    let later: ObjectId = "pkg:later".parse().unwrap();
    cat.new_object(&later, json!({"present": false}).as_object().unwrap().clone()).unwrap();
    assert_eq!(cat.get(&later).unwrap().meta.status, "new");
    let r = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(r.ids, vec!["pkg:later"]);
    assert_eq!(cat.get(&later).unwrap().meta.status, "in-sync");
    assert!(cat.diff(None).unwrap().is_empty());
    set(&cat, "pkg:later", "present", json!(true)).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&later).unwrap().desired, json!({"present": false}));
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert!(cat.get(&later).is_err());

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
    assert!(cat.remove_object(&"pkg:bash".parse().unwrap()).is_err());
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
    // Adding a key is safe; dropping or replacing one can lock the owner out.
    set(&cat, "ssh:local", "authorized", json!(["ssh-ed25519 AAAA one", "ssh-ed25519 BBBB two"]))
        .unwrap();
    let r = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(r.probation, None);
    set(&cat, "ssh:local", "authorized", json!(["ssh-ed25519 AAAA renamed"])).unwrap();
    let err = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap_err();
    assert_eq!(err.exit_code(), EXIT_CONFIRM);
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

fn sshd_env(cat: &Catalog, v: &str) {
    set(cat, "svc:sshd", "env", json!({ "OATH_CHECK": v })).unwrap();
}

#[test]
fn lockout_apply_goes_on_probation_until_kept() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    sshd_env(&cat, "1");
    let r = cat.apply(None, true, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(r.probation, Some(oath_core::probation::DEFAULT_SECS));
    let p = oath_core::probation::read(d.path()).unwrap();
    assert_eq!((p.generation, p.ids.clone()), (r.generation, vec!["svc:sshd".to_string()]));
    assert!(cat.index_text().unwrap().contains("## Probation"));

    // Nothing else applies while it is open, and the refusal is logged.
    set(&cat, "host:local", "hostname", json!("later")).unwrap();
    let err = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap_err();
    assert!(err.to_string().contains("on probation"), "{err}");
    assert_eq!(results(&cat).last().unwrap(), &("refused".to_string(), None));

    assert_eq!(cat.keep(&Actor::unknown()).unwrap(), r.generation);
    assert!(oath_core::probation::read(d.path()).is_none());
    assert!(!cat.index_text().unwrap().contains("## Probation"));
    assert!(cat.keep(&Actor::unknown()).is_err(), "nothing left to keep");
    let ok = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(ok.probation, None);

    // A keep is not an apply: undo takes the hostname, then sshd.
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert_eq!(*hooks.hostname.lock().unwrap(), "oath");
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    let sshd = cat.get(&"svc:sshd".parse().unwrap()).unwrap();
    assert!(sshd.desired.get("env").is_none_or(|e| e.get("OATH_CHECK").is_none()));
}

#[test]
fn undo_takes_a_probation_back_at_once() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    set(&cat, "net:net0", "ipv4", json!("10.0.0.3/24")).unwrap();
    cat.apply(None, true, &Actor::unknown(), &hooks).unwrap();
    assert!(oath_core::probation::read(d.path()).is_some());
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    assert!(oath_core::probation::read(d.path()).is_none());
    assert_eq!(cat.get(&"net:net0".parse().unwrap()).unwrap().desired["ipv4"], "dhcp");
}

#[test]
fn an_expired_probation_is_undone_and_cannot_be_kept() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let actor = Actor::probation();
    assert!(cat.apply_with(None, true, 5, &Actor::unknown(), &hooks).is_err(), "under 10 s");
    sshd_env(&cat, "2");
    let r = cat.apply_with(None, true, 30, &Actor::unknown(), &hooks).unwrap();
    assert!(cat.undo_expired(&actor, &hooks).unwrap().is_none(), "still running");

    let mut p = oath_core::probation::read(d.path()).unwrap();
    p.applied_uptime -= 31.0;
    oath_core::write_json(&d.path().join(oath_core::probation::PROBATION_FILE), &p).unwrap();
    assert!(cat.keep(&Actor::unknown()).unwrap_err().to_string().contains("ran out"));
    let undone = cat.undo_expired(&actor, &hooks).unwrap().unwrap();
    assert_eq!(undone.ids, vec![format!("undo:{}", r.generation)]);
    assert!(oath_core::probation::read(d.path()).is_none());
    assert!(cat.undo_expired(&actor, &hooks).unwrap().is_none(), "only once");
    let last = cat.log_lines().unwrap().pop().unwrap();
    assert_eq!(last["actor"]["tty"], "probation");
}

#[test]
fn a_reboot_ends_a_probation() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    sshd_env(&cat, "3");
    cat.apply(None, true, &Actor::unknown(), &hooks).unwrap();
    let mut p = oath_core::probation::read(d.path()).unwrap();
    p.boot_id = "an-earlier-boot".into();
    oath_core::write_json(&d.path().join(oath_core::probation::PROBATION_FILE), &p).unwrap();
    let actor = Actor::probation();
    assert!(cat.undo_expired(&actor, &hooks).unwrap().is_some());
}

#[test]
fn a_damaged_store_tree_is_not_linked_and_verify_says_so() {
    let (d, cat) = tmp();
    write_hello_store(d.path());
    let hash = promote_slot(d.path(), "hello", None).unwrap();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let id: ObjectId = "pkg:hello".parse().unwrap();
    set(&cat, "pkg:hello", "present", json!(true)).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    assert_eq!(cat.get(&id).unwrap().actual["verified"]["ok"], true);
    let r = cat.verify(None).unwrap();
    assert!(r.iter().any(|v| v.id == id && v.verified.ok), "{r:?}");

    // Something writes into the stored tree behind the catalog's back.
    let file = d.path().join("store/pkg/hello").join(&hash).join("bin/hello");
    let mut perm = std::fs::metadata(&file).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    std::fs::set_permissions(&file, perm).unwrap();
    std::fs::write(&file, "tampered\n").unwrap();

    let r = cat.verify(Some(vec![id.clone()])).unwrap();
    assert!(!r[0].verified.ok && r[0].verified.got != hash, "{r:?}");
    let obj = cat.get(&id).unwrap();
    assert_eq!(obj.meta.status, "error");
    assert_eq!(obj.actual["verified"]["ok"], false);
    assert!(cat.index_text().unwrap().contains("## Store damage"));

    // Apply refuses to link it again.
    set(&cat, "pkg:hello", "present", json!(false)).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    set(&cat, "pkg:hello", "present", json!(true)).unwrap();
    let err = cat.apply(None, false, &Actor::unknown(), &hooks).unwrap_err();
    assert!(err.to_string().contains("store is damaged"), "{err}");
}

#[test]
fn new_fills_in_what_the_type_defaults() {
    let (_d, cat) = tmp();
    let short: ObjectId = "svc:short".parse().unwrap();
    cat.new_object(&short, json!({"exec": ["/bin/sleep", "9"]}).as_object().unwrap().clone())
        .unwrap();
    assert_eq!(
        cat.get(&short).unwrap().desired,
        json!({"exec": ["/bin/sleep", "9"], "wants": [], "restart": "always", "enabled": true})
    );
    let tool: ObjectId = "pkg:tool".parse().unwrap();
    cat.new_object(&tool, Map::new()).unwrap();
    assert_eq!(cat.get(&tool).unwrap().desired, json!({"present": true}));
    // The schema is the type's: every field it lists is one set accepts.
    let schema: serde_json::Value = serde_json::from_str(&cat.schema_json("svc").unwrap()).unwrap();
    for k in schema["properties"].as_object().unwrap().keys() {
        assert!(
            [
                "exec",
                "wants",
                "restart",
                "enabled",
                "user",
                "env",
                "bind_low_ports",
                "keep_children"
            ]
            .contains(&k.as_str()),
            "{k}"
        );
    }
    assert!(set(&cat, "svc:short", "keep_children", json!(true)).is_ok());
    assert!(set(&cat, "svc:short", "nonsense", json!(true)).is_err());
}

#[test]
fn discovered_devices_come_and_go() {
    let (_d, cat) = tmp();
    let disk = oath_core::DevActual {
        present: true,
        class: "block".into(),
        node: "/dev/vda".into(),
        ..Default::default()
    };
    let nic = oath_core::DevActual { present: true, class: "net".into(), ..Default::default() };
    let found = vec![("disk-pci-0000.00.04.0".to_string(), disk), ("net0".to_string(), nic)];
    let ch = cat.write_devices(&cat.lock().unwrap(), &found).unwrap();
    assert_eq!(ch, vec!["+dev:disk-pci-0000.00.04.0", "+dev:net0"]);
    let disk = cat.get(&"dev:disk-pci-0000.00.04.0".parse().unwrap()).unwrap();
    assert_eq!(disk.actual["node"], "/dev/vda");
    assert_eq!(disk.meta.status, "in-sync");
    assert!(cat.diff(None).unwrap().is_empty());
    assert!(cat.index_text().unwrap().contains("`dev`"));
    // Same hardware: nothing changes. The disk is pulled: its object goes.
    assert!(cat.write_devices(&cat.lock().unwrap(), &found).unwrap().is_empty());
    let ch = cat.write_devices(&cat.lock().unwrap(), &found[1..]).unwrap();
    assert_eq!(ch, vec!["-dev:disk-pci-0000.00.04.0"]);
    // try_lock does not wait for a holder.
    let held = cat.lock().unwrap();
    assert!(cat.try_lock().unwrap().is_none());
    drop(held);
    assert!(cat.try_lock().unwrap().is_some());
}

#[test]
fn drift_and_undo_read_only_the_spec() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    let host: ObjectId = "host:local".parse().unwrap();
    let dir = d.path().join("objects/host/local");
    set(&cat, "host:local", "hostname", json!("one")).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    let applied: serde_json::Value = oath_core::read_json(&dir.join("applied.json")).unwrap();
    assert_eq!(applied["hostname"], "one");
    // Whatever the observation says, drift is desired against applied.
    let mut actual = cat.get(&host).unwrap().actual;
    actual["hostname"] = json!("observed-elsewhere");
    oath_core::write_json(&dir.join("actual.json"), &actual).unwrap();
    assert!(cat.diff(None).unwrap().is_empty());
    cat.undo(&Actor::unknown(), &hooks).unwrap();
    let applied: serde_json::Value = oath_core::read_json(&dir.join("applied.json")).unwrap();
    assert_eq!(applied["hostname"], "oath");
    assert_eq!(*hooks.hostname.lock().unwrap(), "oath");

    // A catalog from before applied.json: actual held the applied spec.
    std::fs::remove_file(dir.join("applied.json")).unwrap();
    oath_core::write_json(&dir.join("actual.json"), &cat.get(&host).unwrap().desired).unwrap();
    assert!(cat.diff(None).unwrap().is_empty());
    set(&cat, "host:local", "hostname", json!("two")).unwrap();
    assert_eq!(cat.diff(None).unwrap()[0].fields[0].0, "hostname");
}

#[test]
fn status_lives_outside_the_object_directories() {
    let (d, cat) = tmp();
    let hooks = MemHooks::new(d.path().to_path_buf());
    set(&cat, "host:local", "hostname", json!("seen")).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    let host: ObjectId = "host:local".parse().unwrap();
    assert!(oath_core::status_path(d.path(), &host).is_file());
    assert_eq!(cat.get(&host).unwrap().actual["hostname"], "seen");
    for e in walk(&d.path().join("objects")) {
        assert!(!e.ends_with("actual.json"), "status in the spec tree: {}", e.display());
    }
    // A generation record is spec.
    let rec = cat.get(&"snap:1".parse().unwrap()).unwrap();
    assert_eq!(rec.desired["parent"], 0);
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.path().is_dir() {
            out.extend(walk(&e.path()));
        } else {
            out.push(e.path());
        }
    }
    out
}

#[test]
fn migrate_brings_an_old_catalog_to_the_split() {
    let (d, cat) = seeded();
    let obj = |kind: &str, name: &str| d.path().join("objects").join(kind).join(name);
    // How a catalog looked before R14: actual held the applied spec, a
    // fixed package said removable false, a record lived in actual.
    std::fs::remove_file(obj("host", "local").join("applied.json")).unwrap();
    oath_core::write_json(
        &obj("host", "local").join("actual.json"),
        &json!({"hostname": "old", "power": "run", "env": {}, "timezone": ""}),
    )
    .unwrap();
    let mut meta: serde_json::Value =
        oath_core::read_json(&obj("pkg", "bash").join("meta.json")).unwrap();
    meta.as_object_mut().unwrap().remove("fixed");
    oath_core::write_json(&obj("pkg", "bash").join("meta.json"), &meta).unwrap();
    oath_core::write_json(
        &obj("pkg", "bash").join("actual.json"),
        &json!({"present": true, "removable": false, "links": []}),
    )
    .unwrap();
    std::fs::create_dir_all(obj("snap", "7")).unwrap();
    oath_core::write_json(&obj("snap", "7").join("desired.json"), &json!({})).unwrap();
    oath_core::write_json(&obj("snap", "7").join("meta.json"), &meta_of("snap", "7")).unwrap();
    oath_core::write_json(
        &obj("snap", "7").join("actual.json"),
        &json!({"generation": 7, "parent": 6, "time": "2026-10-01T00:00:00Z", "reason": "apply x"}),
    )
    .unwrap();
    oath_core::write_json(&obj("svc", "serial").join("last.json"), &json!({"generation": 7}))
        .unwrap();

    let n = cat.migrate(&cat.lock().unwrap()).unwrap();
    assert_eq!(n, 4);
    let applied = cat.applied(&"host:local".parse().unwrap()).unwrap().unwrap();
    assert_eq!(applied["hostname"], "old");
    assert!(cat.get(&"pkg:bash".parse().unwrap()).unwrap().meta.fixed);
    assert_eq!(cat.get(&"snap:7".parse().unwrap()).unwrap().desired["parent"], 6);
    assert!(d.path().join("log/svc/serial.last.json").is_file());
    for e in walk(&d.path().join("objects")) {
        assert!(!e.ends_with("actual.json") && !e.ends_with("last.json"), "{}", e.display());
    }
    // The applied spec is what the old actual said, so host drifts to it.
    assert_eq!(cat.diff(None).unwrap().len(), 1);
    assert_eq!(cat.migrate(&cat.lock().unwrap()).unwrap(), 0, "once");
}

fn meta_of(kind: &str, name: &str) -> serde_json::Value {
    json!({"id": format!("{kind}:{name}"), "kind": kind, "name": name, "safety": "mutate", "status": "in-sync"})
}

#[test]
fn observe_rebuilds_package_status_from_the_store() {
    let (d, cat) = tmp();
    write_hello_store(d.path());
    let hash = promote_slot(d.path(), "hello", None).unwrap();
    let hooks = MemHooks::new(d.path().to_path_buf());
    set(&cat, "pkg:hello", "present", json!(true)).unwrap();
    cat.apply(None, false, &Actor::unknown(), &hooks).unwrap();
    // A boot: the status tmpfs is empty.
    std::fs::remove_dir_all(d.path().join("status")).unwrap();
    let hello: ObjectId = "pkg:hello".parse().unwrap();
    assert!(cat.get(&hello).unwrap().actual.is_null());
    cat.observe(&cat.lock().unwrap(), &d.path().join("bin")).unwrap();
    let a = cat.get(&hello).unwrap().actual;
    assert_eq!((a["present"].clone(), a["hash"].clone()), (json!(true), json!(hash)));
    assert_eq!(a["links"], json!(["hello"]));
    assert_eq!(a["removable"], true);
    assert_eq!(a["description"], "Canary. /bin/hello prints hello.");
    let bash = cat.get(&"pkg:bash".parse().unwrap()).unwrap().actual;
    assert_eq!((bash["present"].clone(), bash["removable"].clone()), (json!(false), json!(false)));
    let cur = cat.get(&"snap:current".parse().unwrap()).unwrap().actual;
    assert_eq!(cur["generation"], 1);
}

#[test]
fn a_package_links_its_libraries_into_lib() {
    // R17: /lib is the loader's link farm, made and removed with /bin.
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    let pack = |name: &str, lib: &str| {
        let stage = root.join(format!("stage-{name}"));
        std::fs::create_dir_all(stage.join("bin")).unwrap();
        std::fs::create_dir_all(stage.join("lib/pkgconfig")).unwrap();
        std::fs::create_dir_all(stage.join("lib/oath")).unwrap();
        std::fs::write(stage.join("bin").join(name), name).unwrap();
        std::fs::write(stage.join("lib").join(lib), name).unwrap();
        std::fs::write(stage.join("lib/pkgconfig").join(format!("{name}.pc")), "").unwrap();
        let h = hash_tree(&stage).unwrap();
        oath_core::put_tree(&root.join("store/pkg").join(name), &h, &stage).unwrap();
        h
    };
    let z = pack("zz", "libz.so.1");
    converge_pkg(root, &root.join("bin"), "zz", true, &z).unwrap();
    let link = root.join("lib/libz.so.1");
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        root.join("store/pkg/zz").join(&z).join("lib/libz.so.1")
    );
    assert_eq!(std::fs::read_to_string(&link).unwrap(), "zz");
    assert!(!root.join("lib/pkgconfig").exists() && !root.join("lib/oath").exists());
    // Another package cannot take the name.
    let y = pack("yy", "libz.so.1");
    let e = converge_pkg(root, &root.join("bin"), "yy", true, &y).unwrap_err();
    assert!(e.to_string().contains("/lib/libz.so.1 exists and is not pkg:yy"), "{e}");
    converge_pkg(root, &root.join("bin"), "zz", false, &z).unwrap();
    assert!(link.symlink_metadata().is_err() && !root.join("bin/zz").exists());
}
