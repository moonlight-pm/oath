//! Kernel modules by modalias, in PID 1.
//!
//! The image ships every module the kernel config builds, with depmod's
//! index (`modules.dep`, `modules.alias`, `modules.softdep`,
//! `modules.builtin`). [`Index::coldplug`] loads what the devices in sysfs
//! ask for. The kernel asks for the rest (a filesystem: `fs-btrfs`, a code
//! page: `nls_cp437`) through `/lib/oath/modprobe`, which is this binary
//! ([`modprobe_main`]).

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nix::errno::Errno;
use nix::kmod::{finit_module, ModuleInitFlags};

/// The kernel's modprobe helper (`/proc/sys/kernel/modprobe`): a link to
/// this binary in the initrd and in the root.
pub const HELPER: &str = "/lib/oath/modprobe";

/// Parameters a module always gets. amdgpu drives Southern and Sea
/// Islands cards only when asked; NVIDIA's DRM driver does kernel
/// modesetting, which a Wayland compositor needs, only when asked.
const DEFAULT_PARAMS: &[(&str, &str)] =
    &[("amdgpu", "si_support=1 cik_support=1"), ("nvidia_drm", "modeset=1 fbdev=1")];

/// Coldplug passes: loading a bus driver (virtio_pci, xhci) makes new
/// devices appear, which ask for their own drivers.
const PASSES: usize = 8;

pub fn base() -> PathBuf {
    let kver = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    Path::new("/lib/modules").join(kver.trim())
}

/// Module trees packages bring for kernel `kver` (R34: NVIDIA's): the
/// `lib/modules/<kver>/` with an index of every package whose applied
/// spec is present, but pkg:linux, whose tree is the kernel's own
/// ([`base`]). In the initrd there is no catalog, so none.
pub fn package_bases(oath: &Path, kver: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(oath.join("objects/pkg")) else { return out };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name == "linux" {
            continue;
        }
        let Some(spec) = read_json(&e.path().join("applied.json")) else { continue };
        if spec.get("present").and_then(|p| p.as_bool()) == Some(false) {
            continue;
        }
        let Some(hash) = spec.get("hash").and_then(|h| h.as_str()) else { continue };
        let dir = oath.join("store/pkg").join(&name).join(hash).join("lib/modules").join(kver);
        if dir.join("modules.dep").is_file() {
            out.push(dir);
        }
    }
    out.sort();
    out
}

/// Where package firmware is linked; the kernel's firmware path
/// (`firmware_class.path`), searched before `/lib/firmware`.
pub const PACKAGE_FIRMWARE: &str = "/run/oath/firmware";

/// Firmware packages bring (R34: NVIDIA's GSP firmware): each present
/// package's `lib/firmware`, but pkg:firmware's and pkg:linux's, which
/// are `/lib/firmware` already.
pub fn package_firmware(oath: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(oath.join("objects/pkg")) else { return out };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name == "linux" || name == "firmware" {
            continue;
        }
        let Some(spec) = read_json(&e.path().join("applied.json")) else { continue };
        if spec.get("present").and_then(|p| p.as_bool()) == Some(false) {
            continue;
        }
        let Some(hash) = spec.get("hash").and_then(|h| h.as_str()) else { continue };
        let dir = oath.join("store/pkg").join(&name).join(hash).join("lib/firmware");
        if dir.is_dir() {
            out.push(dir);
        }
    }
    out.sort();
    out
}

/// Link each top-level entry of each package firmware dir into `into`
/// (`nvidia` → `<store>/nvidia/<hash>/lib/firmware/nvidia`); the first
/// package to name an entry has it. A link whose package moved or left
/// goes. Whether any link is there.
pub fn link_package_firmware(dirs: &[PathBuf], into: &Path) -> bool {
    let mut want: HashMap<std::ffi::OsString, PathBuf> = HashMap::new();
    for dir in dirs {
        for e in fs::read_dir(dir).into_iter().flatten().flatten() {
            want.entry(e.file_name()).or_insert_with(|| e.path());
        }
    }
    for e in fs::read_dir(into).into_iter().flatten().flatten() {
        if want.get(&e.file_name()) != fs::read_link(e.path()).ok().as_ref() {
            let _ = fs::remove_file(e.path());
        }
    }
    if want.is_empty() {
        return false;
    }
    let _ = fs::create_dir_all(into);
    for (name, target) in &want {
        let link = into.join(name);
        if fs::read_link(&link).ok().as_ref() != Some(target) {
            let _ = std::os::unix::fs::symlink(target, &link);
        }
    }
    true
}

