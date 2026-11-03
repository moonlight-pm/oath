//! The box, from the host's side: where a session lives, what is mounted, and how bwrap is
//! started. Everything the model drives runs in here. The root is an allowlist, and the
//! project is an overlay, so its writes land in the session and the real tree does not change
//! until the owner applies them.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub const BWRAP: &str = "/bin/bwrap";
/// Where writ's own binary is in a box.
pub const INNER: &str = "/writ";
/// A tmpfs is memory, so a box cannot fill the machine's with /tmp or its home.
const TMPFS_BYTES: u64 = 4 << 30;
/// More files than this and the search for secret names gives up, and the box does not start.
const MAX_SCANNED: usize = 4_000_000;

/// From `/`, read-only, what a program needs to run and what teaches it the machine. Not
/// `/oath/log`, `/oath/run`, `/oath/ssh`, `/oath/login`; not `/etc/shadow` or `/etc/ssh`.
const SYSTEM: &[&str] = &["/bin", "/sbin", "/lib", "/oath/store"];
const SYSTEM_IF_PRESENT: &[&str] = &[
    "/lib32",
    "/oath/INDEX.md",
    "/oath/objects",
    "/oath/schema",
    "/oath/status",
    "/oath/output.json",
    "/etc/passwd",
    "/etc/group",
    "/etc/nsswitch.conf",
    "/etc/shells",
    "/etc/hosts",
    "/var/empty",
];

/// Caches under home a build reads, each with writes that vanish with the box. Only these
/// parts: `~/.cargo/credentials.toml` stays out.
const CACHES: &[&str] = &[".cargo/registry", ".local/share/go/pkg/mod"];

