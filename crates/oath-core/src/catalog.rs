use std::collections::HashSet;
use std::fs;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::error::{Error, Result};
use crate::hooks::{Actor, ApplyHooks, ApplyReport};
use crate::id::ObjectId;
use crate::kinds::{Dev, Host, HostPower, Meta, Net, Pkg, PkgActual, Plan, Snap, Ssh, Svc};
use crate::probation;
use crate::{
    now_rfc3339, parse_gen_subvol, read_json, write_atomic, write_json, BTRFS_TOP, KIND_DEV,
    KIND_HOST, KIND_NET, KIND_PKG, KIND_PLAN, KIND_SNAP, KIND_SSH, KIND_SVC,
};

/// In an object directory: created by `oath new`, not applied yet.
const NEW_MARK: &str = "new";
/// In an object directory: `oath rm` is pending; apply deletes the object.
const REMOVE_MARK: &str = "remove";
/// In an object directory: the spec as it was last applied. Drift is
/// desired against it, and undo puts it back.
const APPLIED: &str = "applied.json";
/// Where status (`actual`) lives, under the catalog root: a tmpfs on the
/// machine, so no snapshot holds it and every boot observes afresh (R14).
pub const STATUS_DIR: &str = "status";

/// `<root>/status/<kind>/<name>.json`: what the machine observed of `id`.
pub fn status_path(root: &Path, id: &ObjectId) -> PathBuf {
    root.join(STATUS_DIR).join(&id.kind).join(format!("{}.json", id.name))
}
/// A package that can never be removed: seeded fixed, or fixed by this
/// build's `base` profile.
/// A package is fixed when this build's `base` profile fixes it, whatever
/// its `meta.fixed` says: that was written when the machine was seeded,
/// and a package a later base no longer fixes (dropbear, R36) is the
/// owner's to remove.
fn fixed(obj: &Object) -> bool {
    if obj.id.kind == KIND_PKG {
        crate::seed::base_fixes(&obj.id)
    } else {
        obj.meta.fixed
    }
}

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

/// One package's tree, hashed again by [`Catalog::verify`].
#[derive(Clone, Debug)]
pub struct Verified {
    pub id: ObjectId,
    /// What it is linked as.
    pub hash: String,
    pub verified: crate::PkgVerified,
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

/// Write down what the type fills in where drift compares a field whole:
/// a `host:local.mounts` entry without `options` would otherwise differ
/// from the spec apply records. `doc` has passed `validate`.
pub(crate) fn normalize(id: &ObjectId, doc: &mut Value) -> Result<()> {
    if id.kind == KIND_HOST && doc.get("mounts").is_some() {
        let h: Host = serde_json::from_value(doc.clone())?;
        doc["mounts"] = serde_json::to_value(&h.mounts)?;
    }
    Ok(())
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

impl Catalog {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self { root: root.into() })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The catalog lock if nobody else holds it, without waiting (PID 1
    /// must not stall on an `oath apply`).
    pub fn try_lock(&self) -> Result<Option<CatalogLock>> {
        fs::create_dir_all(&self.root)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join(".lock"))?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return Ok(Some(CatalogLock { _file: file }));
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
            return Ok(None);
        }
        Err(err.into())
    }

    /// Make the `dev:*` objects what discovery `found`: new hardware gets
    /// an object (desired `present: true`), hardware that is gone loses
    /// its object, and the rest get a fresh actual. The caller holds the
    /// lock. Returns the changes (`+dev:x`, `-dev:y`).
    pub fn write_devices(
        &self,
        _lock: &CatalogLock,
        found: &[(String, crate::DevActual)],
    ) -> Result<Vec<String>> {
        let mut changes = Vec::new();
        for id in self.ls(Some(KIND_DEV))? {
            if !found.iter().any(|(n, _)| *n == id.name) {
                fs::remove_dir_all(self.obj_dir(&id))?;
                self.drop_actual(&id);
                changes.push(format!("-{id}"));
            }
        }
        for (name, actual) in found {
            let id = ObjectId::new(KIND_DEV, name);
            let dir = self.obj_dir(&id);
            if !dir.join("desired.json").is_file() {
                fs::create_dir_all(&dir)?;
                write_json(&dir.join("meta.json"), &Meta::new(KIND_DEV, name, "mutate"))?;
                write_json(&dir.join("desired.json"), &json!({ "present": true }))?;
                write_json(&dir.join(APPLIED), &json!({ "present": true }))?;
                changes.push(format!("+{id}"));
            }
            let path = status_path(&self.root, &id);
            if read_json::<crate::DevActual>(&path).ok().as_ref() != Some(actual) {
                write_json(&path, actual)?;
            }
        }
        if !changes.is_empty() {
            self.write_index()?;
        }
        Ok(changes)
    }