/// `host:local.module_options` as applied: parameters by module name.
pub fn host_module_options(oath: &Path) -> HashMap<String, String> {
    read_json(&oath.join("objects/host/local/applied.json"))
        .and_then(|spec| spec.get("module_options").and_then(|o| o.as_object()).cloned())
        .map(|o| {
            o.into_iter().filter_map(|(m, p)| Some((norm(&m), p.as_str()?.to_string()))).collect()
        })
        .unwrap_or_default()
}

fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// depmod's view of the module tree.
pub struct Index {
    base: PathBuf,
    /// name → path under `base`
    paths: HashMap<String, String>,
    /// name → names it needs (`modules.dep`)
    deps: HashMap<String, Vec<String>>,
    /// (pattern, name), in file order
    aliases: Vec<(String, String)>,
    pre: HashMap<String, Vec<String>>,
    post: HashMap<String, Vec<String>>,
    builtin: HashSet<String>,
    cmdline: String,
    /// `host:local.module_options`: one machine's parameters, by module.
    options: HashMap<String, String>,
}

/// A module's name from its path: `kernel/fs/nls/nls_iso8859-1.ko` →
/// `nls_iso8859_1`. The kernel treats `-` and `_` alike in names.
pub fn name_of(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path);
    norm(file.split(".ko").next().unwrap_or(file))
}

fn norm(name: &str) -> String {
    name.replace('-', "_")
}

/// Modules no device asks for. `tun` is how a rootless container gets its
/// network (R59), and a process that is not in a user namespace of its own
/// can do nothing with it.
const WANTED: &[&str] = &["tun"];

impl Index {
    pub fn open(base: &Path) -> Option<Self> {
        let read = |f: &str| fs::read_to_string(base.join(f)).unwrap_or_default();
        let dep = fs::read_to_string(base.join("modules.dep")).ok()?;
        let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();
        let mut idx = Self::parse(
            base,
            &dep,
            &read("modules.alias"),
            &read("modules.softdep"),
            &read("modules.builtin"),
            &cmdline,
        );
        let oath = Path::new("/oath");
        let kver = base.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        for dir in package_bases(oath, &kver) {
            let read = |f: &str| fs::read_to_string(dir.join(f)).unwrap_or_default();
            idx.merge(Self::parse(
                &dir,
                &read("modules.dep"),
                &read("modules.alias"),
                &read("modules.softdep"),
                "",
                &cmdline,
            ));
        }
        idx.options = host_module_options(oath);
        Some(idx)
    }

    /// Add a package's modules (R34): their paths are absolute, their
    /// needs are names, which may be the kernel's own modules.
    fn merge(&mut self, other: Self) {
        for (name, rel) in other.paths {
            let path = other.base.join(rel).to_string_lossy().into_owned();
            self.paths.entry(name).or_insert(path);
        }
        for (name, needs) in other.deps {
            self.deps.entry(name).or_insert(needs);
        }
        self.aliases.extend(other.aliases);
        for (name, m) in other.pre {
            self.pre.entry(name).or_default().extend(m);
        }
        for (name, m) in other.post {
            self.post.entry(name).or_default().extend(m);
        }
    }

