//! First catalog from a profile (`seed/*.json`), plus overlays.
//!
//! A profile names objects and their first desired document. `extends`
//! pulls in another profile first. `fixed` lists packages that can never
//! be removed. An overlay is the same shape without `extends`; each of
//! its objects is a JSON merge patch (RFC 7386) on the current desired.
//! Seeding writes spec only: status is observed by the machine (R14).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{Error, Result};
use crate::kinds::Meta;
use crate::{write_json, ObjectId, KIND_SSH, KIND_SVC};

/// Built-in profiles. `base` is a headless machine you reach over SSH;
/// `desk` adds the graphical session (rivulet, seatd) and turns the
/// screen's shell off.
pub const PROFILES: &[(&str, &str)] =
    &[("base", include_str!("../seed/base.json")), ("desk", include_str!("../seed/desk.json"))];

pub const DEFAULT_PROFILE: &str = "base";

/// Whether this build's `base` profile fixes `id`. The image's own files
/// need such a package (`/etc/passwd` names `pkg:zsh`'s shell), so it is
/// never removable, however the machine came to have it: seeded, or made
/// by `cargo make update` for an image that added it.
pub fn base_fixes(id: &ObjectId) -> bool {
    static FIXED: OnceLock<BTreeSet<String>> = OnceLock::new();
    FIXED
        .get_or_init(|| Profile::load(DEFAULT_PROFILE).map(|p| p.fixed).unwrap_or_default())
        .contains(&id.to_string())
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    #[serde(default)]
    extends: Option<String>,
    #[serde(default)]
    fixed: Vec<String>,
    #[serde(default)]
    objects: BTreeMap<String, Value>,
}

/// A resolved profile: every object's first desired document.
#[derive(Debug, Default)]
pub struct Profile {
    fixed: BTreeSet<String>,
    objects: BTreeMap<String, Value>,
}

impl Profile {
    pub fn load(name: &str) -> Result<Self> {
        Self::load_from(name, PROFILES)
    }

    fn load_from(name: &str, profiles: &[(&str, &str)]) -> Result<Self> {
        let mut seen = Vec::new();
        Self::load_inner(name, profiles, &mut seen)
    }

    /// Whether `id` (`pkg:glibc` or `glibc`) can never be removed.
    pub fn is_fixed(&self, name: &str) -> bool {
        let id = if name.contains(':') { name.to_string() } else { format!("pkg:{name}") };
        self.fixed.contains(&id)
    }

    /// Every `pkg:*` object the profile names, present or not. These and
    /// every locked pack are what `cargo make publish` puts on an origin.
    pub fn pkgs(&self) -> Vec<String> {
        self.objects.keys().filter_map(|id| id.strip_prefix("pkg:")).map(str::to_string).collect()
    }

    /// Every `pkg:*` object the profile pins with a `hash`, by name, with its
    /// spec: a pack built elsewhere (R28), not from `pkgs/`.
    pub fn pinned_pkgs(&self) -> BTreeMap<String, Value> {
        self.objects
            .iter()
            .filter(|(id, v)| {
                id.starts_with("pkg:") && v["hash"].as_str().is_some_and(|h| !h.is_empty())
            })
            .map(|(id, v)| (id["pkg:".len()..].to_string(), v.clone()))
            .collect()
    }

    /// `host:local.origins` as the profile seeds it: where its machines
    /// fetch packs, in order.
    pub fn origins(&self) -> Vec<String> {
        self.objects
            .get("host:local")
            .and_then(|h| h["origins"].as_array())
            .map(|a| a.iter().filter_map(|o| o.as_str().map(str::to_string)).collect())
            .unwrap_or_default()
    }

    /// Every object the profile seeds.
    pub fn ids(&self) -> Vec<String> {
        self.objects.keys().cloned().collect()
    }

    /// `pkg:*` objects the profile wants present. The image packs these.
    pub fn present_pkgs(&self) -> Vec<String> {
        self.objects
            .iter()
            .filter(|(id, v)| id.starts_with("pkg:") && v["present"] == json!(true))
            .map(|(id, _)| id["pkg:".len()..].to_string())
            .collect()
    }