    /// Bring a catalog from before R14 to the split. Each object that
    /// converges gets the `applied.json` its old `actual.json` stood for; a
    /// fixed package's `removable: false` becomes `meta.fixed`; a
    /// generation record moves into its `desired.json`; a service's
    /// `last.json` moves to the log tree; and the old `actual.json` goes
    /// (status is observed again). Returns how many objects changed.
    pub fn migrate(&self, _lock: &CatalogLock) -> Result<usize> {
        let mut changed = 0;
        for id in self.ls(None)? {
            let dir = self.obj_dir(&id);
            let last = dir.join("last.json");
            if last.is_file() {
                let to =
                    self.root.join("log").join(&id.kind).join(format!("{}.last.json", id.name));
                fs::create_dir_all(to.parent().unwrap_or(&self.root))?;
                fs::rename(&last, &to)?;
                changed += 1;
            }
            let legacy = dir.join("actual.json");
            if !legacy.is_file() {
                continue;
            }
            let actual: Value = read_json(&legacy).unwrap_or(Value::Null);
            let mut meta: Meta = read_json(&dir.join("meta.json"))?;
            let desired: Value = read_json(&dir.join("desired.json"))?;
            if id.kind == KIND_SNAP && id.name != "current" {
                if desired.get("generation").is_none() && actual.get("generation").is_some() {
                    write_json(&dir.join("desired.json"), &actual)?;
                }
            } else if id.kind != KIND_SVC && !dir.join(APPLIED).is_file() {
                let applied = match id.kind.as_str() {
                    KIND_PLAN => {
                        json!({ "hash": actual.get("hash").cloned().unwrap_or(json!("")) })
                    }
                    _ => {
                        let obj = Object {
                            id: id.clone(),
                            meta: meta.clone(),
                            desired,
                            actual: actual.clone(),
                        };
                        self.last_good(&obj)?
                    }
                };
                write_json(&dir.join(APPLIED), &applied)?;
            }
            if id.kind == KIND_PKG && actual.get("removable") == Some(&json!(false)) && !meta.fixed
            {
                meta.fixed = true;
                write_json(&dir.join("meta.json"), &meta)?;
            }
            fs::remove_file(&legacy)?;
            changed += 1;
        }
        Ok(changed)
    }