    pub fn parse(
        base: &Path,
        dep: &str,
        alias: &str,
        softdep: &str,
        builtin: &str,
        cmdline: &str,
    ) -> Self {
        let mut paths = HashMap::new();
        let mut deps = HashMap::new();
        for line in dep.lines() {
            let Some((path, rest)) = line.split_once(':') else { continue };
            let name = name_of(path.trim());
            paths.insert(name.clone(), path.trim().to_string());
            deps.insert(name, rest.split_whitespace().map(name_of).collect());
        }
        let aliases = alias
            .lines()
            .filter_map(|l| {
                let mut w = l.split_whitespace();
                (w.next() == Some("alias")).then_some(())?;
                Some((w.next()?.to_string(), norm(w.next()?)))
            })
            .collect();
        let (mut pre, mut post) = (HashMap::new(), HashMap::new());
        for line in softdep.lines() {
            let mut w = line.split_whitespace();
            if w.next() != Some("softdep") {
                continue;
            }
            let Some(name) = w.next().map(norm) else { continue };
            let mut into: Option<&mut HashMap<String, Vec<String>>> = None;
            for tok in w {
                match tok {
                    "pre:" => into = Some(&mut pre),
                    "post:" => into = Some(&mut post),
                    m => {
                        if let Some(map) = into.as_deref_mut() {
                            map.entry(name.clone()).or_default().push(norm(m));
                        }
                    }
                }
            }
        }
        Self {
            base: base.to_path_buf(),
            paths,
            deps,
            aliases,
            pre,
            post,
            builtin: builtin.lines().map(name_of).collect(),
            cmdline: cmdline.to_string(),
            options: HashMap::new(),
        }
    }

    /// Modules for a name or an alias (`pci:v00008086d…`, `fs-btrfs`).
    pub fn resolve(&self, wanted: &str) -> Vec<String> {
        let name = norm(wanted);
        if self.paths.contains_key(&name) || self.builtin.contains(&name) {
            return vec![name];
        }
        let mut out: Vec<String> = Vec::new();
        for (pattern, m) in &self.aliases {
            if glob(pattern.as_bytes(), wanted.as_bytes()) && !out.contains(m) {
                out.push(m.clone());
            }
        }
        out
    }

    /// Nodes for modules that are not loaded yet, as kmod's `static-nodes`
    /// finds them: a module with a `devname:` alias and a fixed
    /// `char-major-M-m` (or `block-major-M-m`) one. Its node can be in `/dev`
    /// before it loads, and the first open asks the kernel for that alias
    /// (`/dev/uhid`, BlueZ's LE input; `/dev/uinput`; `/dev/fuse`).
    pub fn static_nodes(&self) -> Vec<StaticNode> {
        let mut out = Vec::new();
        for (pattern, m) in &self.aliases {
            let Some(name) = pattern.strip_prefix("devname:") else { continue };
            if name.is_empty() || name.split('/').any(|c| c.is_empty() || c == "." || c == "..") {
                continue;
            }
            let dev = self.aliases.iter().filter(|(_, n)| n == m).find_map(|(p, _)| {
                let (block, rest) = match p.strip_prefix("char-major-") {
                    Some(r) => (false, r),
                    None => (true, p.strip_prefix("block-major-")?),
                };
                let (major, minor) = rest.split_once('-')?;
                Some((block, major.parse().ok()?, minor.parse().ok()?))
            });
            if let Some((block, major, minor)) = dev {
                out.push(StaticNode { name: name.to_string(), block, major, minor });
            }
        }
        out
    }

    /// Display drivers that replace the firmware framebuffer, and sound,
    /// wait until the network and sshd are up: on canto an early amdgpu
    /// hangs the NIC.
    pub fn deferred(&self, name: &str) -> bool {
        self.paths
            .get(name)
            .is_some_and(|p| crate::gpu::takes_over_firmware_fb(p) || p.contains("/sound/"))
    }

    /// Defaults, the machine's `host:local.module_options`, then
    /// `name.key=value` from the kernel command line (kmod does the same;
    /// the kernel only applies those to built-ins). A later value wins.
    fn params(&self, name: &str) -> String {
        let mut out: Vec<String> = DEFAULT_PARAMS
            .iter()
            .filter(|(m, _)| *m == name)
            .map(|(_, p)| (*p).to_string())
            .collect();
        out.extend(self.options.get(name).cloned());
        for tok in self.cmdline.split_whitespace() {
            if let Some((m, kv)) = tok.split_once('.') {
                if norm(m) == name && kv.contains('=') {
                    out.push(kv.to_string());
                }
            }
        }
        out.join(" ")
    }