    fn load_inner(name: &str, profiles: &[(&str, &str)], seen: &mut Vec<String>) -> Result<Self> {
        if seen.iter().any(|s| s == name) {
            return Err(Error::Msg(format!("seed profile `{name}` extends itself")));
        }
        seen.push(name.to_string());
        let Some((_, text)) = profiles.iter().find(|(n, _)| *n == name) else {
            let names: Vec<&str> = profiles.iter().map(|(n, _)| *n).collect();
            return Err(Error::hint(
                format!("no seed profile `{name}`"),
                format!("profiles: {}", names.join(", ")),
            ));
        };
        let file: ProfileFile = serde_json::from_str(text)
            .map_err(|e| Error::Msg(format!("seed profile {name}: {e}")))?;
        let mut out = match &file.extends {
            Some(parent) => Self::load_inner(parent, profiles, seen)?,
            None => Self::default(),
        };
        out.layer(file)?;
        Ok(out)
    }

    fn layer(&mut self, file: ProfileFile) -> Result<()> {
        for id in file.fixed {
            check_id(&id)?;
            self.fixed.insert(id);
        }
        for (id, patch) in file.objects {
            check_id(&id)?;
            let base = self.objects.remove(&id).unwrap_or_else(|| json!({}));
            self.objects.insert(id, merge_patch(base, &patch));
        }
        Ok(())
    }
}

/// Write `schema/<kind>.json` (generated from the types) and
/// `schema/<kind>.md` for every kind, leaving files that already match.
/// Returns the files it rewrote. PID 1 runs this at every boot, so the
/// files always describe the binaries on the machine.
pub fn write_schemas(root: &Path) -> Result<Vec<String>> {
    let dir = root.join("schema");
    std::fs::create_dir_all(&dir)?;
    let mut changed = Vec::new();
    for (kind, md) in crate::schema::KINDS {
        let json = crate::schema::text(kind).unwrap_or_default();
        for (name, body) in [(format!("{kind}.json"), json.as_str()), (format!("{kind}.md"), md)] {
            let p = dir.join(&name);
            if std::fs::read_to_string(&p).ok().as_deref() != Some(body) {
                write_text(&p, body)?;
                changed.push(name);
            }
        }
    }
    Ok(changed)
}

/// Seed the default profile.
pub fn seed(root: &Path) -> Result<()> {
    seed_profile(root, DEFAULT_PROFILE)
}

/// Write a fresh catalog at `root` from a built-in profile.
pub fn seed_profile(root: &Path, profile: &str) -> Result<()> {
    let p = Profile::load(profile)?;
    let _lock = crate::Catalog::open(root)?.lock()?;
    for d in ["schema", "objects", "log", "run"] {
        std::fs::create_dir_all(root.join(d))?;
    }
    write_schemas(root)?;
    for (id, desired) in &p.objects {
        let oid: ObjectId = id.parse()?;
        // A seeded object starts applied: the image is built that way.
        write_object(root, &oid, desired, true, p.fixed.contains(id))?;
    }
    std::fs::write(root.join("log/apply.jsonl"), "")?;
    crate::Catalog::open(root)?.write_index()?;
    Ok(())
}