/// The whole environment of a box. Nothing comes from the host's.
fn environment(home: &Path) -> Vec<(String, String)> {
    let home = home.display();
    [
        ("HOME", format!("{home}")),
        ("PATH", "/bin:/sbin".into()),
        ("TMPDIR", "/tmp".into()),
        ("TERM", "dumb".into()),
        ("LANG", "C.UTF-8".into()),
        ("NO_COLOR", "1".into()),
        ("PAGER", "cat".into()),
        ("GIT_PAGER", "cat".into()),
        ("GIT_TERMINAL_PROMPT", "0".into()),
        ("CARGO_NET_OFFLINE", "true".into()),
        ("CARGO_TERM_COLOR", "never".into()),
        ("GOPATH", format!("{home}/.local/share/go")),
        ("GOCACHE", "/tmp/go-build".into()),
        ("GOPROXY", "off".into()),
        ("GOSUMDB", "off".into()),
        ("GOFLAGS", "-mod=readonly".into()),
        ("GOTOOLCHAIN", "local".into()),
        ("GOTELEMETRY", "off".into()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

pub fn home() -> Result<PathBuf, String> {
    let h = std::env::var_os("HOME").ok_or("HOME is not set")?;
    let h = fs::canonicalize(&h).map_err(|e| format!("HOME: {e}"))?;
    if h == Path::new("/") {
        return Err("HOME is /".into());
    }
    Ok(h)
}

/// Why a directory cannot be a project, if it cannot. A project is mounted into the box, so
/// one that holds the home directory, or a tool's private directory, would bring it all along.
pub fn refuse_project(project: &Path, home: &Path) -> Option<String> {
    if home.starts_with(project) {
        return Some(format!("{} holds the home directory", project.display()));
    }
    let allowed = [home, Path::new("/tmp"), Path::new("/work")];
    if !allowed.iter().any(|a| project.starts_with(a) && project != *a) {
        return Some(format!("{} is not under {}, /tmp or /work", project.display(), home.display()));
    }
    // A hidden directory at the top of home is a tool's state or credentials. ~/.cache is
    // the exception: nothing there is secret, and `writ check` builds its project in it.
    if let Ok(rest) = project.strip_prefix(home) {
        if let Some(first) = rest.components().next() {
            let first = first.as_os_str();
            if first.as_bytes().starts_with(b".") && first != OsStr::new(".cache") {
                return Some(format!("{} is inside {}", project.display(), first.to_string_lossy()));
            }
        }
    }
    None
}

/// A name that usually holds a secret. A convenience, not a boundary: a secret under any
/// other name reaches the box. Examples and public keys are not secrets.
fn secret_file(name: &[u8]) -> bool {
    let name = String::from_utf8_lossy(name);
    if name == ".env" {
        return true;
    }
    if let Some(rest) = name.strip_prefix(".env.") {
        return !matches!(rest, "example" | "sample" | "template" | "dist");
    }
    matches!(
        name.as_ref(),
        "id_rsa" | "id_dsa" | "id_ecdsa" | "id_ed25519" | ".netrc" | ".pgpass" | ".npmrc" | ".pypirc"
            | ".git-credentials" | "credentials.json" | "secrets.json"
    ) || [".pem", ".key", ".p12", ".pfx"].iter().any(|s| name.ends_with(s))
}

fn secret_dir(name: &[u8]) -> bool {
    matches!(name, b".ssh" | b".gnupg" | b".aws")
}

/// What to hide in a project: files, then directories. Never follows a symlink.
fn find_secrets(project: &Path) -> Result<(Vec<PathBuf>, Vec<PathBuf>), String> {
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    let mut stack = vec![project.to_path_buf()];
    let mut scanned = 0usize;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            scanned += 1;
            if scanned > MAX_SCANNED {
                return Err(format!("{} has over {MAX_SCANNED} files", project.display()));
            }
            let Ok(kind) = entry.file_type() else { continue };
            let name = entry.file_name();
            if kind.is_dir() {
                if secret_dir(name.as_bytes()) {
                    dirs.push(entry.path());
                } else if !matches!(name.as_bytes(), b".git" | b"target" | b"node_modules") {
                    stack.push(entry.path());
                }
            } else if kind.is_file() && secret_file(name.as_bytes()) {
                files.push(entry.path());
            }
        }
    }
    Ok((files, dirs))
}

/// One session's place on the host, outside the box.
pub struct Session {
    pub dir: PathBuf,
    pub upper: PathBuf,
    work: PathBuf,
    empty: PathBuf,
}

impl Session {
    pub fn open(id: &str, home: &Path) -> Result<Session, String> {
        let safe = !id.is_empty()
            && !id.starts_with('.')
            && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
        if !safe {
            return Err(format!("{id:?} is not a session name"));
        }
        let dir = home.join(".local/share/writ/sessions").join(id);
        let (upper, work, empty) = (dir.join("upper"), dir.join("work"), dir.join("empty"));
        for d in [&upper, &work] {
            fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        for d in [&dir, &upper, &work] {
            fs::set_permissions(d, fs::Permissions::from_mode(0o700))
                .map_err(|e| format!("{}: {e}", d.display()))?;
        }
        fs::write(&empty, b"").map_err(|e| format!("{}: {e}", empty.display()))?;
        Ok(Session { dir, upper, work, empty })
    }

    /// Paths the session changed, whiteouts included. Reads names only.
    pub fn changed(&self) -> usize {
        count(&self.upper)
    }

    /// Remove the session and everything the box left, whatever modes it set. The kernel
    /// leaves overlayfs's work directory at mode 000.
    pub fn remove(self) -> Result<(), String> {
        open_up(&self.dir);
        fs::remove_dir_all(&self.dir).map_err(|e| format!("{}: {e}", self.dir.display()))
    }
}

fn count(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| {
            let sub = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            1 + if sub { count(&e.path()) } else { 0 }
        })
        .sum()
}

/// Give the owner back every directory under `dir`, without following a symlink.
fn open_up(dir: &Path) {
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            open_up(&e.path());
        }
    }
}

/// bwrap comes from the store, read-only and root's. Not from the project or a directory the
/// box could write.
pub fn trusted_bwrap() -> Result<(), String> {
    let real = fs::canonicalize(BWRAP).map_err(|e| format!("{BWRAP}: {e}"))?;
    let m = fs::metadata(&real).map_err(|e| format!("{}: {e}", real.display()))?;
    if m.uid() != 0 || m.mode() & 0o022 != 0 {
        return Err(format!("{} can be changed by someone other than root", real.display()));
    }
    Ok(())
}

pub struct Plan {
    pub home: PathBuf,
    pub project: PathBuf,
    pub exe: PathBuf,
    upper: PathBuf,
    work: PathBuf,
    empty: PathBuf,
    secret_files: Vec<PathBuf>,
    secret_dirs: Vec<PathBuf>,
    /// A writable tmpfs outside Landlock's rules, so `writ check` can see Landlock refuse.
    pub landlock_probe: bool,
}