    /// Load `name` after what it needs. `Ok(true)` if this call loaded it.
    pub fn load(&self, name: &str, seen: &mut HashSet<String>) -> Result<bool, String> {
        if !seen.insert(name.to_string()) || self.builtin.contains(name) || is_loaded(name) {
            return Ok(false);
        }
        let path = self.paths.get(name).ok_or_else(|| format!("no module {name}"))?;
        for m in self.pre.get(name).into_iter().flatten() {
            let _ = self.load(m, seen);
        }
        for m in self.deps.get(name).into_iter().flatten() {
            self.load(m, seen)?;
        }
        let file = fs::File::open(self.base.join(path)).map_err(|e| format!("{path}: {e}"))?;
        let params = CString::new(self.params(name)).map_err(|e| e.to_string())?;
        // The boot screen lets go before a display driver takes it; any
        // other driver that hangs in its init leaves its name there.
        if crate::gpu::drives_display(path) {
            crate::splash::release();
        } else {
            crate::splash::say(&format!("Loading drivers: {name}"));
        }
        match finit_module(&file, &params, ModuleInitFlags::empty()) {
            Ok(()) | Err(Errno::EEXIST) => {}
            Err(e) => return Err(format!("{name}: {e}")),
        }
        for m in self.post.get(name).into_iter().flatten() {
            let _ = self.load(m, seen);
        }
        Ok(true)
    }

    /// Load the drivers the devices in sysfs ask for, pass after pass
    /// until no new module loads. With `defer`, display and sound wait.
    /// Returns what loaded and what failed.
    pub fn coldplug(&self, defer: bool) -> (Vec<String>, Vec<String>) {
        let mut tried = HashSet::new();
        let (mut loaded, mut failed) = (Vec::new(), Vec::new());
        for m in WANTED.iter().filter(|m| self.paths.contains_key(**m)) {
            match self.load(m, &mut HashSet::new()) {
                Ok(true) => loaded.push(m.to_string()),
                Ok(false) => {}
                Err(e) => failed.push(e),
            }
        }
        for _ in 0..PASSES {
            let mut seen = HashSet::new();
            let mut new = false;
            for alias in device_aliases(Path::new("/sys/devices")) {
                if !tried.insert(alias.clone()) {
                    continue;
                }
                for m in self.resolve(&alias) {
                    if defer && self.deferred(&m) {
                        continue;
                    }
                    match self.load(&m, &mut seen) {
                        Ok(true) => {
                            new = true;
                            loaded.push(m);
                        }
                        Ok(false) => {}
                        Err(e) => failed.push(e),
                    }
                }
            }
            if !new {
                break;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        (loaded, failed)
    }
}

/// A node under `/dev` whose module loads when it is first opened.
pub struct StaticNode {
    /// Under `/dev`: `uhid`, `net/tun`, `snd/seq`.
    pub name: String,
    pub block: bool,
    pub major: u32,
    pub minor: u32,
}

fn is_loaded(name: &str) -> bool {
    Path::new("/sys/module").join(name).join("initstate").exists()
}

/// Every `modalias` under `root`, without following links.
fn device_aliases(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                stack.push(e.path());
            } else if ft.is_file() && e.file_name() == "modalias" {
                if let Ok(s) = fs::read_to_string(e.path()) {
                    let s = s.trim();
                    if !s.is_empty() {
                        out.push(s.to_string());
                    }
                }
            }
        }
    }
    out
}

/// fnmatch(3) without flags: `*`, `?` and `[…]` (ranges, `!` or `^`).
fn glob(p: &[u8], s: &[u8]) -> bool {
    let (mut pi, mut si) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while si < s.len() {
        if pi < p.len() {
            match p[pi] {
                b'*' => {
                    star = Some((pi, si));
                    pi += 1;
                    continue;
                }
                b'?' => {
                    pi += 1;
                    si += 1;
                    continue;
                }
                b'[' => {
                    if let Some((hit, end)) = class(&p[pi..], s[si]) {
                        if hit {
                            pi += end;
                            si += 1;
                            continue;
                        }
                    } else if s[si] == b'[' {
                        pi += 1;
                        si += 1;
                        continue;
                    }
                }
                c if c == s[si] => {
                    pi += 1;
                    si += 1;
                    continue;
                }
                _ => {}
            }
        }
        match star {
            Some((sp, ss)) => {
                pi = sp + 1;
                si = ss + 1;
                star = Some((sp, ss + 1));
            }
            None => return false,
        }
    }
    p[pi..].iter().all(|&c| c == b'*')
}