/// Merge an overlay into the catalog at `root` before it first boots.
/// Each patch lands in `desired` and, for svc, host, net and ssh, counts
/// as applied (PID 1 boots from `applied.json`). Packages keep their
/// applied spec, and a package the overlay creates starts applied as
/// absent: one it wants present is linked by the first `oath apply`.
/// Returns the ids touched.
pub fn apply_overlay(root: &Path, overlay: &Value) -> Result<Vec<ObjectId>> {
    let cat = crate::Catalog::open(root)?;
    let _lock = cat.lock()?;
    let file: ProfileFile = serde_json::from_value(overlay.clone())
        .map_err(|e| Error::hint(format!("overlay: {e}"), "{\"objects\": {\"kind:name\": {…}}}"))?;
    if file.extends.is_some() || !file.fixed.is_empty() {
        return Err(Error::hint(
            "an overlay has only `objects`",
            "profiles use extends/fixed; overlays patch desired",
        ));
    }
    let mut touched = Vec::new();
    for (id, patch) in &file.objects {
        check_id(id)?;
        let oid: ObjectId = id.parse()?;
        let dir = root.join("objects").join(&oid.kind).join(&oid.name);
        let path = dir.join("desired.json");
        let exists = path.is_file();
        let base = if exists { crate::read_json(&path)? } else { json!({}) };
        let desired = merge_patch(base, patch);
        cat.validate(&oid, &desired)?;
        let boots_from_it =
            matches!(oid.kind.as_str(), KIND_SVC | crate::KIND_HOST | crate::KIND_NET | KIND_SSH);
        if exists {
            write_json(&path, &desired)?;
            if boots_from_it {
                write_json(&dir.join("applied.json"), &desired)?;
            }
        } else if oid.kind == crate::KIND_PKG {
            write_object(root, &oid, &desired, false, false)?;
            let mut nothing_linked = desired.clone();
            nothing_linked["present"] = json!(false);
            write_json(&dir.join("applied.json"), &nothing_linked)?;
        } else {
            write_object(root, &oid, &desired, true, false)?;
        }
        touched.push(oid);
    }
    cat.write_index()?;
    Ok(touched)
}

/// RFC 7386: objects merge key by key, `null` deletes, anything else replaces.
pub fn merge_patch(mut target: Value, patch: &Value) -> Value {
    let Value::Object(p) = patch else {
        return patch.clone();
    };
    if !target.is_object() {
        target = json!({});
    }
    let t = target.as_object_mut().expect("object");
    for (k, v) in p {
        if v.is_null() {
            t.remove(k);
        } else {
            let cur = t.remove(k).unwrap_or(Value::Null);
            t.insert(k.clone(), merge_patch(cur, v));
        }
    }
    target
}

fn check_id(id: &str) -> Result<()> {
    let oid: ObjectId = id.parse()?;
    let known = [
        crate::KIND_HOST,
        crate::KIND_SVC,
        crate::KIND_SNAP,
        crate::KIND_PKG,
        crate::KIND_NET,
        crate::KIND_SSH,
        crate::KIND_DEV,
        crate::KIND_PLAN,
    ];
    if !known.contains(&oid.kind.as_str()) {
        return Err(Error::hint(format!("unknown kind in `{id}`"), "oath schema"));
    }
    Ok(())
}

fn write_object(
    root: &Path,
    id: &ObjectId,
    desired: &Value,
    applied: bool,
    fixed: bool,
) -> Result<()> {
    let dir = root.join("objects").join(&id.kind).join(&id.name);
    std::fs::create_dir_all(&dir)?;
    write_json(&dir.join("desired.json"), desired)?;
    let mut meta = Meta::new(&id.kind, &id.name, "mutate");
    meta.fixed = fixed;
    write_json(&dir.join("meta.json"), &meta)?;
    if applied {
        write_json(&dir.join("applied.json"), desired)?;
    }
    Ok(())
}

