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
    /// Local timezone (POSIX TZ). Empty = UTC. Every service and login
    /// gets `TZ` from this; the system clock stays UTC (T34).
    #[serde(default)]
    pub timezone: String,
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

/// Unix account a service runs as. Oath has these two and no others.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SvcUser {
    Root,
    Home,
}

impl SvcUser {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Home => "home",
        }
    }

    pub fn uid(self) -> u32 {
        match self {
            Self::Root => 0,
            Self::Home => crate::seat::UID,
        }
    }

    pub fn gid(self) -> u32 {
        match self {
            Self::Root => 0,
            Self::Home => crate::seat::GID,
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
    /// Absent means "not set". Effective account is [`crate::svc_spawn`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<SvcUser>,
    /// Extra environment for this process. Empty when the field is absent.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// When true, a non-root process keeps `CAP_NET_BIND_SERVICE`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bind_low_ports: bool,
}

impl Svc {
    /// Process identity. `wants`, `restart`, and `enabled` can change
    /// without bouncing a running process.
    pub fn same_spawn(&self, other: &Self) -> bool {
        self.exec == other.exec
            && self.user == other.user
            && self.env == other.env
            && self.bind_low_ports == other.bind_low_ports
    }
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

/// One runtime need: slot + realization. Hash is required (T41/T43).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PkgNeed {
    pub id: String,
    pub hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pkg {
    pub present: bool,
    /// If non-empty, apply wget's this into the store when `present`.
    #[serde(default)]
    pub url: String,
    /// Pin: realization id (`sha256-` + 64 hex). Empty = discovery.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hash: String,
    /// Other `pkg:*` this pack needs at runtime, each pinned to a
    /// realization id. Cycles and missing present needs refuse apply.
    /// Empty means none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<PkgNeed>,
    /// Short prose. Empty means none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Source or project page. Not the fetch `url`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub home: String,
}

/// One upstream component in a pack tree (`components.json`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PkgComponent {
    pub name: String,
    pub version: String,
    pub purl: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cpe: String,
}

/// One hash-keyed advisory (not part of the pack tree).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PkgAdvisory {
    pub id: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub summary: String,
}

/// Pin of a plan file. Product lives on `pkg:<name>`, not here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanActual {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hash: String,
    /// Last product tree hash. Not a proof.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub product: String,
    /// Sorted build_needs id+hash digest. Not a proof.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub inputs: String,
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
    /// Live realization (what `/bin` points at). Empty on old-layout
    /// trees that have not been hashed yet.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hash: String,
    /// Other realizations of this name still on disk.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub realizations: Vec<PkgRealization>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<PkgNeed>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub home: String,
    /// Live pack `components.json`. Empty if the file is missing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<PkgComponent>,
    /// Advisories for the live hash. `None` = never fetched (unknown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advisories: Option<Vec<PkgAdvisory>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PkgRealization {
    pub hash: String,
    pub linked: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bins: Vec<String>,
    /// First lines of pack `INDEX.md`, if present.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub index: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<PkgComponent>,
}