    /// Observe what only the catalog and the store can say (PID 1 at boot,
    /// on a fresh `status/`): each package from its tree and the links in
    /// `bin_dir`, each plan and the current generation from their applied
    /// specs. Hosts, links, keys, devices and services are observed by
    /// whoever converges them.
    pub fn observe(&self, _lock: &CatalogLock, bin_dir: &Path) -> Result<()> {
        for id in self.ls(Some(KIND_PKG))? {
            let obj = self.get(&id)?;
            let removable = !fixed(&obj);
            let spec = self.applied(&id)?.unwrap_or(obj.desired);
            let Ok(spec) = serde_json::from_value::<Pkg>(spec) else { continue };
            let mut actual = crate::pkg::observe(&self.root, bin_dir, &id.name, spec.present);
            actual.removable = removable;
            actual.url = spec.url;
            actual.needs = spec.needs;
            actual.description = spec.description;
            actual.home = spec.home;
            actual.source = spec.source;
            self.write_actual(&id, &actual)?;
        }
        for id in self.ls(Some(KIND_PLAN))? {
            let spec = self.applied(&id)?.unwrap_or(self.get(&id)?.desired);
            let hash = spec.get("hash").cloned().unwrap_or(json!(""));
            self.write_actual(&id, &json!({ "hash": hash }))?;
        }
        let cur = ObjectId::new(KIND_SNAP, "current");
        if self.obj_dir(&cur).is_dir() {
            let spec = self.applied(&cur)?.unwrap_or(self.get(&cur)?.desired);
            self.write_actual(&cur, &spec)?;
        }
        Ok(())
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

    /// The kind's schema, generated from its type (the file under
    /// `schema/` is a copy for readers).
    pub fn schema_json(&self, kind: &str) -> Result<String> {
        if !self.root.join("schema").join(format!("{kind}.json")).is_file() {
            return Err(Error::hint(format!("no kind `{kind}`"), "oath schema   (list kinds)"));
        }
        crate::schema::text(kind)
            .ok_or_else(|| Error::hint(format!("no kind `{kind}`"), "oath schema   (list kinds)"))
    }

    pub fn schema_md(&self, kind: &str) -> Result<String> {
        crate::schema::prose(kind).map(str::to_string).ok_or_else(|| {
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
        // Not observed yet: null. A catalog from before R14 kept it here.
        let actual: Value = read_json(&status_path(&self.root, id))
            .or_else(|_| read_json(&dir.join("actual.json")))
            .unwrap_or(Value::Null);
        let obj = Object { id: id.clone(), meta: meta.clone(), desired, actual };
        meta.status = if dir.join(REMOVE_MARK).exists() {
            "removing".into()
        } else if dir.join(NEW_MARK).exists() {
            "new".into()
        } else if obj.actual.pointer("/verified/ok") == Some(&json!(false))
            || (id.kind == KIND_SVC && obj.actual.get("state") == Some(&json!("failed")))
        {
            // A package whose stored tree no longer hashes to its pin, or a
            // service PID 1 gave up on (its actual says why).
            "error".into()
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
        if id.kind == KIND_PKG && fields.get("present") == Some(&Value::Bool(false)) && fixed(&obj)
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
        normalize(id, &mut obj.desired)?;
        write_json(&self.obj_dir(id).join("desired.json"), &obj.desired)?;
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
        let given = Value::Object(fields);
        self.validate(id, &given)?;
        // What the type fills in for absent fields is written down, so the
        // object says what it will do.
        let desired = match id.kind.as_str() {
            KIND_SVC => serde_json::to_value(serde_json::from_value::<Svc>(given)?)?,
            KIND_PKG => serde_json::to_value(serde_json::from_value::<Pkg>(given)?)?,
            _ => given,
        };
        fs::create_dir_all(&dir)?;
        write_json(&dir.join("meta.json"), &Meta::new(&id.kind, &id.name, "mutate"))?;
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
        if id.kind == KIND_PKG && fixed(&obj) {
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
        self.apply_with(ids, confirm, probation::DEFAULT_SECS, actor, hooks)
    }

    /// [`Catalog::apply`], with the seconds a change that can cut off SSH
    /// stays on probation before PID 1 undoes it (unless `oath keep`).
    pub fn apply_with(
        &self,
        ids: Option<Vec<ObjectId>>,
        confirm: bool,
        probation_secs: u64,
        actor: &Actor,
        hooks: &dyn ApplyHooks,
    ) -> Result<ApplyReport> {
        if !(probation::MIN_SECS..=probation::MAX_SECS).contains(&probation_secs) {
            return Err(Error::hint(
                format!(
                    "probation must be {}..={} seconds, not {probation_secs}",
                    probation::MIN_SECS,
                    probation::MAX_SECS
                ),
                "oath apply --confirm --probation <secs>",
            ));
        }
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

        if let Some(p) = probation::read(&self.root) {
            let e = Error::hint(
                format!(
                    "generation {} ({}) is on probation; nothing else applies until it is kept \
                     or undone",
                    p.generation,
                    p.ids.join(", ")
                ),
                "from a new SSH session: sudo oath keep; or sudo oath undo",
            );
            self.append_log(
                actor,
                confirm,
                None,
                parent,
                &id_list,
                "refused",
                Some(&e.to_string()),
            )?;
            return Err(e);
        }
        if let Err(e) = self.preflight(&selected, confirm) {
            self.append_log(
                actor,
                confirm,
                None,
                parent,
                &id_list,
                "refused",
                Some(&e.to_string()),
            )?;
            return Err(e);
        }
        // Power is not configuration: a reboot or halt on its own takes no
        // generation and leaves undo nothing to take back.
        if selected
            .iter()
            .all(|d| d.id.kind == KIND_HOST && d.fields.iter().all(|(f, _, _)| f == "power"))
        {
            let mut report = ApplyReport {
                generation: parent,
                ids: id_list.clone(),
                power_only: true,
                ..Default::default()
            };
            if let Err(e) = self.converge_selected(&selected, hooks, &mut report) {
                self.append_log(
                    actor,
                    confirm,
                    None,
                    parent,
                    &id_list,
                    "error",
                    Some(&e.to_string()),
                )?;
                return Err(e);
            }
            self.append_log(actor, confirm, None, parent, &id_list, "ok", None)?;
            self.write_index()?;
            if report.rebooting {
                hooks.reboot()?;
            } else if report.halting {
                hooks.halt()?;
            }
            return Ok(report);
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
            self.append_log(
                actor,
                confirm,
                None,
                parent,
                &id_list,
                "refused",
                Some(&e.to_string()),
            )?;
            return Err(e);
        }
        self.record_generation(generation, parent, &selected)?;

        let mut report = ApplyReport {
            generation,
            ids: id_list.clone(),
            rebooting: false,
            halting: false,
            probation: None,
            power_only: false,
        };
        // A change that can cut off SSH is undone unless a new session
        // keeps it, failed or not: a half-converged net is just as likely
        // to have locked the owner out.
        let lockout: Vec<String> =
            selected.iter().filter(|d| self.lockout(d)).map(|d| d.id.to_string()).collect();
        let converged = self.converge_selected(&selected, hooks, &mut report);
        if !lockout.is_empty() {
            probation::write(
                &self.root,
                &probation::Probation::open(generation, lockout, probation_secs),
            )?;
            report.probation = Some(probation_secs);
        }
        if let Err(e) = converged {
            self.append_log(
                actor,
                confirm,
                Some(generation),
                parent,
                &id_list,
                "error",
                Some(&e.to_string()),
            )?;
            let _ = self.write_index();
            return Err(e);
        }
        self.append_log(actor, confirm, Some(generation), parent, &id_list, "ok", None)?;
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
        let target =
            self.undo_target()?.ok_or_else(|| Error::hint("nothing to undo", "oath log"))?;
        self.undo_locked(target, actor, hooks)
    }

    /// PID 1's rollback: undo the apply on probation if its time has run
    /// out and nothing has kept or undone it since. `None` when there is
    /// nothing to do.
    pub fn undo_expired(
        &self,
        actor: &Actor,
        hooks: &dyn ApplyHooks,
    ) -> Result<Option<ApplyReport>> {
        let _lock = self.lock()?;
        let Some(p) = probation::read(&self.root) else { return Ok(None) };
        if !p.expired() {
            return Ok(None);
        }
        match self.undo_target()? {
            Some(target) if target.0 == p.generation => {
                self.undo_locked(target, actor, hooks).map(Some)
            }
            // Already undone some other way: the record is stale.
            _ => {
                probation::clear(&self.root)?;
                Ok(None)
            }
        }
    }

    /// Confirm the apply on probation: it stays, and applies work again.
    /// The caller checks that this is a new session
    /// ([`probation::check_new_session`]).
    pub fn keep(&self, actor: &Actor) -> Result<u64> {
        let _lock = self.lock()?;
        let p = probation::read(&self.root)
            .ok_or_else(|| Error::hint("no apply is on probation", "oath log"))?;
        if p.expired() {
            return Err(Error::hint(
                format!("generation {}'s probation ran out; PID 1 is undoing it", p.generation),
                "oath log; apply it again and keep it in time",
            ));
        }
        probation::clear(&self.root)?;
        let ids = vec![format!("keep:{}", p.generation)];
        self.append_log(actor, false, None, p.generation, &ids, "ok", None)?;
        self.write_index()?;
        Ok(p.generation)
    }

    fn undo_locked(
        &self,
        (generation, parent, ids): (u64, u64, Vec<ObjectId>),
        actor: &Actor,
        hooks: &dyn ApplyHooks,
    ) -> Result<ApplyReport> {
        let snap = hooks.snapshot_catalog(generation)?;
        let mut back = Vec::new();
        for id in &ids {
            if self.restore_spec(id, &snap, hooks)? {
                back.push(id.clone());
            }
        }
        let mut svcs = Vec::new();
        let mut report = ApplyReport::default();
        for id in &back {
            self.stop_dhcp_client_first(id, hooks)?;
            let applied = self.converge_spec(id, hooks, &mut report)?;
            if id.kind == KIND_SVC {
                svcs.push((id.clone(), applied["enabled"] == json!(true)));
            }
        }

        self.set_current_generation(parent)?;

        hooks.notify_init()?;
        for (id, enabled) in svcs {
            hooks.wait_converge(&id, enabled)?;
        }

        if probation::read(&self.root).is_some_and(|p| p.generation == generation) {
            probation::clear(&self.root)?;
        }
        let ids = vec![format!("undo:{generation}")];
        self.append_log(actor, false, Some(parent), generation, &ids, "ok", None)?;
        self.write_index()?;
        Ok(ApplyReport { generation: parent, ids, ..Default::default() })
    }

    /// Hash every linked package tree (all, or `ids`) again against the
    /// hash it is linked as. With the lock (root), the outcome goes into
    /// each actual as `verified` (a damaged package's status is `error`)
    /// and INDEX lists the damage; without it, this only reports.
    /// Returns each package with what its tree hashes to now.
    pub fn verify(&self, ids: Option<Vec<ObjectId>>) -> Result<Vec<Verified>> {
        let ids = match ids {
            Some(ids) => ids,
            None => self.ls(Some(KIND_PKG))?,
        };
        let mut out = Vec::new();
        for id in ids {
            if id.kind != KIND_PKG {
                return Err(Error::hint(format!("{id} is not a package"), "oath verify [pkg:…]"));
            }
            let obj = self.get(&id)?;
            let hash = obj.actual.get("hash").and_then(|h| h.as_str()).unwrap_or("");
            if obj.actual.get("present") != Some(&json!(true))
                || !crate::is_realization_id(hash)
                || !crate::realization_dir(&self.root, &id.name, hash).is_dir()
            {
                continue;
            }
            let v = crate::verify_tree(&self.root, &id.name, hash)?;
            out.push(Verified { id, hash: hash.to_string(), verified: v });
        }
        let Ok(_lock) = self.lock() else { return Ok(out) };
        for r in &out {
            let p = status_path(&self.root, &r.id);
            let Ok(mut actual) = read_json::<Value>(&p) else { continue };
            // Linked as something else since it was hashed: leave it.
            if actual.get("hash").and_then(|h| h.as_str()) != Some(r.hash.as_str()) {
                continue;
            }
            actual["verified"] = serde_json::to_value(&r.verified)?;
            write_json(&p, &actual)?;
        }
        self.write_index()?;
        Ok(out)
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
                if !crate::seat::is_host_name(&h.hostname, 63) {
                    return Err(Error::hint(
                        format!("{id}: hostname `{}` is not a host name", h.hostname),
                        hint(),
                    ));
                }
                for (name, addr) in &h.hosts {
                    if !crate::seat::is_host_name(name, 253) {
                        return Err(Error::hint(
                            format!("{id}: hosts: `{name}` is not a host name"),
                            hint(),
                        ));
                    }
                    if name == "localhost" || name.eq_ignore_ascii_case(&h.hostname) {
                        return Err(Error::hint(
                            format!("{id}: hosts: `{name}` is this machine; /etc/hosts has it"),
                            hint(),
                        ));
                    }
                    if addr.parse::<std::net::IpAddr>().is_err() {
                        return Err(Error::hint(
                            format!(
                                "{id}: hosts: `{name}`'s `{addr}` is not an IPv4 or IPv6 address"
                            ),
                            hint(),
                        ));
                    }
                }
                for (i, o) in h.origins.iter().enumerate() {
                    if !crate::is_origin(o) {
                        return Err(Error::hint(
                            format!(
                                "{id}: origins: `{o}` is not an origin (an http:// or https:// \
                                 prefix, no query, fragment or .tar)"
                            ),
                            hint(),
                        ));
                    }
                    if h.origins[..i].contains(o) {
                        return Err(Error::hint(format!("{id}: origins: `{o}` twice"), hint()));
                    }
                }
                if !h.profile.is_empty() && !crate::PROFILES.iter().any(|(p, _)| *p == h.profile) {
                    let names: Vec<&str> = crate::PROFILES.iter().map(|(p, _)| *p).collect();
                    return Err(Error::hint(
                        format!("{id}: profile: no seed profile `{}`", h.profile),
                        format!("profiles: {}", names.join(", ")),
                    ));
                }
                for key in &h.publishers {
                    crate::sshsig::parse_public_key(key)
                        .map_err(|e| Error::hint(format!("{id}: publishers: {e}"), hint()))?;
                }
                for (path, m) in &h.mounts {
                    let why = crate::mounts::check_path(path)
                        .or_else(|| crate::mounts::check_source(&m.source))
                        .or_else(|| crate::mounts::check_options(&m.options));
                    if let Some(why) = why {
                        return Err(Error::hint(format!("{id}: mounts: {why}"), hint()));
                    }
                }
            }
            KIND_SVC => {
                let s: Svc = serde_json::from_value(doc.clone()).map_err(bad)?;
                if s.enabled && s.exec.is_empty() {
                    return Err(Error::hint(format!("{id}: enabled with an empty exec"), hint()));
                }
                if s.rtprio.is_some_and(|p| !(1..=99).contains(&p)) {
                    return Err(Error::hint(format!("{id}: rtprio is 1 to 99"), hint()));
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
                if let Some(Err(why)) = p.source.as_ref().map(crate::PkgSource::check) {
                    return Err(Error::hint(format!("{id}: {why}"), hint()));
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
                let _: Snap = serde_json::from_value(doc.clone()).map_err(bad)?;
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
        let mut result = Ok(());
        for d in selected {
            result = if self.removing(&d.id) {
                self.converge_removal(&d.id, hooks)
            } else {
                self.converge_one(&d.id, hooks, report)
            };
            if result.is_err() {
                break;
            }
        }
        // A service follows its package: PID 1 restarts one whose program
        // now resolves to another realization. It hears once, after every
        // package in the apply has moved, failed or not.
        if selected.iter().any(|d| d.id.kind == KIND_PKG) {
            result = result.and(hooks.notify_init());
        }
        result
    }

    /// Converge `id` and wait for PID 1 when it is a service. Once its spec
    /// is applied the object is no longer new, even if the service then
    /// fails to come up: that is an error in its status, not a pending
    /// creation.
    fn converge_one(
        &self,
        id: &ObjectId,
        hooks: &dyn ApplyHooks,
        report: &mut ApplyReport,
    ) -> Result<()> {
        self.stop_dhcp_client_first(id, hooks)?;
        let applied = self.converge_spec(id, hooks, report)?;
        let _ = fs::remove_file(self.obj_dir(id).join(NEW_MARK));
        if id.kind == KIND_SVC {
            let enabled = applied.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
            hooks.notify_init()?;
            hooks.wait_converge(id, enabled)?;
        } else if id.kind == KIND_NET {
            // PID 1 runs the DHCP client of an applied dhcp net:net0.
            hooks.notify_init()?;
        }
        Ok(())
    }

    /// A `net` spec that wants no DHCP client (static, or down) is applied
    /// to PID 1 first, so it stops the client before the address changes:
    /// a running client would put its lease back.
    fn stop_dhcp_client_first(&self, id: &ObjectId, hooks: &dyn ApplyHooks) -> Result<()> {
        if id.kind != KIND_NET {
            return Ok(());
        }
        let desired = self.get(id)?.desired;
        if desired["up"] == json!(true) && desired["ipv4"] == json!("dhcp") {
            return Ok(());
        }
        write_json(&self.obj_dir(id).join(APPLIED), &desired)?;
        hooks.notify_init()
    }

    /// Converge `id` to its desired spec, record what that observed as its
    /// actual, and keep the spec as applied (`applied.json`). A service only
    /// gets its applied spec here: PID 1 runs it once notified. Returns the
    /// applied spec.
    fn converge_spec(
        &self,
        id: &ObjectId,
        hooks: &dyn ApplyHooks,
        report: &mut ApplyReport,
    ) -> Result<Value> {
        let dir = self.obj_dir(id);
        let obj = self.get(id)?;
        let desired = obj.desired;
        let applied = match id.kind.as_str() {
            KIND_HOST => {
                let mut host: Host = serde_json::from_value(desired)?;
                let reboot = host.power == HostPower::Reboot;
                let halt = host.power == HostPower::Halt;
                if reboot || halt {
                    host.power = HostPower::Run;
                    write_json(&dir.join("desired.json"), &host)?;
                }
                let previous: Option<Host> =
                    self.applied(id)?.and_then(|v| serde_json::from_value(v).ok());
                let actual = hooks.converge_host(&host, previous.as_ref())?;
                self.write_actual(id, &actual)?;
                report.rebooting |= reboot;
                report.halting |= halt;
                serde_json::to_value(&host)?
            }
            KIND_SVC => desired,
            KIND_SNAP => {
                self.write_actual(id, &desired)?;
                desired
            }
            KIND_PKG => self.converge_one_pkg(id, hooks)?,
            KIND_NET => {
                let net: Net = serde_json::from_value(desired.clone())?;
                let actual = hooks.converge_net(id, &net)?;
                self.write_actual(id, &actual)?;
                desired
            }
            KIND_SSH => {
                let ssh: Ssh = serde_json::from_value(desired.clone())?;
                let actual = hooks.converge_ssh(id, &ssh)?;
                self.write_actual(id, &actual)?;
                desired
            }
            KIND_PLAN => {
                let mut actual = obj.actual;
                if !actual.is_object() {
                    actual = json!({});
                }
                if let Some(h) = desired.get("hash") {
                    actual["hash"] = h.clone();
                }
                self.write_actual(id, &actual)?;
                desired
            }
            KIND_DEV => {
                let dev: Dev = serde_json::from_value(desired.clone())?;
                let actual = hooks.converge_dev(id, &dev)?;
                self.write_actual(id, &actual)?;
                desired
            }
            other => {
                return Err(Error::hint(
                    format!("no handler for {other}"),
                    format!("oath schema {other}"),
                ));
            }
        };
        write_json(&dir.join(APPLIED), &applied)?;
        Ok(applied)
    }

    /// An object's observation (`actual`), under `status/`.
    pub(crate) fn write_actual(&self, id: &ObjectId, actual: &impl serde::Serialize) -> Result<()> {
        write_json(&status_path(&self.root, id), actual)
    }

    /// Forget what was observed of `id` (it is gone).
    fn drop_actual(&self, id: &ObjectId) {
        let _ = fs::remove_file(status_path(&self.root, id));
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
        self.drop_actual(id);
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
        if !existed && id.kind == KIND_PKG && to.join("desired.json").is_file() {
            // A package the image fixes stays: the image's own files need it.
            if fixed(&self.get(id)?) {
                return Ok(true);
            }
        }
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
            self.drop_actual(id);
            return Ok(false);
        }
        fs::create_dir_all(&to)?;
        if !to.join("meta.json").is_file() && from.join("meta.json").is_file() {
            fs::copy(from.join("meta.json"), to.join("meta.json"))?;
        }
        // The snapshot's desired is what was applied before (apply writes
        // last-good spec into it), so it stands in for an applied.json that
        // a catalog from before applied.json lacks.
        let desired: Value = read_json(&from.join("desired.json"))?;
        let applied = if from.join(APPLIED).is_file() {
            read_json(&from.join(APPLIED))?
        } else {
            desired.clone()
        };
        write_json(&to.join(APPLIED), &applied)?;
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
        if let Some(applied) = self.applied(id)? {
            return Ok(applied);
        }
        // A catalog from before applied.json: derive it from actual.
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
                // What was applied, which actual mirrors. Actual leaves out
                // empty fields, so undo invents no `url: ""` either.
                for k in ["url", "needs", "description", "home", "source"] {
                    if let Some(v) = obj.actual.get(k) {
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

    /// Link or unlink a package. The applied spec carries the hash it was
    /// linked at, so undo can put back a pin that discovery found.
    fn converge_one_pkg(&self, id: &ObjectId, hooks: &dyn ApplyHooks) -> Result<Value> {
        let obj = self.get(id)?;
        let pkg: Pkg = serde_json::from_value(obj.desired.clone())?;
        let removable = !fixed(&obj);
        if !pkg.present && !removable {
            return Err(Error::hint(format!("{id} is not removable"), "oath schema pkg"));
        }
        let mut actual = match self.records_only(&obj)? {
            Some(live) => live,
            None => hooks.converge_pkg(id, &pkg)?,
        };
        actual.removable = removable;
        actual.url = pkg.url.clone();
        actual.needs = pkg.needs.clone();
        actual.description = pkg.description.clone();
        actual.home = pkg.home.clone();
        actual.source = pkg.source.clone();
        if actual.hash.is_empty() && !pkg.hash.is_empty() {
            actual.hash = pkg.hash.clone();
        }
        self.write_actual(id, &actual)?;
        let mut applied = obj.desired;
        if !actual.hash.is_empty() {
            applied["hash"] = json!(actual.hash);
        }
        Ok(applied)
    }

    /// The live actual of a package whose apply changes only what it
    /// records (`description`, `home`, `source`): nothing to fetch, hash or
    /// link, so a new `source` on a large pack costs nothing. None when the
    /// apply must converge it.
    fn records_only(&self, obj: &Object) -> Result<Option<PkgActual>> {
        let Some(applied) = self.applied(&obj.id)? else { return Ok(None) };
        let drift = diff_values(&obj.desired, &applied);
        let record = |k: &str| matches!(k, "description" | "home" | "source");
        if drift.is_empty() || drift.iter().any(|(k, _, _)| !record(k)) {
            return Ok(None);
        }
        let Ok(live) = serde_json::from_value::<PkgActual>(obj.actual.clone()) else {
            return Ok(None);
        };
        let pin = applied["hash"].as_str().unwrap_or("");
        let present = obj.desired["present"] != json!(false);
        Ok((!pin.is_empty() && live.hash == pin && live.present == present).then_some(live))
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
                    let linked = self.applied(&id)?.unwrap_or(obj.actual);
                    spec.hash =
                        linked.get("hash").and_then(|h| h.as_str()).unwrap_or("").to_string();
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

    /// Desired against the spec last applied. Never applied: everything.
    fn drift_fields(&self, obj: &Object) -> Result<Vec<(String, Value, Value)>> {
        if let Some(applied) = self.applied(&obj.id)? {
            return Ok(diff_values(&obj.desired, &applied));
        }
        if obj.id.kind == KIND_SVC || self.is_new(&obj.id) {
            return Ok(diff_values(&obj.desired, &Value::Null));
        }
        // A catalog from before applied.json: actual held the applied spec.
        Ok(diff_values(&obj.desired, &obj.actual))
    }

    /// The spec `id` last applied, if it has been applied.
    pub fn applied(&self, id: &ObjectId) -> Result<Option<Value>> {
        let p = self.obj_dir(id).join(APPLIED);
        if p.is_file() {
            Ok(Some(read_json(&p)?))
        } else {
            Ok(None)
        }
    }

    /// Power, the boot generation, and changes that can cut off SSH.
    fn needs_confirm(&self, d: &Drift) -> bool {
        let changed = |k: &str| d.fields.iter().find(|(f, _, _)| f == k).map(|(_, new, _)| new);
        match (d.id.kind.as_str(), d.id.name.as_str()) {
            (KIND_HOST, _) => changed("power").is_some_and(|p| p.as_str() != Some("run")),
            (KIND_SNAP, "current") => true,
            _ => self.lockout(d),
        }
    }

    /// Changes that can cut this machine off from SSH: they need
    /// `--confirm` and go on probation.
    fn lockout(&self, d: &Drift) -> bool {
        let id = &d.id;
        let changed = |k: &str| d.fields.iter().find(|(f, _, _)| f == k).map(|(_, new, _)| new);
        match (id.kind.as_str(), id.name.as_str()) {
            (KIND_SVC, "sshd") => {
                self.removing(id)
                    || changed("enabled") == Some(&json!(false))
                    || ["exec", "user", "env"].iter().any(|k| changed(k).is_some())
            }
            // Dropping a key can drop the one the owner logs in with.
            (KIND_SSH, "local") => {
                d.fields.iter().find(|(f, _, _)| f == "authorized").is_some_and(|(_, new, old)| {
                    let blobs = |v: &Value| -> Vec<String> {
                        v.as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|k| k.as_str()?.split_whitespace().nth(1))
                            .map(str::to_string)
                            .collect()
                    };
                    let kept = blobs(new);
                    kept.is_empty() || blobs(old).iter().any(|k| !kept.contains(k))
                })
            }
            (KIND_NET, "net0") => ["up", "ipv4", "gateway"].iter().any(|k| changed(k).is_some()),
            // PID 1 restarts sshd when the package it runs from moves.
            (KIND_PKG, name) => changed("hash").is_some() && self.sshd_runs_from(name),
            _ => false,
        }
    }

    /// Whether `svc:sshd`'s program resolves into `pkg:<name>`'s store.
    fn sshd_runs_from(&self, name: &str) -> bool {
        let sshd = ObjectId::new(KIND_SVC, "sshd");
        let spec = match self.applied(&sshd) {
            Ok(Some(v)) => v,
            _ => return false,
        };
        let Some(exe) = spec.pointer("/exec/0").and_then(Value::as_str) else { return false };
        crate::pkg::program_realization(&self.root, Path::new(exe)).is_some_and(|(n, _)| n == name)
    }

    fn current_generation(&self) -> Result<u64> {
        let id = ObjectId::new(KIND_SNAP, "current");
        let obj = self.get(&id)?;
        let doc = self.applied(&id)?.unwrap_or(obj.actual);
        Ok(doc.get("generation").and_then(|v| v.as_u64()).unwrap_or(0))
    }

    /// `snap:current` is `generation`, applied.
    fn set_current_generation(&self, generation: u64) -> Result<()> {
        let cur = ObjectId::new(KIND_SNAP, "current");
        let g = json!({ "generation": generation });
        write_json(&self.obj_dir(&cur).join("desired.json"), &g)?;
        write_json(&self.obj_dir(&cur).join(APPLIED), &g)?;
        self.write_actual(&cur, &g)
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
        let gens = self.root.join(".gens");
        let mut dirs = vec![gens.as_path()];
        // The machine's generation subvolumes number only its own catalog.
        if self.root == Path::new(crate::DEFAULT_ROOT) {
            dirs.push(Path::new(BTRFS_TOP));
        }
        for dir in dirs {
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
        // The record is history, so it is spec: it stays with the catalog on
        // disk and in later snapshots. Read-only: nothing sets it.
        let id = ObjectId::new(KIND_SNAP, generation.to_string());
        let dir = self.obj_dir(&id);
        fs::create_dir_all(&dir)?;
        write_json(
            &dir.join("meta.json"),
            &Meta::new(KIND_SNAP, &generation.to_string(), "mutate"),
        )?;
        write_json(&dir.join("desired.json"), &rec)?;

        self.set_current_generation(generation)
    }

    /// One line of `log/apply.jsonl`. `actor.via` names the tool that ran
    /// `oath` for the actor (`mcp:<client>`); `confirm: true` marks an apply
    /// given `--confirm`, so the log answers what an agent forced.
    #[allow(clippy::too_many_arguments)]
    fn append_log(
        &self,
        actor: &Actor,
        confirm: bool,
        generation: Option<u64>,
        parent: u64,
        ids: &[String],
        result: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let mut who = json!({ "uid": actor.uid, "tty": actor.tty });
        if let Some(via) = &actor.via {
            who["via"] = json!(via);
        }
        let mut rec = json!({
            "time": now_rfc3339(),
            "actor": who,
            "ids": ids,
            "generation": generation,
            "parent_generation": parent,
            "result": result,
            "error": error,
        });
        if confirm {
            rec["confirm"] = json!(true);
        }
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

fn is_cidr(s: &str) -> bool {
    let Some((ip, len)) = s.split_once('/') else {
        return false;
    };
    ip.parse::<std::net::Ipv4Addr>().is_ok() && len.parse::<u8>().is_ok_and(|n| n <= 32)
}
