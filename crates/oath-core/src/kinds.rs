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
    /// refused. A package this build's `base` profile fixes counts as fixed
    /// without it (`seed::base_fixes`).
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
    /// Where this machine fetches a pinned pack it does not have, in
    /// order: origin prefixes (`https://store.oath.moonlight.pm`), each
    /// tried as {origin}/pkg/{name}/{hash}.tar until one serves the pinned
    /// tree. A pkg's own `url` wins. Packs record no location; any origin
    /// may serve any pack, and the hash decides.
    #[serde(default)]
    pub origins: Vec<String>,
    /// The seed profile this machine follows (`desk`): `oath update` takes
    /// the newest output published for it, {origin}/out/{profile}.json.
    /// The image sets it; empty takes no updates.
    #[serde(default)]
    pub profile: String,
    /// The keys whose signed outputs `oath update` takes (R38):
    /// `ssh-ed25519 AAAA… name` lines, each a build host's signing key. A
    /// person adds one; an agent does not.
    #[serde(default)]
    pub publishers: Vec<String>,
    /// Filesystems to mount: a path under /mnt or /home, or /home itself
    /// (home on another volume, R45), to its source. PID 1 mounts each at
    /// boot and when its disk appears; apply mounts, remounts and unmounts.
    /// /home itself only at boot, before sshd and the desk, in place of
    /// `@home`: an apply says it waits for the next boot, and a missing
    /// source leaves `@home` (or `@`'s /home) with sshd's keys written. A
    /// source that is not plugged in waits, and is not an error. Status
    /// lists only what is mounted now.
    #[serde(default)]
    pub mounts: BTreeMap<String, Mount>,
    /// Kernel module parameters for this machine, by module name
    /// (`nvidia`: `NVreg_EnableGpuFirmware=0`): one machine's hardware
    /// workarounds, never a default (R34). PID 1 passes them when it loads
    /// the module, after its own defaults and before `module.key=value` on
    /// the kernel command line; a module already loaded takes a change at
    /// the next boot.
    #[serde(default)]
    pub module_options: BTreeMap<String, String>,
    /// The seat's console opens a shell as `home` without asking, and the
    /// desk starts unlocked (R42). Off, with a password set (`oath
    /// password`), the console asks for it and the desk starts locked.
    /// `svc:login` sets it, given the password; on when absent.
    #[serde(default = "default_true")]
    pub autologin: bool,
}

/// One filesystem in `host:local.mounts`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Mount {
    /// `LABEL=…`, `UUID=…`, `PARTLABEL=…` or `PARTUUID=…`. Device names
    /// (`/dev/sdb1`) move between boots and ports, so they are refused.
    pub source: String,
    /// mount(8) `-o` options (`noatime,compress=zstd:1`); empty is the
    /// filesystem's defaults.
    #[serde(default)]
    pub options: String,
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

/// Unix account a service runs as. Oath has these three and no others:
/// `void` (R36) is the unprivileged one, for a service that needs nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SvcUser {
    Root,
    Home,
    Void,
}

impl SvcUser {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Home => "home",
            Self::Void => crate::seat::VOID,
        }
    }

    pub fn uid(self) -> u32 {
        match self {
            Self::Root => 0,
            Self::Home => crate::seat::UID,
            Self::Void => crate::seat::VOID_ID,
        }
    }

    pub fn gid(self) -> u32 {
        match self {
            Self::Root => 0,
            Self::Home => crate::seat::GID,
            Self::Void => crate::seat::VOID_ID,
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
    /// The highest realtime priority (1-99) the process may give its
    /// threads (`RLIMIT_RTPRIO`); it may then also renice itself to -20
    /// (`RLIMIT_NICE`). Neither when absent. PipeWire's data threads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 99))]
    pub rtprio: Option<u8>,
    /// Whose turn on the seat (tty1, its screen and keyboard) it runs in
    /// (R42): `desk` while the desk has it, `console` while the console
    /// has it (`oath desk stop`, until `oath desk`); PID 1 holds it the
    /// rest of the time. Absent: it runs either way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat: Option<SvcSeat>,
}

/// The seat's two owners (`/oath/run/seat`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SvcSeat {
    Desk,
    Console,
}

impl SvcSeat {
    /// Whether a service with this seat runs while `now` has the seat.
    pub fn runs_on(self, now: crate::login::Seat) -> bool {
        matches!(
            (self, now),
            (Self::Desk, crate::login::Seat::Desk) | (Self::Console, crate::login::Seat::Console)
        )
    }
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
            && self.rtprio == other.rtprio
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
    /// Where the pinned tree was built from (R47), set beside `hash` by
    /// the tool that pinned it. Not part of any hash, and never a reason
    /// to move or restart anything; `oath update` writes the release's.
    /// Absent or null: none recorded (a release's own build, or an older
    /// tool).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PkgSource>,
}

/// The commit a package's tree was built from, and who built it (R47).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct PkgSource {
    /// The repository as https://<host>/<owner>/<name>, no `.git`: tools
    /// compare it to tell whether a live pin came from their own tree.
    #[schemars(regex(pattern = r"^https://\S+[^/]$"))]
    pub repo: String,
    /// The full commit id (40 hex) the tree was built from.
    #[schemars(regex(pattern = r"^[0-9a-f]{40}$"))]
    pub commit: String,
    /// The branch checked out; empty when detached.
    pub branch: String,
    /// The tree had changes not committed: `commit` does not have them.
    pub dirty: bool,
    /// user@host that built it.
    pub built_by: String,
    /// When it was built, RFC 3339 UTC.
    pub built_at: String,
}

impl PkgSource {
    /// `branch@commit` (8 digits), `dirty` noted: what a person reads.
    pub fn label(&self) -> String {
        let branch = if self.branch.is_empty() { "detached" } else { &self.branch };
        let dirty = if self.dirty { " (dirty)" } else { "" };
        format!("{branch}@{}{dirty}", &self.commit[..self.commit.len().min(8)])
    }

    /// What is wrong with it, if anything the schema's patterns forbid.
    pub fn check(&self) -> std::result::Result<(), String> {
        let hex = |s: &str| {
            s.len() == 40 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        let repo = &self.repo;
        if !repo.starts_with("https://") || repo.ends_with(".git") || repo.ends_with('/') {
            return Err(format!("source.repo is https://<host>/<owner>/<name>, not {repo:?}"));
        }
        if !hex(&self.commit) {
            return Err(format!("source.commit is 40 hex digits, not {:?}", self.commit));
        }
        if self.built_by.is_empty() || self.built_at.is_empty() {
            return Err("source.built_by and built_at are user@host and an RFC 3339 time".into());
        }
        Ok(())
    }
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
    /// The applied `source`: where the live tree was built from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PkgSource>,
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
