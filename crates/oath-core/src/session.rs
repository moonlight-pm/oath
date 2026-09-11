//! Graphical desk: Sola (River) or Omarchy (Hyprland). Exclusive compositor.

use serde_json::Value;

use crate::error::{Error, Result};
use crate::id::ObjectId;
use crate::kinds::{HostSession, Svc};
use crate::{write_json, Catalog, Drift, KIND_PKG, KIND_SVC};

pub const SOLA_DESK: &[&str] =
    &["river", "sola-bus", "sola-call", "sola-river", "sola-shell", "sola-session"];

pub const OMARCHY_DESK: &[&str] = &["hyprland", "omarchy-shell"];

pub fn is_sola_desk_svc(id: &str) -> bool {
    let name = id.strip_prefix("svc:").unwrap_or(id);
    SOLA_DESK.contains(&name)
}

pub fn is_omarchy_desk_svc(id: &str) -> bool {
    let name = id.strip_prefix("svc:").unwrap_or(id);
    OMARCHY_DESK.contains(&name) || name.starts_with("omarchy-")
}

pub fn session_allows(id: &str, session: HostSession) -> bool {
    match session {
        HostSession::Sola => !is_omarchy_desk_svc(id),
        HostSession::Omarchy => !is_sola_desk_svc(id),
    }
}

pub fn flags(session: HostSession) -> Vec<(&'static str, bool)> {
    match session {
        HostSession::Sola => SOLA_DESK
            .iter()
            .map(|n| (*n, true))
            .chain(OMARCHY_DESK.iter().map(|n| (*n, false)))
            .collect(),
        HostSession::Omarchy => SOLA_DESK
            .iter()
            .map(|n| (*n, false))
            .chain(OMARCHY_DESK.iter().map(|n| (*n, true)))
            .collect(),
    }
}

impl Catalog {
    /// When `host:local.session` drifted, write the matching `svc:*`
    /// enabled flags and add those objects to this apply.
    pub fn expand_session_switch(&self, selected: &mut Vec<Drift>) -> Result<()> {
        let host_id = ObjectId::new("host", "local");
        let Some(host_drift) = selected.iter().find(|d| d.id == host_id) else {
            return Ok(());
        };
        if !host_drift.fields.iter().any(|(k, _, _)| k == "session") {
            return Ok(());
        }
        let host = self.get(&host_id)?;
        let session: HostSession = match host.desired.get("session") {
            Some(v) => serde_json::from_value(v.clone())
                .map_err(|e| Error::hint(format!("host:local.session: {e}"), "oath schema host"))?,
            None => HostSession::Sola,
        };
        if session == HostSession::Omarchy {
            self.require_hyprland_pkg()?;
        }
        for (name, enabled) in flags(session) {
            let id = ObjectId::new(KIND_SVC, name);
            let Ok(obj) = self.get(&id) else { continue };
            let mut desired = obj.desired.clone();
            let Some(map) = desired.as_object_mut() else { continue };
            let prev = map.get("enabled").cloned().unwrap_or(Value::Null);
            let next = Value::Bool(enabled);
            if prev == next {
                continue;
            }
            map.insert("enabled".into(), next.clone());
            write_json(&self.obj_dir(&id).join("desired.json"), &desired)?;
            self.touch_status(&id, "drift")?;
            if !selected.iter().any(|d| d.id == id) {
                selected.push(Drift { id, fields: vec![("enabled".into(), next, prev)] });
            }
        }
        Ok(())
    }

    pub fn check_compositor_mutex(&self) -> Result<()> {
        let river_on = svc_enabled(self, "river")?;
        let hypr_on = svc_enabled(self, "hyprland")?;
        if river_on && hypr_on {
            return Err(Error::hint(
                "svc:river and svc:hyprland cannot both be enabled (one compositor)",
                "oath set host:local session=sola|omarchy",
            ));
        }
        Ok(())
    }

    fn require_hyprland_pkg(&self) -> Result<()> {
        let id = ObjectId::new(KIND_PKG, "hyprland");
        let obj =
            self.get(&id).map_err(|_| Error::hint("pkg:hyprland is missing", "oath schema pkg"))?;
        let present = obj
            .desired
            .get("present")
            .and_then(|v| v.as_bool())
            .or_else(|| obj.actual.get("present").and_then(|v| v.as_bool()))
            .unwrap_or(false);
        if !present {
            return Err(Error::hint(
                "session=omarchy needs pkg:hyprland present",
                "oath get pkg:hyprland",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kvm_runs_on_both_desks() {
        assert!(session_allows("svc:sola-kvm", HostSession::Sola));
        assert!(session_allows("svc:sola-kvm", HostSession::Omarchy));
        assert!(!session_allows("svc:river", HostSession::Omarchy));
        assert!(!session_allows("svc:hyprland", HostSession::Sola));
    }
}

fn svc_enabled(cat: &Catalog, name: &str) -> Result<bool> {
    let id = ObjectId::new(KIND_SVC, name);
    let Ok(obj) = cat.get(&id) else {
        return Ok(false);
    };
    let spec: Svc = serde_json::from_value(obj.desired)?;
    Ok(spec.enabled)
}
