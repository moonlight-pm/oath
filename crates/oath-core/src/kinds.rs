use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub safety: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_generation: Option<u64>,
}

impl Meta {
    pub fn new(kind: &str, name: &str, safety: &str) -> Self {
        Self {
            id: format!("{kind}:{name}"),
            kind: kind.to_string(),
            name: name.to_string(),
            safety: safety.to_string(),
            status: "in-sync".into(),
            last_generation: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Host {
    pub hostname: String,
    pub power: HostPower,
    /// Injected into every svc spawn; `/etc/profile` is a side effect.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Display timezone (POSIX TZ). Empty = UTC. Seat processes get `TZ`
    /// from this; not `env`, not `/etc/localtime` (T34).
    #[serde(default)]
    pub timezone: String,
    /// Which graphical desk PID 1 runs. Default sola. Exclusive compositor (T39).
    #[serde(default)]
    pub session: HostSession,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostSession {
    #[default]
    Sola,
    Omarchy,
}

impl HostSession {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sola => "sola",
            Self::Omarchy => "omarchy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostPower {
    Run,
    Reboot,
    Halt,
}

impl HostPower {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Reboot => "reboot",
            Self::Halt => "halt",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Svc {
    pub exec: Vec<String>,
    #[serde(default)]
    pub wants: Vec<String>,
    pub restart: SvcRestart,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SvcRestart {
    Never,
    Always,
    OnFailure,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SvcActual {
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<i32>,
    #[serde(default)]
    pub restarts: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PkgRequires {
    /// Vulkan WSI DRM format modifiers on the connected GPU.
    /// AMD GFX6–8 and virtio-gpu do not have them (gamescope nest).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub drm_modifiers: bool,
}

impl PkgRequires {
    pub fn is_none(&self) -> bool {
        !self.drm_modifiers
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pkg {
    pub present: bool,
    /// If non-empty, apply wget's this into the store when `present`.
    #[serde(default)]
    pub url: String,
    /// Hardware the bits need. Apply `present=true` is refused if unmet.
    #[serde(default, skip_serializing_if = "PkgRequires::is_none")]
    pub requires: PkgRequires,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Net {
    pub up: bool,
    pub ipv4: String,
    #[serde(default)]
    pub gateway: String,
    /// Runtime lease when `ipv4` is `dhcp`. Not a desired field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ssh {
    #[serde(default)]
    pub authorized: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dev {
    pub present: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevActual {
    pub present: bool,
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub node: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshActual {
    #[serde(default)]
    pub authorized: Vec<String>,
    pub host_key: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PkgActual {
    pub present: bool,
    #[serde(default)]
    pub links: Vec<String>,
    /// Engine-owned. If false, `present=false` is refused (not confirm).
    #[serde(default = "default_true")]
    pub removable: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "PkgRequires::is_none")]
    pub requires: PkgRequires,
}
