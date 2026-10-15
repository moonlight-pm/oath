use std::collections::HashSet;
use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::error::{Error, Result};
use crate::hooks::{Actor, ApplyHooks, ApplyReport};
use crate::id::ObjectId;
use crate::kinds::{Dev, Host, HostPower, Meta, Net, Pkg, Plan, Ssh, Svc};
use crate::{
    now_rfc3339, parse_gen_subvol, read_json, write_atomic, write_json, BTRFS_TOP, KIND_DEV,
    KIND_HOST, KIND_NET, KIND_PKG, KIND_PLAN, KIND_SNAP, KIND_SSH, KIND_SVC,
};

/// In an object directory: created by `oath new`, not applied yet.
const NEW_MARK: &str = "new";
/// In an object directory: `oath rm` is pending; apply deletes the object.
const REMOVE_MARK: &str = "remove";
/// Kinds `oath new` / `oath rm` work on. host, net, ssh, snap and dev are
/// singletons or inventory owned by the engine.
const CREATABLE: &[&str] = &[KIND_SVC, KIND_PKG];
const LOCK_WAIT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct Catalog {
    root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Object {
    pub id: ObjectId,
    pub meta: Meta,
    pub desired: Value,
    pub actual: Value,
}

#[derive(Clone, Debug)]
pub struct Drift {
    pub id: ObjectId,
    pub fields: Vec<(String, Value, Value)>,
}

/// Exclusive hold on the catalog (`<root>/.lock`, `flock`). Every writer of
/// spec or log takes it; dropping it releases the lock. Status files that
/// PID 1 owns (svc `actual.json`) are written atomically without it.
pub struct CatalogLock {
    _file: fs::File,
}

pub fn diff_values(desired: &Value, actual: &Value) -> Vec<(String, Value, Value)> {
    let mut out = Vec::new();
    let Some(d) = desired.as_object() else {
        return out;
    };
    let empty = Map::new();
    let a = actual.as_object().unwrap_or(&empty);
    for (k, dv) in d {
        let av = a.get(k).cloned().unwrap_or(Value::Null);
        if &av != dv {
            out.push((k.clone(), dv.clone(), av));
        }
    }
    out
}

/// Copy `src/objects` to `dst/objects`: what a copy snapshot keeps, and
/// all undo reads back.
pub fn copy_objects(src: &Path, dst: &Path) -> Result<()> {
    let to = dst.join("objects");
    if to.exists() {
        fs::remove_dir_all(&to)?;
    }
    copy_tree(&src.join("objects"), &to)
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for e in fs::read_dir(src)? {
        let e = e?;
        let from = e.path();
        let to = dst.join(e.file_name());
        let ft = from.symlink_metadata()?.file_type();
        if ft.is_dir() {
            copy_tree(&from, &to)?;
        } else if ft.is_symlink() {
            std::os::unix::fs::symlink(fs::read_link(&from)?, &to)?;
        } else if ft.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// The actual document an object starts with before anything converges.
pub(crate) fn first_actual(id: &ObjectId, desired: &Value, fixed: bool) -> Value {
    match id.kind.as_str() {
        KIND_SVC => json!({ "state": "stopped", "pid": null, "restarts": 0 }),
        KIND_SSH => json!({ "authorized": [], "host_key": false }),
        KIND_PKG => {
            let mut a = json!({
                "present": false,
                "links": [],
                "removable": !fixed,
            });
            for k in ["url", "needs", "description", "home"] {
                if let Some(v) = desired.get(k) {
                    a[k] = v.clone();
                }
            }
            a
        }
        _ => desired.clone(),
    }
}

impl Catalog {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self { root: root.into() })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Take the catalog lock, waiting up to 30 s for another writer.
    pub fn lock(&self) -> Result<CatalogLock> {
        fs::create_dir_all(&self.root)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join(".lock"))?;
        let deadline = Instant::now() + LOCK_WAIT;
        loop {
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(CatalogLock { _file: file });
            }
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EWOULDBLOCK) {
                return Err(err.into());
            }
            if Instant::now() >= deadline {
                return Err(Error::hint(
                    "the catalog is busy: another writer holds /oath/.lock",
                    "wait for the other oath command to finish, then retry",
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub fn kinds(&self) -> Result<Vec<String>> {
        let dir = self.root.join("schema");
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut ks = Vec::new();
        for e in fs::read_dir(&dir)? {
            let e = e?;
            let name = e.file_name();
            let name = name.to_string_lossy();
            if let Some(k) = name.strip_suffix(".json") {
                ks.push(k.to_string());
            }
        }
        ks.sort();
        Ok(ks)
    }

    pub fn schema_json(&self, kind: &str) -> Result<String> {
        let p = self.root.join("schema").join(format!("{kind}.json"));
        fs::read_to_string(&p)
            .map_err(|_| Error::hint(format!("no kind `{kind}`"), "oath schema   (list kinds)"))
    }

    pub fn schema_md(&self, kind: &str) -> Result<String> {
        let p = self.root.join("schema").join(format!("{kind}.md"));
        fs::read_to_string(&p).map_err(|_| {
            Error::hint(format!("no prose for `{kind}`"), format!("oath schema {kind}"))
        })
    }

    pub fn ls(&self, kind: Option<&str>) -> Result<Vec<ObjectId>> {
        let mut ids = Vec::new();
        let objects = self.root.join("objects");
        if !objects.is_dir() {
            return Ok(ids);
        }
        let kinds: Vec<String> =
            if let Some(k) = kind { vec![k.to_string()] } else { self.kinds()? };
        for k in kinds {
            let kd = objects.join(&k);
            if !kd.is_dir() {
                continue;
            }
            let mut names = Vec::new();
            for e in fs::read_dir(&kd)? {
                let e = e?;
                if e.file_type()?.is_dir() && e.path().join("desired.json").is_file() {
                    names.push(e.file_name().to_string_lossy().into_owned());
                }
            }
            names.sort();
            for n in names {
                ids.push(ObjectId::new(&k, n));
            }
        }
        Ok(ids)
    }

    pub fn get(&self, id: &ObjectId) -> Result<Object> {
        let dir = self.obj_dir(id);
        if !dir.join("desired.json").is_file() {
            return Err(Error::hint(format!("no object {id}"), "oath ls"));
        }
        let mut meta: Meta = read_json(&dir.join("meta.json"))?;
        let desired: Value = read_json(&dir.join("desired.json"))?;
        let actual: Value = read_json(&dir.join("actual.json")).unwrap_or(Value::Null);
        let obj = Object { id: id.clone(), meta: meta.clone(), desired, actual };
        meta.status = if dir.join(REMOVE_MARK).exists() {
            "removing".into()
        } else if dir.join(NEW_MARK).exists() {
            "new".into()
        } else if self.drift_fields(&obj)?.is_empty() {
            "in-sync".into()
        } else {
            "drift".into()
        };
        Ok(Object { id: id.clone(), meta, desired: obj.desired, actual: obj.actual })
    }

    /// `oath set`: merge fields into desired. Keys must be in the kind's
    /// schema and the result must parse as the kind's type.
    pub fn set_fields(&self, id: &ObjectId, fields: Map<String, Value>) -> Result<()> {
        let _lock = self.lock()?;
        if id.kind == KIND_SNAP && id.name != "current" {
            return Err(Error::hint(format!("{id} is read-only"), "oath schema snap"));
        }
        let mut obj = self.get(id)?;
        if self.obj_dir(id).join(REMOVE_MARK).exists() {
            return Err(Error::hint(format!("{id} is being removed"), "oath apply, or oath undo"));
        }
        if id.kind == KIND_PKG
            && fields.get("present") == Some(&Value::Bool(false))
            && !pkg_removable(&obj.actual)
        {
            return Err(Error::hint(format!("{id} is not removable"), "oath schema pkg"));
        }
        if id.kind == KIND_DEV && fields.get("present") == Some(&Value::Bool(false)) {
            return Err(Error::hint(format!("{id} is not removable"), "oath schema dev"));
        }
        self.check_keys(id, &fields)?;
        let keys: Vec<String> = fields.keys().cloned().collect();
        let Some(map) = obj.desired.as_object_mut() else {
            return Err(Error::Msg("desired is not an object".into()));
        };
        for (k, v) in fields {
            map.insert(k, v);
        }
        self.validate(id, &obj.desired).map_err(|e| match e {
            Error::Hint { message, hint } => {
                Error::Hint { message: format!("{message} (setting {})", keys.join(", ")), hint }
            }
            other => other,
        })?;
        write_json(&self.obj_dir(id).join("desired.json"), &obj.desired)?;
        self.touch_status(id, "drift")?;
        Ok(())
    }

    /// `oath new`: create a `svc` or `pkg`. It exists from now on but does
    /// nothing until `oath apply`; `oath undo` of that apply removes it.
    pub fn new_object(&self, id: &ObjectId, fields: Map<String, Value>) -> Result<()> {
        let _lock = self.lock()?;
        if !CREATABLE.contains(&id.kind.as_str()) {
            return Err(Error::hint(
                format!("oath new makes {} objects, not {}", CREATABLE.join(" or "), id.kind),
                "oath schema",
            ));
        }
        let dir = self.obj_dir(id);
        if dir.join("desired.json").is_file() {
            return Err(Error::hint(format!("{id} already exists"), format!("oath set {id} k=v")));
        }
        self.check_keys(id, &fields)?;
        let desired = Value::Object(fields);
        self.validate(id, &desired)?;
        fs::create_dir_all(&dir)?;
        write_json(&dir.join("meta.json"), &Meta::new(&id.kind, &id.name, "mutate"))?;
        write_json(&dir.join("actual.json"), &first_actual(id, &desired, false))?;
        write_atomic(&dir.join(NEW_MARK), b"")?;
        write_json(&dir.join("desired.json"), &desired)?;
        self.write_index()?;
        Ok(())
    }

    /// `oath rm`: mark a `svc` or `pkg` for removal. `oath apply` stops or
    /// unlinks it and deletes the object; undo brings it back. An object
    /// that was never applied is deleted at once.
    pub fn remove_object(&self, id: &ObjectId) -> Result<()> {
        let _lock = self.lock()?;
        if !CREATABLE.contains(&id.kind.as_str()) {
            return Err(Error::hint(format!("{id} cannot be removed"), "oath schema"));
        }
        let obj = self.get(id)?;
        if id.kind == KIND_PKG && !pkg_removable(&obj.actual) {
            return Err(Error::hint(format!("{id} is not removable"), "oath schema pkg"));
        }
        let dir = self.obj_dir(id);
        if dir.join(NEW_MARK).exists() {
            fs::remove_dir_all(&dir)?;
        } else {
            write_atomic(&dir.join(REMOVE_MARK), b"")?;
        }
        self.write_index()?;
        Ok(())
    }

    pub fn diff(&self, id: Option<&ObjectId>) -> Result<Vec<Drift>> {
        let ids = if let Some(id) = id { vec![id.clone()] } else { self.ls(None)? };
        let mut out = Vec::new();
        for id in ids {
            if id.kind == KIND_SNAP && id.name != "current" {
                continue;
            }
            if self.obj_dir(&id).join(REMOVE_MARK).exists() {
                out.push(Drift { id, fields: vec![("remove".into(), json!(true), Value::Null)] });
                continue;
            }
            let obj = self.get(&id)?;
            let mut fields = self.drift_fields(&obj)?;
            // A new object is drift until an apply takes it, even one that
            // starts in-sync (a pkg made absent): undo of that apply removes it.
            if fields.is_empty() && self.is_new(&id) {
                fields.push(("new".into(), json!(true), Value::Null));
            }
            if !fields.is_empty() {
                out.push(Drift { id, fields });
            }
        }
        Ok(out)
    }

    /// Converge drifted objects (all, or `ids`). Every selected object is
    /// checked before a generation is taken; a refusal changes nothing.
    /// Every outcome is logged: `refused` (no generation), `error` (the
    /// generation exists and is what `undo` restores), or `ok`.
    pub fn apply(
        &self,
        ids: Option<Vec<ObjectId>>,
        confirm: bool,
        actor: &Actor,
        hooks: &dyn ApplyHooks,
    ) -> Result<ApplyReport> {
        let _lock = self.lock()?;
        let drift = self.diff(None)?;
        let selected: Vec<Drift> = if let Some(ids) = ids {
            for id in &ids {
                if !self.obj_dir(id).join("desired.json").is_file() {
                    return Err(Error::hint(format!("no object {id}"), "oath ls"));
                }
            }
            drift.into_iter().filter(|d| ids.contains(&d.id)).collect()
        } else {
            drift
        };
        if selected.is_empty() {
            return Ok(ApplyReport::default());
        }
        let id_list: Vec<String> = selected.iter().map(|d| d.id.to_string()).collect();
        let parent = self.current_generation()?;

        if let Err(e) = self.preflight(&selected, confirm) {
            self.append_log(actor, None, parent, &id_list, "refused", Some(&e.to_string()))?;
            return Err(e);
        }

        let generation = self.next_generation()?;
        // The snapshot keeps last-good spec: what was applied before, not
        // the pending `set` values.
        let mut pending: Vec<(ObjectId, Value)> = Vec::new();
        for d in &selected {
            if self.removing(&d.id) || self.is_new(&d.id) {
                continue;
            }
            let obj = self.get(&d.id)?;
            pending.push((d.id.clone(), obj.desired.clone()));
            write_json(&self.obj_dir(&d.id).join("desired.json"), &self.last_good(&obj)?)?;
        }
        let snap = hooks.snapshot(generation);
        for (id, des) in &pending {
            write_json(&self.obj_dir(id).join("desired.json"), des)?;
        }
        if let Err(e) = snap {
            self.append_log(actor, None, parent, &id_list, "refused", Some(&e.to_string()))?;
            return Err(e);
        }
        self.record_generation(generation, parent, &selected)?;

        let mut report =
            ApplyReport { generation, ids: id_list.clone(), rebooting: false, halting: false };
        if let Err(e) = self.converge_selected(&selected, hooks, &mut report) {
            self.append_log(
                actor,
                Some(generation),
                parent,
                &id_list,
                "error",
                Some(&e.to_string()),
            )?;
            let _ = self.write_index();
            return Err(e);
        }
        self.append_log(actor, Some(generation), parent, &id_list, "ok", None)?;
        self.write_index()?;

        if report.rebooting {
            hooks.reboot()?;
        } else if report.halting {
            hooks.halt()?;
        }
        Ok(report)
    }

    /// Undo the newest apply not already undone (`ok` or `error`): put back
    /// the spec of the objects it touched, from its snapshot, and converge
    /// them. Status, the log, the store and host keys stay as they are.
    pub fn undo(&self, actor: &Actor, hooks: &dyn ApplyHooks) -> Result<ApplyReport> {
        let _lock = self.lock()?;
        let (generation, parent, ids) =
            self.undo_target()?.ok_or_else(|| Error::hint("nothing to undo", "oath log"))?;
        let snap = hooks.snapshot_catalog(generation)?;
        let mut back = Vec::new();
        for id in &ids {
            if self.restore_spec(id, &snap, hooks)? {
                back.push(id.clone());
            }
        }
        let mut svcs = Vec::new();
        for id in &back {
            let obj = self.get(id)?;
            match id.kind.as_str() {
                KIND_HOST => {
                    let host: Host = serde_json::from_value(obj.desired)?;
                    let actual = hooks.converge_host(&host)?;
                    write_json(&self.obj_dir(id).join("actual.json"), &actual)?;
                }
                KIND_PKG => self.converge_one_pkg(id, hooks)?,
                KIND_NET => {
                    let net: Net = serde_json::from_value(obj.desired)?;
                    let actual = hooks.converge_net(id, &net)?;
                    write_json(&self.obj_dir(id).join("actual.json"), &actual)?;
                }
                KIND_SSH => {
                    let ssh: Ssh = serde_json::from_value(obj.desired)?;
                    let actual = hooks.converge_ssh(id, &ssh)?;
                    write_json(&self.obj_dir(id).join("actual.json"), &actual)?;
                }
                KIND_DEV => {
                    let dev: Dev = serde_json::from_value(obj.desired)?;
                    let actual = hooks.converge_dev(id, &dev)?;
                    write_json(&self.obj_dir(id).join("actual.json"), &actual)?;
                }
                KIND_SVC => {
                    let enabled =
                        obj.desired.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
                    svcs.push((id.clone(), enabled));
                }
                KIND_PLAN => {
                    let mut actual = obj.actual.clone();
                    if let Some(h) = obj.desired.get("hash") {
                        actual["hash"] = h.clone();
                    }
                    write_json(&self.obj_dir(id).join("actual.json"), &actual)?;
                }
                _ => {}
            }
            self.touch_status(id, "in-sync")?;
        }

        let cur = ObjectId::new(KIND_SNAP, "current");
        let g = json!({ "generation": parent });
        write_json(&self.obj_dir(&cur).join("desired.json"), &g)?;
        write_json(&self.obj_dir(&cur).join("actual.json"), &g)?;

        hooks.notify_init()?;
        for (id, enabled) in svcs {
            hooks.wait_converge(&id, enabled)?;
        }

        let ids = vec![format!("undo:{generation}")];
        self.append_log(actor, Some(parent), generation, &ids, "ok", None)?;
        self.write_index()?;
        Ok(ApplyReport { generation: parent, ids, ..Default::default() })
    }

    pub fn log_lines(&self) -> Result<Vec<Value>> {
        let p = self.root.join("log/apply.jsonl");
        if !p.is_file() {
            return Ok(Vec::new());
        }
        let s = fs::read_to_string(p)?;
        let mut out = Vec::new();
        for line in s.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            out.push(serde_json::from_str(line)?);
        }
        Ok(out)
    }

    pub fn write_index(&self) -> Result<()> {
        let body = crate::index::generate(self)?;
        write_atomic(&self.root.join("INDEX.md"), body.as_bytes())
    }

    pub fn index_text(&self) -> Result<String> {
        let p = self.root.join("INDEX.md");
        if p.is_file() {
            Ok(fs::read_to_string(p)?)
        } else {
            crate::index::generate(self)
        }
    }

    /// Parse `doc` as `id`'s kind and check what the type cannot.
    pub fn validate(&self, id: &ObjectId, doc: &Value) -> Result<()> {
        let bad = |e: serde_json::Error| {
            Error::hint(format!("{id}: {e}"), format!("oath schema {}", id.kind))
        };
        let hint = || format!("oath schema {}", id.kind);
        match id.kind.as_str() {
            KIND_HOST => {
                let h: Host = serde_json::from_value(doc.clone()).map_err(bad)?;
                if h.hostname.is_empty()
                    || h.hostname.len() > 63
                    || !h
                        .hostname
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
                {
                    return Err(Error::hint(
                        format!("{id}: hostname `{}` is not a host name", h.hostname),
                        hint(),
                    ));
                }
            }
            KIND_SVC => {
                let s: Svc = serde_json::from_value(doc.clone()).map_err(bad)?;
                if s.enabled && s.exec.is_empty() {
                    return Err(Error::hint(format!("{id}: enabled with an empty exec"), hint()));
                }
            }
            KIND_PKG => {
                let p: Pkg = serde_json::from_value(doc.clone()).map_err(bad)?;
                if !p.hash.is_empty() && !crate::is_realization_id(&p.hash) {
                    return Err(Error::hint(
                        format!("{id}: hash must be sha256- and 64 hex digits"),
                        hint(),
                    ));
                }
            }
            KIND_NET => {
                let n: Net = serde_json::from_value(doc.clone()).map_err(bad)?;
                if n.ipv4 != "dhcp" && !is_cidr(&n.ipv4) {
                    return Err(Error::hint(
                        format!("{id}: ipv4 is `dhcp` or a.b.c.d/n, not `{}`", n.ipv4),
                        hint(),
                    ));
                }
            }
            KIND_SSH => {
                let s: Ssh = serde_json::from_value(doc.clone()).map_err(bad)?;
                if let Some(k) = s.authorized.iter().find(|k| {
                    let k = k.trim();
                    !(k.starts_with("ssh-") || k.starts_with("ecdsa-") || k.starts_with("sk-"))
                }) {
                    return Err(Error::hint(format!("{id}: not a public key: {k}"), hint()));
                }
            }
            KIND_DEV => {
                let _: Dev = serde_json::from_value(doc.clone()).map_err(bad)?;
            }
            KIND_PLAN => {
                let _: Plan = serde_json::from_value(doc.clone()).map_err(bad)?;
            }
            KIND_SNAP => {
                if doc.get("generation").and_then(|g| g.as_u64()).is_none() {
                    return Err(Error::hint(format!("{id}: generation must be a number"), hint()));
                }
            }
            other => {
                return Err(Error::hint(format!("no kind `{other}`"), "oath schema"));
            }
        }
        Ok(())
    }

    fn check_keys(&self, id: &ObjectId, fields: &Map<String, Value>) -> Result<()> {
        let allowed = self.allowed_keys(&id.kind)?;
        for k in fields.keys() {
            if !allowed.iter().any(|a| a == k) {
                return Err(Error::hint(
                    format!("unknown field `{k}` on {id}"),
                    format!("oath schema {}", id.kind),
                ));
            }
        }
        Ok(())
    }

    /// Everything that can be refused without touching the machine.
    fn preflight(&self, selected: &[Drift], confirm: bool) -> Result<()> {
        for d in selected {
            if self.removing(&d.id) {
                continue;
            }
            self.validate(&d.id, &self.get(&d.id)?.desired)?;
        }
        let confirm_needed: Vec<String> =
            selected.iter().filter(|d| self.needs_confirm(d)).map(|d| d.id.to_string()).collect();
        if !confirm_needed.is_empty() && !confirm {
            return Err(Error::Confirm(format!(
                "{} needs --confirm: it powers off, changes the boot generation, or can cut \
                 this machine off from SSH. See /oath/INDEX.md safety.",
                confirm_needed.join(", ")
            )));
        }
        self.check_svc_wants()?;
        self.check_pkg_needs()?;
        Ok(())
    }

    fn converge_selected(
        &self,
        selected: &[Drift],
        hooks: &dyn ApplyHooks,
        report: &mut ApplyReport,
    ) -> Result<()> {
        for d in selected {
            if self.removing(&d.id) {
                self.converge_removal(&d.id, hooks)?;
                continue;
            }
            self.touch_status(&d.id, "applying")?;
            let res = self.converge_one(&d.id, hooks, report);
            if let Err(e) = res {
                let _ = self.touch_status(&d.id, "error");
                return Err(e);
            }
            let _ = fs::remove_file(self.obj_dir(&d.id).join(NEW_MARK));
            self.touch_status(&d.id, "in-sync")?;
        }
        Ok(())
    }

    fn converge_one(
        &self,
        id: &ObjectId,
        hooks: &dyn ApplyHooks,
        report: &mut ApplyReport,
    ) -> Result<()> {
        let dir = self.obj_dir(id);
        match id.kind.as_str() {
            KIND_HOST => {
                let mut host: Host = serde_json::from_value(self.get(id)?.desired)?;
                let reboot = host.power == HostPower::Reboot;
                let halt = host.power == HostPower::Halt;
                if reboot || halt {
                    host.power = HostPower::Run;
                    write_json(&dir.join("desired.json"), &host)?;
                }
                let actual = hooks.converge_host(&host)?;
                write_json(&dir.join("actual.json"), &actual)?;
                report.rebooting |= reboot;
                report.halting |= halt;
            }
            KIND_SVC => {
                let obj = self.get(id)?;
                let enabled = obj.desired.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
                write_json(&dir.join("applied.json"), &obj.desired)?;
                hooks.notify_init()?;
                hooks.wait_converge(id, enabled)?;
            }
            KIND_SNAP => {
                write_json(&dir.join("actual.json"), &self.get(id)?.desired)?;
            }
            KIND_PKG => self.converge_one_pkg(id, hooks)?,
            KIND_NET => {
                let net: Net = serde_json::from_value(self.get(id)?.desired)?;
                let actual = hooks.converge_net(id, &net)?;
                write_json(&dir.join("actual.json"), &actual)?;
            }
            KIND_SSH => {
                let ssh: Ssh = serde_json::from_value(self.get(id)?.desired)?;
                let actual = hooks.converge_ssh(id, &ssh)?;
                write_json(&dir.join("actual.json"), &actual)?;
            }
            KIND_PLAN => {
                let obj = self.get(id)?;
                let mut actual = obj.actual.clone();
                if let Some(h) = obj.desired.get("hash") {
                    actual["hash"] = h.clone();
                }
                write_json(&dir.join("actual.json"), &actual)?;
            }
            KIND_DEV => {
                let dev: Dev = serde_json::from_value(self.get(id)?.desired)?;
                let actual = hooks.converge_dev(id, &dev)?;
                write_json(&dir.join("actual.json"), &actual)?;
            }
            other => {
                return Err(Error::hint(
                    format!("no handler for {other}"),
                    format!("oath schema {other}"),
                ));
            }
        }
        Ok(())
    }

    /// Stop or unlink, then delete the object directory.
    fn converge_removal(&self, id: &ObjectId, hooks: &dyn ApplyHooks) -> Result<()> {
        let dir = self.obj_dir(id);
        let obj = self.get(id)?;
        match id.kind.as_str() {
            KIND_SVC => {
                let mut off = obj.desired.clone();
                off["enabled"] = json!(false);
                write_json(&dir.join("applied.json"), &off)?;
                hooks.notify_init()?;
                hooks.wait_converge(id, false)?;
            }
            KIND_PKG => {
                let mut pkg: Pkg = serde_json::from_value(obj.desired)?;
                pkg.present = false;
                hooks.converge_pkg(id, &pkg)?;
            }
            _ => {}
        }
        fs::remove_dir_all(&dir)?;
        if id.kind == KIND_SVC {
            hooks.notify_init()?;
        }
        Ok(())
    }

    /// Put back `id`'s spec from a snapshot catalog. Returns false when the
    /// object did not exist then (it is taken out again).
    fn restore_spec(&self, id: &ObjectId, snap: &Path, hooks: &dyn ApplyHooks) -> Result<bool> {
        let from = snap.join("objects").join(&id.kind).join(&id.name);
        let to = self.obj_dir(id);
        let existed = from.join("desired.json").is_file() && !from.join(NEW_MARK).exists();
        if !existed {
            if to.join("desired.json").is_file() {
                match id.kind.as_str() {
                    KIND_PKG => {
                        if let Ok(mut pkg) = serde_json::from_value::<Pkg>(self.get(id)?.desired) {
                            pkg.present = false;
                            hooks.converge_pkg(id, &pkg)?;
                        }
                    }
                    KIND_SVC => {
                        let mut off: Value = read_json(&to.join("desired.json"))?;
                        off["enabled"] = json!(false);
                        write_json(&to.join("applied.json"), &off)?;
                        hooks.notify_init()?;
                        hooks.wait_converge(id, false)?;
                    }
                    _ => {}
                }
            }
            if to.exists() {
                fs::remove_dir_all(&to)?;
            }
            return Ok(false);
        }
        fs::create_dir_all(&to)?;
        for name in ["meta.json", "actual.json"] {
            if !to.join(name).is_file() && from.join(name).is_file() {
                fs::copy(from.join(name), to.join(name))?;
            }
        }
        let desired: Value = read_json(&from.join("desired.json"))?;
        if id.kind == KIND_SVC {
            let applied = if from.join("applied.json").is_file() {
                read_json(&from.join("applied.json"))?
            } else {
                desired.clone()
            };
            write_json(&to.join("applied.json"), &applied)?;
        }
        write_json(&to.join("desired.json"), &desired)?;
        let _ = fs::remove_file(to.join(NEW_MARK));
        let _ = fs::remove_file(to.join(REMOVE_MARK));
        Ok(true)
    }

    /// Newest apply that took a generation and has not been undone.
    fn undo_target(&self) -> Result<Option<(u64, u64, Vec<ObjectId>)>> {
        let mut undone = HashSet::new();
        for rec in self.log_lines()?.into_iter().rev() {
            let ids: Vec<String> = rec
                .get("ids")
                .and_then(|i| i.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            if let Some(g) = ids.first().and_then(|s| s.strip_prefix("undo:")) {
                if let Ok(g) = g.parse::<u64>() {
                    undone.insert(g);
                }
                continue;
            }
            let Some(generation) = rec.get("generation").and_then(|g| g.as_u64()) else {
                continue;
            };
            if !matches!(rec.get("result").and_then(|r| r.as_str()), Some("ok" | "error")) {
                continue;
            }
            if undone.contains(&generation) {
                continue;
            }
            let parent = rec.get("parent_generation").and_then(|g| g.as_u64()).unwrap_or(0);
            let ids = ids.iter().filter_map(|s| s.parse().ok()).collect();
            return Ok(Some((generation, parent, ids)));
        }
        Ok(None)
    }

    fn last_good(&self, obj: &Object) -> Result<Value> {
        let id = &obj.id;
        Ok(match id.kind.as_str() {
            KIND_SVC => {
                let p = self.obj_dir(id).join("applied.json");
                if p.is_file() {
                    read_json(&p)?
                } else {
                    obj.desired.clone()
                }
            }
            KIND_PKG => {
                let mut last = json!({
                    "present": obj.actual.get("present").cloned().unwrap_or(json!(false)),
                });
                if let Some(h) = obj.actual.get("hash") {
                    if !h.as_str().unwrap_or("").is_empty() {
                        last["hash"] = h.clone();
                    }
                }
                // Only fields the spec has: an invented `url: ""` would
                // drift after undo against an actual that has none.
                for k in ["url", "needs", "description", "home"] {
                    if let Some(v) = obj.desired.get(k) {
                        last[k] = v.clone();
                    }
                }
                last
            }
            KIND_NET => json!({
                "up": obj.actual.get("up").cloned().unwrap_or(json!(false)),
                "ipv4": obj.actual.get("ipv4").cloned().unwrap_or(json!("")),
                "gateway": obj.actual.get("gateway").cloned().unwrap_or(json!("")),
                "lease": null,
            }),
            KIND_SSH => json!({
                "authorized": obj.actual.get("authorized").cloned().unwrap_or(json!([]))
            }),
            KIND_DEV => json!({
                "present": obj.actual.get("present").cloned().unwrap_or(json!(true))
            }),
            KIND_HOST => {
                let mut a = obj.actual.clone();
                if let Some(m) = a.as_object_mut() {
                    m.retain(|k, _| {
                        matches!(k.as_str(), "hostname" | "power" | "env" | "timezone")
                    });
                }
                a
            }
            _ => obj.actual.clone(),
        })
    }

    fn converge_one_pkg(&self, id: &ObjectId, hooks: &dyn ApplyHooks) -> Result<()> {
        let obj = self.get(id)?;
        let pkg: Pkg = serde_json::from_value(obj.desired.clone())?;
        let removable = pkg_removable(&obj.actual);
        if !pkg.present && !removable {
            return Err(Error::hint(format!("{id} is not removable"), "oath schema pkg"));
        }
        let mut actual = hooks.converge_pkg(id, &pkg)?;
        actual.removable = removable;
        actual.url = pkg.url.clone();
        actual.needs = pkg.needs.clone();
        actual.description = pkg.description.clone();
        actual.home = pkg.home.clone();
        if actual.hash.is_empty() && !pkg.hash.is_empty() {
            actual.hash = pkg.hash.clone();
        }
        write_json(&self.obj_dir(id).join("actual.json"), &actual)?;
        self.touch_status(id, "in-sync")?;
        Ok(())
    }

    fn check_svc_wants(&self) -> Result<()> {
        let mut svcs = Vec::new();
        for id in self.ls(Some(KIND_SVC))? {
            if self.removing(&id) {
                continue;
            }
            if let Ok(spec) = serde_json::from_value::<Svc>(self.get(&id)?.desired) {
                svcs.push((id.to_string(), spec));
            }
        }
        crate::svc_start_order(&svcs)?;
        Ok(())
    }

    fn check_pkg_needs(&self) -> Result<()> {
        let mut pkgs = Vec::new();
        for id in self.ls(Some(KIND_PKG))? {
            let obj = self.get(&id)?;
            if let Ok(mut spec) = serde_json::from_value::<Pkg>(obj.desired) {
                if spec.hash.is_empty() {
                    spec.hash =
                        obj.actual.get("hash").and_then(|h| h.as_str()).unwrap_or("").to_string();
                }
                if self.removing(&id) {
                    spec.present = false;
                }
                pkgs.push((id.to_string(), spec));
            }
        }
        crate::pkg::check_needs(&pkgs)
    }

    pub(crate) fn obj_dir(&self, id: &ObjectId) -> PathBuf {
        self.root.join("objects").join(&id.kind).join(&id.name)
    }

    fn removing(&self, id: &ObjectId) -> bool {
        self.obj_dir(id).join(REMOVE_MARK).exists()
    }

    fn is_new(&self, id: &ObjectId) -> bool {
        self.obj_dir(id).join(NEW_MARK).exists()
    }

    fn allowed_keys(&self, kind: &str) -> Result<Vec<String>> {
        let raw = self.schema_json(kind)?;
        let v: Value = serde_json::from_str(&raw)?;
        let mut keys = Vec::new();
        if let Some(props) = v.get("properties").and_then(|p| p.as_object()) {
            keys.extend(props.keys().cloned());
        }
        Ok(keys)
    }

    fn drift_fields(&self, obj: &Object) -> Result<Vec<(String, Value, Value)>> {
        if obj.id.kind == KIND_SVC {
            let applied_p = self.obj_dir(&obj.id).join("applied.json");
            let applied = if applied_p.is_file() { read_json(&applied_p)? } else { Value::Null };
            return Ok(diff_values(&obj.desired, &applied));
        }
        Ok(diff_values(&obj.desired, &obj.actual))
    }

    /// Power, the boot generation, and changes that can cut off SSH.
    fn needs_confirm(&self, d: &Drift) -> bool {
        let id = &d.id;
        let changed = |k: &str| d.fields.iter().find(|(f, _, _)| f == k).map(|(_, new, _)| new);
        match (id.kind.as_str(), id.name.as_str()) {
            (KIND_HOST, _) => changed("power").is_some_and(|p| p.as_str() != Some("run")),
            (KIND_SNAP, "current") => true,
            (KIND_SVC, "sshd") => {
                self.removing(id)
                    || changed("enabled") == Some(&json!(false))
                    || ["exec", "user", "env"].iter().any(|k| changed(k).is_some())
            }
            (KIND_SSH, "local") => changed("authorized")
                .is_some_and(|a| a.as_array().map(|a| a.is_empty()).unwrap_or(true)),
            (KIND_NET, "net0") => ["up", "ipv4", "gateway"].iter().any(|k| changed(k).is_some()),
            _ => false,
        }
    }

    fn current_generation(&self) -> Result<u64> {
        let id = ObjectId::new(KIND_SNAP, "current");
        let obj = self.get(&id)?;
        Ok(obj.actual.get("generation").and_then(|v| v.as_u64()).unwrap_or(0))
    }

    fn next_generation(&self) -> Result<u64> {
        let mut max = self.current_generation()?;
        if let Ok(ids) = self.ls(Some(KIND_SNAP)) {
            for id in ids {
                if let Ok(n) = id.name.parse::<u64>() {
                    max = max.max(n);
                }
            }
        }
        if let Ok(lines) = self.log_lines() {
            for rec in lines {
                if let Some(n) = rec.get("generation").and_then(|v| v.as_u64()) {
                    max = max.max(n);
                }
                if let Some(n) = rec.get("parent_generation").and_then(|v| v.as_u64()) {
                    max = max.max(n);
                }
            }
        }
        for dir in [Path::new(BTRFS_TOP), self.root.join(".gens").as_path()] {
            if let Ok(rd) = fs::read_dir(dir) {
                for e in rd.flatten() {
                    let name = e.file_name();
                    let name = name.to_string_lossy();
                    if let Some(n) = parse_gen_subvol(&name) {
                        max = max.max(n);
                    } else if let Ok(n) = name.parse::<u64>() {
                        max = max.max(n);
                    }
                }
            }
        }
        Ok(max + 1)
    }

    fn record_generation(&self, generation: u64, parent: u64, selected: &[Drift]) -> Result<()> {
        let reason = selected.iter().map(|d| d.id.to_string()).collect::<Vec<_>>().join(",");
        let rec = json!({
            "generation": generation,
            "parent": parent,
            "time": now_rfc3339(),
            "reason": format!("apply {reason}"),
        });
        let id = ObjectId::new(KIND_SNAP, generation.to_string());
        let dir = self.obj_dir(&id);
        fs::create_dir_all(&dir)?;
        write_json(&dir.join("actual.json"), &rec)?;
        write_json(
            &dir.join("meta.json"),
            &Meta::new(KIND_SNAP, &generation.to_string(), "mutate"),
        )?;
        // no desired — read-only
        write_atomic(&dir.join("desired.json"), b"{}\n")?;

        let cur = ObjectId::new(KIND_SNAP, "current");
        let g = json!({ "generation": generation });
        write_json(&self.obj_dir(&cur).join("desired.json"), &g)?;
        write_json(&self.obj_dir(&cur).join("actual.json"), &g)?;
        Ok(())
    }

    pub(crate) fn touch_status(&self, id: &ObjectId, status: &str) -> Result<()> {
        let p = self.obj_dir(id).join("meta.json");
        let mut meta: Meta = read_json(&p)?;
        meta.status = status.into();
        write_json(&p, &meta)?;
        Ok(())
    }

    fn append_log(
        &self,
        actor: &Actor,
        generation: Option<u64>,
        parent: u64,
        ids: &[String],
        result: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let rec = json!({
            "time": now_rfc3339(),
            "actor": { "uid": actor.uid, "tty": actor.tty },
            "ids": ids,
            "generation": generation,
            "parent_generation": parent,
            "result": result,
            "error": error,
        });
        let line = serde_json::to_string(&rec)?;
        let path = self.root.join("log/apply.jsonl");
        if let Some(d) = path.parent() {
            fs::create_dir_all(d)?;
        }
        use std::io::Write;
        let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(f, "{line}")?;
        f.sync_all()?;
        Ok(())
    }
}

fn pkg_removable(actual: &Value) -> bool {
    actual.get("removable").and_then(|v| v.as_bool()).unwrap_or(true)
}

fn is_cidr(s: &str) -> bool {
    let Some((ip, len)) = s.split_once('/') else {
        return false;
    };
    ip.parse::<std::net::Ipv4Addr>().is_ok() && len.parse::<u8>().is_ok_and(|n| n <= 32)
}
