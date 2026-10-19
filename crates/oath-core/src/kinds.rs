use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// The kinds' JSON Schemas are generated from the desired types below
// (`crate::schema`): the doc comment on a type or field is the
// description an agent reads, and serde's attributes decide what is
// required and what defaults.

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub safety: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_generation: Option<u64>,
    /// A package the seed profile fixes: `present=false` and `oath rm` are
    /// refused.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fixed: bool,
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
            fixed: false,
        }
    }
}

/// This machine: its name, power state, environment, timezone and the
/// other machines it names in /etc/hosts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Host {
    /// Linux hostname: letters, digits, `-` and `.`, at most 63. Source of
    /// truth is this object, not /etc/hostname.
    #[schemars(length(min = 1, max = 63), regex(pattern = r"^[A-Za-z0-9.-]+$"))]
    pub hostname: String,
    /// `run` is steady state. `reboot` and `halt` need --confirm on apply.
    pub power: HostPower,
    /// Environment PID 1 puts in every svc spawn. Also written to
    /// root-owned /etc/profile for logins (a side effect; the catalog is
    /// truth).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Local timezone (POSIX TZ string). Empty is UTC. Every svc and every
    /// login gets TZ from this unless env sets TZ. The system clock stays
    /// UTC.
    #[serde(default)]
    pub timezone: String,
    /// Other machines by name: host name to IPv4 or IPv6 address, written
    /// to /etc/hosts after localhost and this hostname (a side effect; the
    /// catalog is truth). For names the network's DNS does not serve.
    #[serde(default)]
    pub hosts: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

/// A process PID 1 supervises.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Svc {
    /// argv; absolute path in exec[0]. May be empty only while disabled.
    pub exec: Vec<String>,
    /// Other svc ids that should be up first (ordering, not Requires).
    #[serde(default)]
    pub wants: Vec<String>,
    /// When PID 1 starts it again after it exits.
    #[serde(default = "default_restart")]
    pub restart: SvcRestart,
    /// If false, the process is not running.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Unix account: root or home (uid 1). Absent means root. Does not
    /// create accounts.
    // The effective account is `crate::svc_spawn`'s.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<SvcUser>,
    /// Extra environment for this process. Empty when absent. Wins over
    /// host:local.env on the same name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// When true, a non-root process keeps CAP_NET_BIND_SERVICE and may
    /// bind ports below 1024. Root does not need this.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bind_low_ports: bool,
    /// When true, stop and restart signal only the main process and its
    /// children keep running (sshd keeps login sessions). Otherwise the
    /// whole process tree (the service's cgroup) stops.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub keep_children: bool,
}

fn default_restart() -> SvcRestart {
    SvcRestart::Always
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

/// One runtime need: a package and the realization it needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct PkgNeed {
    /// pkg:* id (pkg:libfoo, or libfoo).
    pub id: String,
    /// Realization id sha256-<64 hex> of the needed pack. Apply refuses if
    /// the needed pack's live hash differs.
    #[schemars(regex(pattern = r"^sha256-[0-9a-f]{64}$"))]
    pub hash: String,
}

/// A package: a slot in the store whose linked tree's bin/ is symlinked
/// into /bin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Pkg {
    /// If true, files in /oath/store/pkg/<name>/<hash>/bin/ are symlinked
    /// at /bin/<basename>. If false, those links are removed. The store
    /// tree stays.
    #[serde(default = "default_true")]
    pub present: bool,
    /// If non-empty, apply fetches this URL into the store when present
    /// (the local store wins if the tree is already there). A .tar URL is
    /// a pack tree; an origin prefix plus hash fetches
    /// {url}/pkg/{name}/{hash}.tar; with `file`, the URL is one file.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    /// With `url`: the URL is one file, a vendor's release (R23), and the
    /// tree is that file alone at this relative path (`src/claude`), not
    /// executable. A recipe's source package; `hash` pins the tree.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    /// Pin: realization id sha256-<64 hex> of the pack tree (oath-tree-v1).
    /// Apply verifies it. Empty is discovery (a fetch or the unique
    /// on-disk tree reports actual.hash).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[schemars(regex(pattern = r"^(sha256-[0-9a-f]{64})?$"))]
    pub hash: String,
    /// Runtime needs, each pinned to a realization. Cycles refuse apply.
    /// present=true is refused if a need is missing or its live hash does
    /// not match; present=false is refused while another present pack
    /// still needs this. Apply does not rewrite other objects.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<PkgNeed>,
    /// Short prose for humans and agents. Empty means none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// HTTPS URL of the source tree or main project page. Not the fetch
    /// url.
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