impl Plan {
    pub fn new(project: &Path, session: &Session, home: &Path) -> Result<Plan, String> {
        let project = fs::canonicalize(project).map_err(|e| format!("{}: {e}", project.display()))?;
        if let Some(why) = refuse_project(&project, home) {
            return Err(why);
        }
        trusted_bwrap()?;
        let exe = std::env::current_exe().map_err(|e| format!("own path: {e}"))?;
        let (secret_files, secret_dirs) = find_secrets(&project)?;
        Ok(Plan {
            home: home.to_path_buf(),
            project,
            exe,
            upper: session.upper.clone(),
            work: session.work.clone(),
            empty: session.empty.clone(),
            secret_files,
            secret_dirs,
            landlock_probe: false,
        })
    }

    /// bwrap's arguments, up to the program it executes. Order matters: a later mount covers
    /// an earlier one.
    pub fn args(&self) -> Vec<OsString> {
        let mut a = Args::default();
        a.flag("--unshare-all");
        a.flag("--unshare-user");
        a.flag("--disable-userns");
        a.flag("--assert-userns-disabled");
        a.flag("--die-with-parent");
        a.flag("--new-session");
        a.two("--hostname", "writ");
        a.flag("--clearenv");
        for (k, v) in environment(&self.home) {
            a.three("--setenv", &k, &v);
        }
        for p in SYSTEM {
            a.three("--ro-bind", p, p);
        }
        for p in SYSTEM_IF_PRESENT {
            a.three("--ro-bind-try", p, p);
        }
        a.two("--proc", "/proc");
        a.two("--dev", "/dev");
        a.sized_tmpfs(Path::new("/tmp"));
        a.sized_tmpfs(&self.home);
        for c in CACHES {
            let src = self.home.join(c);
            if src.is_dir() {
                a.two("--overlay-src", &src);
                a.two("--tmp-overlay", &src);
            }
        }
        a.two("--overlay-src", &self.project);
        a.flag("--overlay");
        a.flag(&self.upper);
        a.flag(&self.work);
        a.flag(&self.project);
        for f in &self.secret_files {
            a.three("--ro-bind", &self.empty, f);
        }
        for d in &self.secret_dirs {
            a.two("--tmpfs", d);
            a.two("--remount-ro", d);
        }
        if self.landlock_probe {
            a.two("--tmpfs", "/landlock-probe");
        }
        a.three("--ro-bind", &self.exe, INNER);
        a.two("--remount-ro", "/");
        a.two("--chdir", &self.project);
        a.0
    }

    /// The command that runs `argv` in a box. Its environment is empty and its stdin is
    /// /dev/null; stdout and stderr are pipes, so no terminal reaches the box.
    pub fn command(&self, argv: &[OsString]) -> Command {
        let mut c = Command::new(BWRAP);
        c.env_clear();
        c.args(self.args());
        c.arg(INNER).arg("inner");
        let mut writable: Vec<&Path> =
            vec![Path::new("/tmp"), self.home.as_path(), Path::new("/dev/shm")];
        if !self.project.starts_with(&self.home) && !self.project.starts_with("/tmp") {
            writable.push(&self.project);
        }
        for w in writable {
            c.arg("--write").arg(w);
        }
        c.arg("--").args(argv);
        c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        c
    }
}

#[derive(Default)]
struct Args(Vec<OsString>);

impl Args {
    fn flag<S: AsRef<OsStr>>(&mut self, s: S) {
        self.0.push(s.as_ref().to_os_string());
    }
    fn two<A: AsRef<OsStr>, B: AsRef<OsStr>>(&mut self, a: A, b: B) {
        self.flag(a);
        self.flag(b);
    }
    fn three<A: AsRef<OsStr>, B: AsRef<OsStr>, C: AsRef<OsStr>>(&mut self, a: A, b: B, c: C) {
        self.flag(a);
        self.flag(b);
        self.flag(c);
    }
    fn sized_tmpfs(&mut self, dest: &Path) {
        self.two("--size", TMPFS_BYTES.to_string());
        self.two("--tmpfs", dest);
    }
}