fn write_text(path: &Path, body: &str) -> Result<()> {
    let mut b = body.to_string();
    if !b.ends_with('\n') {
        b.push('\n');
    }
    std::fs::write(path, b)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_profile_loads() {
        for (name, _) in PROFILES {
            let p = Profile::load(name).unwrap();
            assert!(p.objects.contains_key("host:local"), "{name}");
            assert!(p.objects.contains_key("svc:sshd"), "{name}");
        }
    }

    #[test]
    fn desk_turns_the_screen_off_and_ships_rivulet() {
        let p = Profile::load("desk").unwrap();
        assert_eq!(p.objects["svc:screen"]["enabled"], json!(false));
        assert_eq!(p.objects["svc:screen"]["user"], json!("home"));
        assert_eq!(p.objects["svc:rivulet"]["wants"], json!(["svc:seatd"]));
        let pkgs = p.present_pkgs();
        assert!(["rivulet", "seatd", "mesa", "bash"].iter().all(|n| pkgs.contains(&n.to_string())));
    }

    #[test]
    fn a_profile_extends_another() {
        let extra = r#"{"extends": "base", "objects": {
            "host:local": {"env": {"EDITOR": "vi"}},
            "pkg:extra": {"present": true},
            "pkg:later": {"present": false},
            "pkg:elsewhere": {"present": true, "hash": "sha256-0000000000000000000000000000000000000000000000000000000000000000"}
        }}"#;
        let looped = r#"{"extends": "looped"}"#;
        let profiles = [PROFILES[0], ("extra", extra), ("looped", looped)];
        let p = Profile::load_from("extra", &profiles).unwrap();
        assert!(p.objects.contains_key("pkg:bash"));
        assert_eq!(p.objects["host:local"]["hostname"], "oath");
        assert_eq!(p.objects["host:local"]["env"]["EDITOR"], "vi");
        assert!(p.fixed.contains("pkg:oath"));
        assert!(p.present_pkgs().contains(&"extra".to_string()));
        assert!(!p.present_pkgs().contains(&"util".to_string()));
        assert!(!p.present_pkgs().contains(&"later".to_string()));
        assert!(p.pkgs().contains(&"later".to_string()) && p.pkgs().contains(&"oath".to_string()));
        // Only a package the profile pins by hash is built elsewhere (R28).
        assert_eq!(p.pinned_pkgs().keys().collect::<Vec<_>>(), vec!["elsewhere"]);
        assert!(Profile::load("desk").unwrap().pinned_pkgs().is_empty());
        assert!(Profile::load_from("looped", &profiles).is_err());
        assert!(Profile::load_from("nope", &profiles).is_err());
    }

    #[test]
    fn base_is_headless() {
        let p = Profile::load("base").unwrap();
        for id in p.objects.keys() {
            for word in ["river", "sola", "wicket", "steam", "hyprland", "omarchy"] {
                assert!(!id.contains(word), "{id}");
            }
        }
    }

    #[test]
    fn base_carries_no_harness_fixtures() {
        let p = Profile::load("base").unwrap();
        for id in ["svc:hold", "pkg:hello", "pkg:fetchme"] {
            assert!(!p.objects.contains_key(id), "{id}");
        }
        let text = serde_json::to_string(&p.objects).unwrap();
        assert!(!text.contains("10.0.2."), "base names the QEMU host");
    }

    #[test]
    fn merge_patch_follows_rfc_7386() {
        let t = json!({"a": 1, "b": {"c": 2, "d": 3}, "e": [1]});
        let p = json!({"a": null, "b": {"c": 9}, "e": [2], "f": "x"});
        assert_eq!(merge_patch(t, &p), json!({"b": {"c": 9, "d": 3}, "e": [2], "f": "x"}));
    }

    #[test]
    fn overlay_patches_and_creates() {
        let d = tempfile::tempdir().unwrap();
        seed(d.path()).unwrap();
        let ov = json!({"objects": {
            "host:local": {"hostname": "canto", "timezone": "MST7MDT,M3.2.0,M11.1.0"},
            "svc:backup": {"exec": ["/lib/oath/backup-daily", "nas:/backup"], "wants": [],
                           "restart": "always", "enabled": false}
        }});
        let ids = apply_overlay(d.path(), &ov).unwrap();
        assert_eq!(ids.len(), 2);
        let host: Value =
            crate::read_json(&d.path().join("objects/host/local/desired.json")).unwrap();
        assert_eq!(host["hostname"], "canto");
        assert_eq!(host["power"], "run");
        // Counted as applied for PID 1; nothing observed yet, so no status.
        let a: Value = crate::read_json(&d.path().join("objects/svc/backup/applied.json")).unwrap();
        assert_eq!(a["enabled"], false);
        let host: Value =
            crate::read_json(&d.path().join("objects/host/local/applied.json")).unwrap();
        assert_eq!(host["hostname"], "canto");
        assert!(!d.path().join("status").exists());
    }

    #[test]
    fn overlay_refuses_profile_fields() {
        let d = tempfile::tempdir().unwrap();
        seed(d.path()).unwrap();
        assert!(apply_overlay(d.path(), &json!({"extends": "base"})).is_err());
        assert!(apply_overlay(d.path(), &json!({"objects": {"nope:x": {}}})).is_err());
    }
}
