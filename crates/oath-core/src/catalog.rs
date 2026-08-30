use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::error::{Error, Result};
use crate::hooks::{Actor, ApplyHooks, ApplyReport};
use crate::id::ObjectId;
use crate::kinds::{Host, HostPower, Meta, Net, Pkg};
use crate::{
    now_rfc3339, parse_gen_subvol, read_json, write_json, BTRFS_TOP, KIND_HOST, KIND_NET, KIND_PKG,
    KIND_SNAP, KIND_SVC,
};

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

impl Catalog {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self { root: root.into() })
    }

    pub fn root(&self) -> &Path {
        &self.root
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
                if e.file_type()?.is_dir() {
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
        let actual: Value = read_json(&dir.join("actual.json"))?;
        let obj = Object { id: id.clone(), meta: meta.clone(), desired, actual };
        meta.status =
            if self.drift_fields(&obj)?.is_empty() { "in-sync".into() } else { "drift".into() };
        Ok(Object { id: id.clone(), meta, desired: obj.desired, actual: obj.actual })
    }

    pub fn set_fields(&self, id: &ObjectId, fields: Map<String, Value>) -> Result<()> {
        if id.kind == KIND_SNAP && id.name != "current" {
            return Err(Error::hint(format!("{id} is read-only"), "oath schema snap"));
        }
        let mut obj = self.get(id)?;
        if id.kind == KIND_PKG
            && fields.get("present") == Some(&Value::Bool(false))
            && !pkg_removable(&obj.actual)
        {
            return Err(Error::hint(format!("{id} is not removable"), "oath schema pkg"));
        }
        let Some(map) = obj.desired.as_object_mut() else {
            return Err(Error::Msg("desired is not an object".into()));
        };
        let allowed = self.allowed_keys(&id.kind)?;
        for (k, v) in fields {
            if !allowed.iter().any(|a| a == &k) {
                return Err(Error::hint(
                    format!("unknown field `{k}` on {id}"),
                    format!("oath schema {}", id.kind),
                ));
            }
            map.insert(k, v);
        }
        write_json(&self.obj_dir(id).join("desired.json"), &obj.desired)?;
        self.touch_status(id, "drift")?;
        Ok(())
    }

    pub fn diff(&self, id: Option<&ObjectId>) -> Result<Vec<Drift>> {
        let ids = if let Some(id) = id { vec![id.clone()] } else { self.ls(None)? };
        let mut out = Vec::new();
        for id in ids {
            if id.kind == KIND_SNAP && id.name != "current" {
                continue;
            }
            let obj = self.get(&id)?;
            let fields = self.drift_fields(&obj)?;
            if !fields.is_empty() {
                out.push(Drift { id, fields });
            }
        }
        Ok(out)
    }

    pub fn apply(
        &self,
        ids: Option<Vec<ObjectId>>,
        confirm: bool,
        actor: &Actor,
        hooks: &dyn ApplyHooks,
    ) -> Result<ApplyReport> {
        let drift = self.diff(None)?;
        let selected: Vec<Drift> = if let Some(ids) = ids {
            drift.into_iter().filter(|d| ids.contains(&d.id)).collect()
        } else {
            drift
        };
        if selected.is_empty() {
            return Ok(ApplyReport::default());
        }

        let confirm_needed: Vec<String> =
            selected.iter().filter(|d| self.needs_confirm(d)).map(|d| d.id.to_string()).collect();
        if !confirm_needed.is_empty() && !confirm {
            let list = confirm_needed.join(", ");
            return Err(Error::Confirm(format!(
                "{list} needs --confirm (halt / boot-generation class). See /oath/INDEX.md safety."
            )));
        }

        let parent = self.current_generation()?;
        let generation = self.next_generation()?;

        // Snapshot the last-good tree. Pending `set` values are not last-good:
        // host → actual; svc → applied.json (last converged desired).
        let mut pending: Vec<(ObjectId, Value)> = Vec::new();
        for d in &selected {
            let obj = self.get(&d.id)?;
            pending.push((d.id.clone(), obj.desired.clone()));
            let last_good = if d.id.kind == KIND_SVC {
                let p = self.obj_dir(&d.id).join("applied.json");
                if p.is_file() {
                    read_json(&p)?
                } else {
                    obj.desired.clone()
                }
            } else if d.id.kind == KIND_PKG {
                json!({
                    "present": obj.actual.get("present").cloned().unwrap_or(json!(false))
                })
            } else {
                obj.actual.clone()
            };
            write_json(&self.obj_dir(&d.id).join("desired.json"), &last_good)?;
        }
        hooks.snapshot(generation)?;
        for (id, des) in &pending {
            write_json(&self.obj_dir(id).join("desired.json"), des)?;
        }

        self.record_generation(generation, parent, &selected)?;

        let mut report = ApplyReport {
            generation,
            ids: selected.iter().map(|d| d.id.to_string()).collect(),
            rebooting: false,
            halting: false,
        };

        for d in &selected {
            self.touch_status(&d.id, "applying")?;
            match d.id.kind.as_str() {
                KIND_HOST => {
                    let mut host: Host = serde_json::from_value(self.get(&d.id)?.desired.clone())?;
                    let reboot = host.power == HostPower::Reboot;
                    let halt = host.power == HostPower::Halt;
                    if reboot || halt {
                        host.power = HostPower::Run;
                        write_json(&self.obj_dir(&d.id).join("desired.json"), &host)?;
                    }
                    let actual = hooks.converge_host(&host)?;
                    write_json(&self.obj_dir(&d.id).join("actual.json"), &actual)?;
                    self.touch_status(&d.id, "in-sync")?;
                    if reboot {
                        report.rebooting = true;
                    }
                    if halt {
                        report.halting = true;
                    }
                }
                KIND_SVC => {
                    let obj = self.get(&d.id)?;
                    let enabled =
                        obj.desired.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
                    write_json(&self.obj_dir(&d.id).join("applied.json"), &obj.desired)?;
                    hooks.notify_init()?;
                    hooks.wait_converge(&d.id, enabled)?;
                    self.touch_status(&d.id, "in-sync")?;
                }
                KIND_SNAP => {
                    write_json(
                        &self.obj_dir(&d.id).join("actual.json"),
                        &self.get(&d.id)?.desired,
                    )?;
                    self.touch_status(&d.id, "in-sync")?;
                }
                KIND_PKG => {
                    self.converge_one_pkg(&d.id, hooks)?;
                }
                KIND_NET => {
                    let net: Net = serde_json::from_value(self.get(&d.id)?.desired.clone())?;
                    let actual = hooks.converge_net(&d.id, &net)?;
                    write_json(&self.obj_dir(&d.id).join("actual.json"), &actual)?;
                    self.touch_status(&d.id, "in-sync")?;
                }
                _ => {
                    return Err(Error::hint(
                        format!("no handler for {}", d.id.kind),
                        format!("oath schema {}", d.id.kind),
                    ));
                }
            }
        }

        self.append_log(actor, generation, parent, &report.ids, "ok", None)?;
        self.write_index()?;

        if report.rebooting {
            hooks.reboot()?;
        } else if report.halting {
            hooks.halt()?;
        }
        Ok(report)
    }

    pub fn undo(&self, actor: &Actor, hooks: &dyn ApplyHooks) -> Result<ApplyReport> {
        let last =
            self.last_ok_apply()?.ok_or_else(|| Error::hint("nothing to undo", "oath log"))?;
        let generation = last["generation"].as_u64().unwrap_or(0);
        let parent = last["parent_generation"].as_u64().unwrap_or(0);
        hooks.restore_snapshot(generation)?;

        // Catalog files come back from the snapshot. Re-apply host actual
        // from restored desired (hostname in kernel).
        if let Ok(obj) = self.get(&ObjectId::new(KIND_HOST, "local")) {
            if let Ok(host) = serde_json::from_value::<Host>(obj.desired.clone()) {
                let actual = hooks.converge_host(&host)?;
                write_json(&self.obj_dir(&obj.id).join("actual.json"), &actual)?;
            }
        }

        if let Ok(ids) = self.ls(Some(KIND_PKG)) {
            for id in ids {
                self.converge_one_pkg(&id, hooks)?;
            }
        }

        if let Ok(obj) = self.get(&ObjectId::new(KIND_NET, "net0")) {
            if let Ok(net) = serde_json::from_value::<Net>(obj.desired.clone()) {
                let actual = hooks.converge_net(&obj.id, &net)?;
                write_json(&self.obj_dir(&obj.id).join("actual.json"), &actual)?;
            }
        }

        let cur = ObjectId::new(KIND_SNAP, "current");
        let g = json!({ "generation": parent });
        write_json(&self.obj_dir(&cur).join("desired.json"), &g)?;
        write_json(&self.obj_dir(&cur).join("actual.json"), &g)?;

        hooks.notify_init()?;
        if let Ok(ids) = self.ls(Some(KIND_SVC)) {
            for id in ids {
                if let Ok(obj) = self.get(&id) {
                    let enabled =
                        obj.desired.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
                    hooks.wait_converge(&id, enabled)?;
                }
            }
        }

        let ids = vec![format!("undo:{generation}")];
        self.append_log(actor, parent, generation, &ids, "ok", None)?;
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
        fs::write(self.root.join("INDEX.md"), body)?;
        Ok(())
    }

    pub fn index_text(&self) -> Result<String> {
        let p = self.root.join("INDEX.md");
        if p.is_file() {
            Ok(fs::read_to_string(p)?)
        } else {
            crate::index::generate(self)
        }
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
        write_json(&self.obj_dir(id).join("actual.json"), &actual)?;
        self.touch_status(id, "in-sync")?;
        Ok(())
    }

    fn obj_dir(&self, id: &ObjectId) -> PathBuf {
        self.root.join("objects").join(&id.kind).join(&id.name)
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

    fn needs_confirm(&self, d: &Drift) -> bool {
        if d.id.kind == KIND_HOST {
            return d.fields.iter().any(|(k, new, _)| k == "power" && new.as_str() != Some("run"));
        }
        if d.id.kind == KIND_SNAP && d.id.name == "current" {
            return true;
        }
        false
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
        fs::write(dir.join("desired.json"), "{}\n")?;

        let cur = ObjectId::new(KIND_SNAP, "current");
        let g = json!({ "generation": generation });
        write_json(&self.obj_dir(&cur).join("desired.json"), &g)?;
        write_json(&self.obj_dir(&cur).join("actual.json"), &g)?;
        Ok(())
    }

    fn touch_status(&self, id: &ObjectId, status: &str) -> Result<()> {
        let p = self.obj_dir(id).join("meta.json");
        let mut meta: Meta = read_json(&p)?;
        meta.status = status.into();
        write_json(&p, &meta)?;
        Ok(())
    }

    fn append_log(
        &self,
        actor: &Actor,
        generation: u64,
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
        Ok(())
    }

    fn last_ok_apply(&self) -> Result<Option<Value>> {
        let lines = self.log_lines()?;
        Ok(lines.into_iter().rev().find(|v| {
            v.get("result").and_then(|r| r.as_str()) == Some("ok")
                && v.get("ids")
                    .and_then(|i| i.as_array())
                    .map(|a| a.iter().any(|x| x.as_str().is_some_and(|s| !s.starts_with("undo:"))))
                    .unwrap_or(false)
        }))
    }
}

fn pkg_removable(actual: &Value) -> bool {
    actual.get("removable").and_then(|v| v.as_bool()).unwrap_or(true)
}