/// A `[…]` class at the start of `p`: whether `c` is in it, and its length.
fn class(p: &[u8], c: u8) -> Option<(bool, usize)> {
    let mut i = 1;
    let negate = matches!(p.get(i), Some(b'!' | b'^'));
    if negate {
        i += 1;
    }
    let mut hit = false;
    let mut first = true;
    while i < p.len() {
        if p[i] == b']' && !first {
            return Some((hit != negate, i + 1));
        }
        first = false;
        if i + 2 < p.len() && p[i + 1] == b'-' && p[i + 2] != b']' {
            hit |= p[i] <= c && c <= p[i + 2];
            i += 3;
        } else {
            hit |= p[i] == c;
            i += 1;
        }
    }
    None
}

/// The kernel runs `/lib/oath/modprobe -q -- <name>` when it wants a
/// module (`request_module`). Exit 0 if something loaded or already was.
pub fn modprobe_main(args: &[String]) -> i32 {
    let names: Vec<&String> = match args.iter().position(|a| a == "--") {
        Some(i) => args[i + 1..].iter().collect(),
        None => args.iter().filter(|a| !a.starts_with('-')).collect(),
    };
    if args.iter().take_while(|a| *a != "--").any(|a| a == "-r" || a == "--remove") {
        return remove(&names);
    }
    let Some(idx) = Index::open(&base()) else { return 1 };
    let mut ok = false;
    for n in names {
        for m in idx.resolve(n) {
            let mut seen = HashSet::new();
            match idx.load(&m, &mut seen) {
                Ok(_) => ok = true,
                Err(e) => eprintln!("modprobe: {e}"),
            }
        }
    }
    if ok {
        0
    } else {
        1
    }
}