/// The pin of a plan file. The product lives on pkg:<name>, not here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Plan {
    /// sha256- plus 64 lowercase hex of the canonical plan.plan file.
    /// Empty means unpinned (oath build plan:<name> refuses).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[schemars(regex(pattern = r"^(sha256-[0-9a-f]{64})?$"))]
    pub hash: String,
}

/// A generation: snap:current names the live one; snap:N records one
/// apply's snapshot (read-only).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Snap {
    /// For snap:current, the last snapshot apply created. For snap:N,
    /// that generation's id.
    pub generation: u64,
    /// Parent generation (snap:N only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<u64>,
    /// When the snapshot was taken (snap:N only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// Why (snap:N only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
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

/// The network link net0 (the machine's one NIC).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Net {
    /// If true, net0 is up. If false, the link is down.
    pub up: bool,
    /// IPv4 CIDR (static) or the string dhcp.
    #[schemars(regex(pattern = r"^(dhcp|[0-9]{1,3}(\.[0-9]{1,3}){3}/[0-9]{1,2})$"))]
    pub ipv4: String,
    /// IPv4 default gateway. Ignored when ipv4 is dhcp.
    #[serde(default)]
    pub gateway: String,
    /// Runtime lease when `ipv4` is `dhcp`. Not a desired field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub lease: Option<String>,
    /// The DHCP lease as the client last bound or renewed it. Actual only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub dhcp: Option<DhcpLease>,
}

/// What the DHCP server last handed out (`net:net0` actual `dhcp`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DhcpLease {
    /// `a.b.c.d/n`.
    pub address: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub router: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dns: Vec<String>,
    /// Lease length the server gave, in seconds.
    #[serde(default)]
    pub seconds: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub server: String,
    /// `bound` (a new lease) or `renew`.
    pub event: String,
    /// When the client got it, RFC 3339.
    pub at: String,
}

/// Who may log in over SSH (as home).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Ssh {
    /// OpenSSH public key lines that may log in as home (each starts ssh-,
    /// ecdsa- or sk-). Empty means nobody can log in over SSH. No
    /// passwords, and no private key in the catalog.
    #[serde(default)]
    #[schemars(inner(regex(pattern = r"^(ssh-|ecdsa-|sk-)")))]
    pub authorized: Vec<String>,
}

/// A hardware device (inventory).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Dev {
    /// Whether this hardware is in the inventory. The seeded devices
    /// cannot be set false.
    pub present: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevActual {
    pub present: bool,
    /// `block`, `net` or `tty`.
    #[serde(default)]
    pub class: String,
    /// The node or sysfs entry (`/dev/sda`, `/sys/class/net/net0`).
    #[serde(default)]
    pub node: String,
    /// Where it sits on the buses (`pci-0000:00:1f.2-ata-1`): what its
    /// name is made from.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    /// The kernel driver bound to it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub driver: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// Bytes, for a disk.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub size: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removable: bool,
    /// Hardware address, for a NIC.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mac: String,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
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
    /// The last time the linked tree was hashed again against `hash`
    /// (apply, `oath verify`, boot).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified: Option<PkgVerified>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PkgVerified {
    /// The tree still hashes to `hash`.
    pub ok: bool,
    /// RFC 3339.
    pub at: String,
    /// What the tree hashes to now, when that is not `hash`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub got: String,
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
