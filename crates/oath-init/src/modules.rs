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
/// Islands cards only when asked.
const DEFAULT_PARAMS: &[(&str, &str)] = &[("amdgpu", "si_support=1 cik_support=1")];

/// Coldplug passes: loading a bus driver (virtio_pci, xhci) makes new
/// devices appear, which ask for their own drivers.
const PASSES: usize = 8;

pub fn base() -> PathBuf {
    let kver = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    Path::new("/lib/modules").join(kver.trim())
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

impl Index {
    pub fn open(base: &Path) -> Option<Self> {
        let read = |f: &str| fs::read_to_string(base.join(f)).unwrap_or_default();
        let dep = fs::read_to_string(base.join("modules.dep")).ok()?;
        let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();
        Some(Self::parse(
            base,
            &dep,
            &read("modules.alias"),
            &read("modules.softdep"),
            &read("modules.builtin"),
            &cmdline,
        ))
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

    /// Display drivers that replace the firmware framebuffer, and sound,
    /// wait until the network and sshd are up: on canto an early amdgpu
    /// hangs the NIC.
    pub fn deferred(&self, name: &str) -> bool {
        self.paths
            .get(name)
            .is_some_and(|p| crate::gpu::takes_over_firmware_fb(p) || p.contains("/sound/"))
    }

    /// Defaults, then `name.key=value` from the kernel command line (kmod
    /// does the same; the kernel only applies those to built-ins).
    fn params(&self, name: &str) -> String {
        let mut out: Vec<String> = DEFAULT_PARAMS
            .iter()
            .filter(|(m, _)| *m == name)
            .map(|(_, p)| (*p).to_string())
            .collect();
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