/// `modprobe -r`: unload modules by name (`-` and `_` alike). One that is
/// in use stays, and says so.
fn remove(names: &[&String]) -> i32 {
    let mut code = 0;
    for n in names {
        let Ok(c) = std::ffi::CString::new(n.replace('-', "_")) else { continue };
        if unsafe { libc::syscall(libc::SYS_delete_module, c.as_ptr(), libc::O_NONBLOCK) } != 0 {
            eprintln!("modprobe: remove {n}: {}", std::io::Error::last_os_error());
            code = 1;
        }
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEP: &str = "\
kernel/fs/btrfs/btrfs.ko: kernel/lib/raid6/raid6_pq.ko kernel/crypto/xor.ko
kernel/lib/raid6/raid6_pq.ko:
kernel/crypto/xor.ko:
kernel/drivers/net/ethernet/realtek/r8169.ko: kernel/drivers/net/phy/libphy.ko
kernel/drivers/net/phy/libphy.ko:
kernel/drivers/net/phy/realtek/realtek.ko: kernel/drivers/net/phy/libphy.ko
kernel/fs/nls/nls_iso8859-1.ko:
kernel/drivers/gpu/drm/amd/amdgpu/amdgpu.ko:
kernel/sound/hda/controllers/snd-hda-intel.ko:
kernel/drivers/net/ethernet/broadcom/tg3.ko:
";
    const ALIAS: &str = "\
# Aliases extracted from modules themselves.
alias fs-btrfs btrfs
alias pci:v000014E4d00001682sv*sd*bc*sc*i* tg3
alias pci:v000010ECd00008168sv*sd*bc*sc*i* r8169
alias pci:v00001002d*sv*sd*bc03sc*i* amdgpu
alias pci:v*d*sv*sd*bc04sc0[13]i* snd_hda_intel
";

    fn idx() -> Index {
        Index::parse(
            Path::new("/lib/modules/x"),
            DEP,
            ALIAS,
            "softdep r8169 pre: realtek\n",
            "kernel/net/packet/af_packet.ko\n",
            "quiet amdgpu.dc=1 tg3.x=y=z oath.root=/dev/sda2",
        )
    }

    #[test]
    fn names_from_paths() {
        assert_eq!(name_of("kernel/fs/nls/nls_iso8859-1.ko"), "nls_iso8859_1");
        assert_eq!(name_of("kernel/drivers/x/virtio-gpu.ko.xz"), "virtio_gpu");
    }

    #[test]
    fn resolves_names_aliases_and_builtins() {
        let i = idx();
        assert_eq!(i.resolve("fs-btrfs"), ["btrfs"]);
        assert_eq!(i.resolve("nls_iso8859-1"), ["nls_iso8859_1"]);
        assert_eq!(i.resolve("af_packet"), ["af_packet"]);
        assert_eq!(i.resolve("pci:v000014E4d00001682sv0000106Bsd000000F5bc02sc00i00"), ["tg3"]);
        assert_eq!(i.resolve("pci:v00001002d00006810sv00001002sd00000B00bc03sc00i00"), ["amdgpu"]);
        assert_eq!(
            i.resolve("pci:v00001002d0000AAB0sv00001002sd0000AAB0bc04sc03i00"),
            ["snd_hda_intel"]
        );
        assert!(i.resolve("pci:v00001002d0000AAB0sv00001002sd0000AAB0bc04sc02i00").is_empty());
        assert!(i.resolve("fs-ext4").is_empty());
    }

    #[test]
    fn display_and_sound_wait() {
        let i = idx();
        assert!(i.deferred("amdgpu"));
        assert!(i.deferred("snd_hda_intel"));
        assert!(!i.deferred("tg3"));
        assert!(!i.deferred("btrfs"));
    }

    #[test]
    fn params_from_defaults_and_cmdline() {
        let i = idx();
        assert_eq!(i.params("amdgpu"), "si_support=1 cik_support=1 dc=1");
        assert_eq!(i.params("tg3"), "x=y=z");
        assert_eq!(i.params("btrfs"), "");
    }

    #[test]
    fn softdeps_and_deps_parse() {
        let i = idx();
        assert_eq!(i.pre["r8169"], ["realtek"]);
        assert_eq!(i.deps["btrfs"], ["raid6_pq", "xor"]);
        assert!(i.builtin.contains("af_packet"));
    }

    #[test]
    fn package_modules_merge_by_name() {
        let mut i = idx();
        i.merge(Index::parse(
            Path::new("/oath/store/pkg/nvidia/sha256-x/lib/modules/x"),
            "nvidia.ko:\nnvidia-drm.ko: nvidia-modeset.ko libphy.ko\nnvidia-modeset.ko: nvidia.ko\n",
            "alias pci:v000010DEd*sv*sd*bc03sc00i00 nvidia\n",
            "softdep nvidia post: nvidia_modeset nvidia_drm\n",
            "",
            "",
        ));
        assert_eq!(
            i.paths["nvidia_drm"],
            "/oath/store/pkg/nvidia/sha256-x/lib/modules/x/nvidia-drm.ko"
        );
        assert_eq!(i.paths["tg3"], "kernel/drivers/net/ethernet/broadcom/tg3.ko");
        assert_eq!(i.deps["nvidia_drm"], ["nvidia_modeset", "libphy"]);
        assert_eq!(i.post["nvidia"], ["nvidia_modeset", "nvidia_drm"]);
        assert_eq!(i.resolve("pci:v000010DEd00002203sv00001043sd000088A9bc03sc00i00"), ["nvidia"]);
        assert!(i.deferred("nvidia") && i.deferred("nvidia_drm"));
    }

    #[test]
    fn package_bases_are_present_packages_but_linux() {
        let d = tempfile::tempdir().unwrap();
        let o = d.path();
        let pkg = |name: &str, spec: &str, tree: bool| {
            fs::create_dir_all(o.join("objects/pkg").join(name)).unwrap();
            fs::write(o.join("objects/pkg").join(name).join("applied.json"), spec).unwrap();
            if tree {
                let m = o.join("store/pkg").join(name).join("sha256-a/lib/modules/7.3.0");
                fs::create_dir_all(&m).unwrap();
                fs::write(m.join("modules.dep"), "").unwrap();
            }
        };
        pkg("nvidia", r#"{"present": true, "hash": "sha256-a"}"#, true);
        pkg("gone", r#"{"present": false, "hash": "sha256-a"}"#, true);
        pkg("linux", r#"{"present": true, "hash": "sha256-a"}"#, true);
        pkg("mesa", r#"{"present": true, "hash": "sha256-a"}"#, false);
        assert_eq!(
            package_bases(o, "7.3.0"),
            [o.join("store/pkg/nvidia/sha256-a/lib/modules/7.3.0")]
        );
        assert!(package_bases(o, "7.2.0").is_empty());
    }

    #[test]
    fn package_firmware_links_by_entry() {
        let d = tempfile::tempdir().unwrap();
        let o = d.path();
        let pkg = |name: &str, hash: &str, entry: &str| {
            fs::create_dir_all(o.join("objects/pkg").join(name)).unwrap();
            let spec = format!(r#"{{"present": true, "hash": "{hash}"}}"#);
            fs::write(o.join("objects/pkg").join(name).join("applied.json"), spec).unwrap();
            let fw = o.join("store/pkg").join(name).join(hash).join("lib/firmware");
            fs::create_dir_all(fw.join(entry)).unwrap();
            fw
        };
        let nvidia = pkg("nvidia", "sha256-a", "nvidia");
        pkg("firmware", "sha256-b", "intel");
        pkg("linux", "sha256-c", "x");
        assert_eq!(package_firmware(o), [nvidia.clone()]);

        let into = o.join("run/firmware");
        fs::create_dir_all(&into).unwrap();
        std::os::unix::fs::symlink("/gone", into.join("old")).unwrap();
        assert!(link_package_firmware(&package_firmware(o), &into));
        assert_eq!(fs::read_link(into.join("nvidia")).unwrap(), nvidia.join("nvidia"));
        assert!(into.join("old").symlink_metadata().is_err());

        // The package moves: its link follows.
        let moved = pkg("nvidia", "sha256-d", "nvidia");
        assert!(link_package_firmware(&package_firmware(o), &into));
        assert_eq!(fs::read_link(into.join("nvidia")).unwrap(), moved.join("nvidia"));
        assert!(!link_package_firmware(&[], &into));
        assert!(into.join("nvidia").symlink_metadata().is_err());
    }

    #[test]
    fn host_options_come_before_the_cmdline() {
        let d = tempfile::tempdir().unwrap();
        let h = d.path().join("objects/host/local");
        fs::create_dir_all(&h).unwrap();
        fs::write(
            h.join("applied.json"),
            r#"{"module_options": {"nvidia": "NVreg_EnableGpuFirmware=0"}}"#,
        )
        .unwrap();
        let mut i = Index::parse(Path::new("/x"), "nvidia.ko:\n", "", "", "", "nvidia.NVreg_X=1");
        i.options = host_module_options(d.path());
        assert_eq!(i.params("nvidia"), "NVreg_EnableGpuFirmware=0 NVreg_X=1");
        assert_eq!(i.params("nvidia_drm"), "modeset=1 fbdev=1");
    }

    #[test]
    fn static_nodes_need_a_devname_and_a_fixed_major() {
        let i = Index::parse(
            Path::new("/x"),
            "",
            "\
alias devname:uhid uhid
alias char-major-10-239 uhid
alias char-major-116-33 snd_timer
alias devname:snd/timer snd-timer
alias devname:loop-control loop
alias block-major-7-* loop
alias devname:vhost-net vhost_net
alias devname:../etc/x evil
alias char-major-10-1 evil
alias devname:sr0 sr_mod
alias block-major-11-0 sr_mod
",
            "",
            "",
            "",
        );
        let nodes = i.static_nodes();
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["uhid", "snd/timer", "sr0"]);
        let devs: Vec<_> = nodes.iter().map(|n| (n.block, n.major, n.minor)).collect();
        assert_eq!(devs, [(false, 10, 239), (false, 116, 33), (true, 11, 0)]);
    }

    #[test]
    fn glob_like_fnmatch() {
        assert!(glob(b"usb:v05ACp*d*", b"usb:v05ACp828Dd0150"));
        assert!(!glob(b"usb:v05ACp*d*", b"usb:v05ADp828Dd0150"));
        assert!(glob(b"a[0-9]b", b"a7b"));
        assert!(!glob(b"a[!0-9]b", b"a7b"));
        assert!(glob(b"acpi*:QEMU0002:*", b"acpi:QEMU0002:"));
        assert!(glob(b"*", b""));
        assert!(!glob(b"a?", b"a"));
    }
}