/// Wait for a box, killing it at the deadline. Killing bwrap ends everything in the box: it
/// is the box's PID namespace, and `--die-with-parent` takes the rest.
pub fn wait(child: &mut Child, limit: Option<Duration>) -> Result<(ExitStatus, bool), String> {
    let start = Instant::now();
    loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            return Ok((s, false));
        }
        if limit.is_some_and(|l| start.elapsed() > l) {
            let _ = child.kill();
            let s = child.wait().map_err(|e| e.to_string())?;
            return Ok((s, true));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Read a pipe to its end on a thread, handing each read to `f`, and an empty slice at the end.
pub fn drain<R, F>(mut pipe: R, mut f: F) -> std::thread::JoinHandle<()>
where
    R: Read + Send + 'static,
    F: FnMut(&[u8]) + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buf = [0u8; 16384];
        while let Ok(n) = pipe.read(&mut buf) {
            if n == 0 {
                break;
            }
            f(&buf[..n]);
        }
        f(&[]);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn a_project_cannot_bring_home_along() {
        let home = p("/home");
        for bad in ["/", "/home", "/etc", "/oath/store/x", "/home/.ssh", "/home/.ssh/sub", "/home/.config/oath", "/home/.local/share/writ/sessions/x", "/tmp", "/work", "/root/x", "/homely/x"] {
            assert!(refuse_project(&p(bad), &home).is_some(), "{bad}");
        }
        for good in ["/home/Projects/Oath", "/home/x", "/home/.cache/writ/check/project", "/tmp/x", "/work/app"] {
            assert_eq!(refuse_project(&p(good), &home), None, "{good}");
        }
    }

    #[test]
    fn secret_names() {
        for s in [".env", ".env.local", ".env.production", "server.pem", "tls.key", "id_ed25519", "id_rsa", ".netrc", ".npmrc", "cert.p12"] {
            assert!(secret_file(s.as_bytes()), "{s}");
        }
        for s in [".env.example", ".env.sample", "id_ed25519.pub", "main.rs", "environment", "monkey", "key.asc", "README.md"] {
            assert!(!secret_file(s.as_bytes()), "{s}");
        }
    }

    fn plan() -> Plan {
        Plan {
            home: p("/home"),
            project: p("/home/Projects/x"),
            exe: p("/oath/store/pkg/writ/h/bin/writ"),
            upper: p("/home/.local/share/writ/sessions/s/upper"),
            work: p("/home/.local/share/writ/sessions/s/work"),
            empty: p("/home/.local/share/writ/sessions/s/empty"),
            secret_files: vec![p("/home/Projects/x/.env")],
            secret_dirs: vec![p("/home/Projects/x/.ssh")],
            landlock_probe: false,
        }
    }

    fn text(a: &[OsString]) -> Vec<String> {
        a.iter().map(|s| s.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn the_root_is_an_allowlist_and_nothing_shares_the_host() {
        let a = text(&plan().args());
        let s = a.join(" ");
        for bad in ["--share-net", "--bind / ", "--ro-bind / /", "--dev-bind", "--cap-add", "/home/.ssh", "/oath/run", "/oath/log", "/etc/shadow", "/oath/ssh", "/run"] {
            assert!(!s.contains(bad), "{bad} in {s}");
        }
        for need in ["--unshare-all", "--unshare-user", "--disable-userns", "--die-with-parent", "--new-session", "--clearenv", "--remount-ro /"] {
            assert!(s.contains(need), "{need}");
        }
    }

    #[test]
    fn home_is_empty_before_the_project_is_mounted_and_secrets_cover_it() {
        let a = text(&plan().args());
        let at = |t: &str| a.iter().position(|x| x == t).unwrap_or_else(|| panic!("{t}"));
        assert!(at("--overlay") > a.iter().rposition(|x| x == "/home").unwrap());
        assert!(a.iter().position(|x| x == "/home/Projects/x/.env").unwrap() > at("--overlay"));
        assert!(a.iter().position(|x| x == "/home/Projects/x/.ssh").unwrap() > at("--overlay"));
    }

    #[test]
    fn the_environment_holds_no_host_value() {
        std::env::set_var("WRIT_TEST_KEY", "sk-or-v1-canary");
        let a = text(&plan().args()).join(" ");
        assert!(!a.contains("canary"));
        for (k, _) in environment(&p("/home")) {
            assert!(!k.contains("KEY") && !k.contains("TOKEN") && !k.contains("SECRET"), "{k}");
        }
    }

    #[test]
    fn a_session_name_cannot_walk() {
        let home = std::env::temp_dir();
        for bad in ["", ".", "..", "../x", "a/b", ".hidden", "a b"] {
            assert!(Session::open(bad, &home).is_err(), "{bad:?}");
        }
    }
}
