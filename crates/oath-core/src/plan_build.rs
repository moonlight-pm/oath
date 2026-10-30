//! Isolated `oath build` (T43). Network off. Product to the pkg store only,
//! whole or not at all; the script's output to `log/plan/<name>.log`.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::json;

use crate::error::{Error, Result};
use crate::id::ObjectId;
use crate::kinds::{Meta, PkgNeed, PlanActual};
use crate::packhash::{hash_tree, is_realization_id, realization_dir, slot_dir, LIVE_NAME};
use crate::pkg::{fetch_pkg_any, put_tree, resolve_tree, store_present};
use crate::plan::{
    file_hash, fmt_plan, input_set_hash, lint_bytes, lint_path, PlanFile, PLAN_NAME,
};
use crate::write_json;
use crate::{Catalog, KIND_PKG, KIND_PLAN};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildReport {
    pub name: String,
    pub plan_hash: String,
    pub product: String,
    pub inputs: String,
    pub skipped: bool,
    /// The build's output (`build_log`); a skipped build wrote none.
    pub log: PathBuf,
}

/// Where a build of plan `name` writes its script's output:
/// `log/plan/<name>.log` in the catalog (`/oath/log/plan/<name>.log`). The
/// build before it is `<name>.log.1`. `oath logs plan:<name>` reads both.
pub fn build_log(catalog_root: &Path, name: &str) -> PathBuf {
    catalog_root.join("log/plan").join(format!("{name}.log"))
}

/// The last `n` lines of a log, for an error message.
fn tail(path: &Path, n: usize) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Copy a linted plan file into the store and pin `plan:<name>`.
pub fn file_plan(catalog_root: &Path, bytes: &[u8]) -> Result<String> {
    let _lock = Catalog::open(catalog_root)?.lock()?;
    let plan = lint_bytes(bytes)?;
    let hash = file_hash(bytes);
    let dir = catalog_root.join("store/plan").join(&plan.name).join(&hash);
    fs::create_dir_all(&dir)?;
    let dest = dir.join(PLAN_NAME);
    fs::write(&dest, bytes)?;
    let extra: Vec<_> =
        fs::read_dir(&dir)?.flatten().filter(|e| e.file_name() != PLAN_NAME).collect();
    if !extra.is_empty() {
        return Err(Error::hint("plan store dir must contain only plan.plan", "oath schema plan"));
    }
    let dir_obj = catalog_root.join("objects/plan").join(&plan.name);
    fs::create_dir_all(&dir_obj)?;
    // Filing a plan pins it: the new pin is applied at once.
    write_json(&dir_obj.join("desired.json"), &json!({ "hash": hash }))?;
    write_json(&dir_obj.join("applied.json"), &json!({ "hash": hash }))?;
    // A new pin forgets the last build: its product says nothing about
    // this plan file.
    let status = crate::status_path(catalog_root, &ObjectId::new(KIND_PLAN, &plan.name));
    let actual = if status.is_file() {
        let mut a: PlanActual = crate::read_json(&status)?;
        if a.hash != hash {
            a = PlanActual { hash: hash.clone(), product: String::new(), inputs: String::new() };
        }
        a
    } else {
        PlanActual { hash: hash.clone(), product: String::new(), inputs: String::new() }
    };
    write_json(&status, &actual)?;
    write_json(&dir_obj.join("meta.json"), &Meta::new(KIND_PLAN, &plan.name, "mutate"))?;
    Ok(hash)
}

/// Record a build of a plan file as `plan:<name>`'s last build, when the
/// plan's pin is that file: an update builds the output's plan before it
/// files it, and `oath build plan:<name>` then finds the product built.
pub fn record_build(catalog_root: &Path, r: &BuildReport) -> Result<()> {
    let id = ObjectId::new(KIND_PLAN, &r.name);
    let pin = Catalog::open(catalog_root)?.get(&id)?.desired;
    if pin.get("hash").and_then(|h| h.as_str()) != Some(r.plan_hash.as_str()) {
        return Ok(());
    }
    write_json(
        &crate::status_path(catalog_root, &id),
        &PlanActual {
            hash: r.plan_hash.clone(),
            product: r.product.clone(),
            inputs: r.inputs.clone(),
        },
    )
}

pub fn build_file(catalog_root: &Path, path: &Path) -> Result<BuildReport> {
    let _lock = Catalog::open(catalog_root)?.lock()?;
    let (plan, bytes) = lint_path(path)?;
    build_inner(catalog_root, &plan, &file_hash(&bytes), None, false)
}

/// [`build_file`] for a caller that runs several builds of different
/// plans at once (`cargo make pkgs -j`): the script runs without the
/// catalog lock, which is held only while a pack is fetched or the
/// product stored, one build of this process at a time.
pub fn build_file_concurrent(catalog_root: &Path, path: &Path) -> Result<BuildReport> {
    let (plan, bytes) = lint_path(path)?;
    build_inner(catalog_root, &plan, &file_hash(&bytes), None, true)
}

/// Serializes the store writes of this process's concurrent builds, so
/// each takes the catalog lock in turn and none waits out its timeout.
/// A sandbox forks only with it held (`sandbox_child`): its child lives
/// for the whole build without exec'ing, and would keep an inherited
/// lock file open, and so the lock held, until the build ends.
static STORE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The catalog lock and this process's turn at it. Fields drop in order:
/// the lock goes before the turn, so no fork sees it held.
struct StoreLock {
    _lock: crate::CatalogLock,
    _turn: std::sync::MutexGuard<'static, ()>,
}

fn store_turn() -> std::sync::MutexGuard<'static, ()> {
    STORE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The catalog lock for a concurrent build's store write; none for a
/// build whose caller holds it already.
fn store_lock(catalog_root: &Path, concurrent: bool) -> Result<Option<StoreLock>> {
    if !concurrent {
        return Ok(None);
    }
    let turn = store_turn();
    Ok(Some(StoreLock { _lock: Catalog::open(catalog_root)?.lock()?, _turn: turn }))
}

pub fn build_pinned(catalog_root: &Path, name: &str) -> Result<BuildReport> {
    let cat = Catalog::open(catalog_root)?;
    let _lock = cat.lock()?;
    let id = ObjectId::new(KIND_PLAN, name);
    let obj = cat.get(&id)?;
    let hash = obj.desired.get("hash").and_then(|h| h.as_str()).unwrap_or("").to_string();
    if !is_realization_id(&hash) {
        return Err(Error::hint(
            format!("plan:{name} is unpinned"),
            format!("oath set plan:{name} --from-file <path>"),
        ));
    }
    let path = catalog_root.join("store/plan").join(name).join(&hash).join(PLAN_NAME);
    let (plan, bytes) = lint_path(&path)?;
    if plan.name != name {
        return Err(Error::hint(
            format!("plan file name `{}` != slot `{name}`", plan.name),
            "oath schema plan",
        ));
    }
    if file_hash(&bytes) != hash {
        return Err(Error::hint(
            "plan.plan bytes do not match the pin",
            "oath set plan:<name> --from-file <path>",
        ));
    }
    build_inner(catalog_root, &plan, &hash, Some(&id), false)
}

fn build_inner(
    catalog_root: &Path,
    plan: &PlanFile,
    plan_hash: &str,
    plan_id: Option<&ObjectId>,
    concurrent: bool,
) -> Result<BuildReport> {
    let inputs = input_set_hash(&plan.build_needs);
    if let Some(id) = plan_id {
        if let Ok(obj) = Catalog::open(catalog_root)?.get(id) {
            let prev_h = obj.actual.get("hash").and_then(|v| v.as_str()).unwrap_or("");
            let prev_p = obj.actual.get("product").and_then(|v| v.as_str()).unwrap_or("");
            let prev_i = obj.actual.get("inputs").and_then(|v| v.as_str()).unwrap_or("");
            // The cache key is the plan file and its inputs, both.
            if prev_h == plan_hash && prev_i == inputs && is_realization_id(prev_p) {
                let dest = realization_dir(catalog_root, &plan.name, prev_p);
                if dest.is_dir() {
                    return Ok(BuildReport {
                        name: plan.name.clone(),
                        plan_hash: plan_hash.to_string(),
                        product: prev_p.to_string(),
                        inputs,
                        skipped: true,
                        log: build_log(catalog_root, &plan.name),
                    });
                }
            }
        }
    }

    for n in &plan.build_needs {
        ensure_need(catalog_root, n, concurrent)?;
    }

    let scratch = catalog_root.join("run/plan-build").join(&plan.name);
    let _ = fs::remove_dir_all(&scratch);
    let out = scratch.join("out");
    fs::create_dir_all(out.join("bin"))?;
    let script = fmt_plan(plan);
    let script_path = scratch.join("build.sh");
    let body = format!("#!/bin/sh\nset -eu\n{}", plan.script);
    fs::write(
        &script_path,
        if plan.script.is_empty() { "#!/bin/sh\nset -eu\n".into() } else { body },
    )?;
    let mut perm = fs::metadata(&script_path)?.permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&script_path, perm)?;

    let mut packs: Vec<(PkgNeed, PathBuf)> = Vec::new();
    for n in &plan.build_needs {
        let name = n.id.strip_prefix("pkg:").unwrap();
        packs.push((n.clone(), resolve_tree(catalog_root, name, &n.hash)?));
    }

    let log_path = build_log(catalog_root, &plan.name);
    fs::create_dir_all(log_path.parent().unwrap())?;
    if log_path.is_file() {
        let mut old = log_path.clone().into_os_string();
        old.push(".1");
        fs::rename(&log_path, old)?;
    }
    let mut log = fs::File::create(&log_path)?;
    writeln!(log, "# oath build plan:{} {plan_hash} at {}", plan.name, crate::now_rfc3339())?;
    if let Err(e) = run_sandbox(&packs, &script_path, &out, &log) {
        let _ = writeln!(log, "# failed: {e}");
        return Err(Error::hint(
            format!(
                "plan:{} failed: {e}\n--- last lines of {} ---\n{}",
                plan.name,
                log_path.display(),
                tail(&log_path, 25)
            ),
            format!("oath logs plan:{} -n 200", plan.name),
        ));
    }

    let product = hash_tree(&out)?;
    if let Some(id) = plan_id {
        if let Ok(obj) = Catalog::open(catalog_root)?.get(id) {
            let prev_h = obj.actual.get("hash").and_then(|v| v.as_str()).unwrap_or("");
            let prev_p = obj.actual.get("product").and_then(|v| v.as_str()).unwrap_or("");
            let prev_i = obj.actual.get("inputs").and_then(|v| v.as_str()).unwrap_or("");
            if prev_h == plan_hash
                && prev_i == inputs
                && is_realization_id(prev_p)
                && prev_p != product
            {
                return Err(Error::security(
                    format!(
                        "plan:{} rebuilt to {product}, previous product was {prev_p}",
                        plan.name
                    ),
                    "same plan + inputs must yield the same product; the plan is not closed",
                ));
            }
        }
    }

    let _store = store_lock(catalog_root, concurrent)?;
    let dest = realization_dir(catalog_root, &plan.name, &product);
    if dest.is_dir() {
        let existing = hash_tree(&dest)?;
        if existing != product {
            return Err(Error::security(
                format!("store already has {} with a different tree", dest.display()),
                "oath schema pkg",
            ));
        }
    } else {
        put_tree(&slot_dir(catalog_root, &plan.name), &product, &out)?;
    }
    writeln!(log, "# product {product}")?;

    ensure_pkg_object(catalog_root, plan, &product)?;
    if let Some(id) = plan_id {
        write_json(
            &crate::status_path(catalog_root, id),
            &PlanActual {
                hash: plan_hash.to_string(),
                product: product.clone(),
                inputs: inputs.clone(),
            },
        )?;
        let _ = Catalog::open(catalog_root)?.get(id);
    }
    let _ = fs::remove_dir_all(&scratch);
    let _ = script;
    Ok(BuildReport {
        name: plan.name.clone(),
        plan_hash: plan_hash.to_string(),
        product,
        inputs,
        skipped: false,
        log: log_path,
    })
}

fn ensure_pkg_object(catalog_root: &Path, plan: &PlanFile, product: &str) -> Result<()> {
    let dir = catalog_root.join("objects/pkg").join(&plan.name);
    if dir.join("desired.json").is_file() {
        return Ok(());
    }
    fs::create_dir_all(&dir)?;
    let mut desired = json!({ "present": false, "hash": product });
    if !plan.run_needs.is_empty() {
        desired["needs"] = serde_json::to_value(&plan.run_needs)?;
    }
    write_json(&dir.join("desired.json"), &desired)?;
    write_json(&dir.join("applied.json"), &desired)?;
    write_json(&dir.join("meta.json"), &Meta::new(KIND_PKG, &plan.name, "mutate"))?;
    Ok(())
}

fn ensure_need(catalog_root: &Path, n: &PkgNeed, concurrent: bool) -> Result<()> {
    let name = n.id.strip_prefix("pkg:").ok_or_else(|| {
        Error::hint(format!("build_needs id must be pkg:*, got {}", n.id), "oath schema plan")
    })?;
    if store_present(catalog_root, name, &n.hash) {
        let tree = resolve_tree(catalog_root, name, &n.hash)?;
        let got = hash_tree(&tree)?;
        if got != n.hash {
            return Err(Error::hint(
                format!("{} tree hash is {got}, plan wants {}", n.id, n.hash),
                "oath schema plan",
            ));
        }
        return Ok(());
    }
    let _store = store_lock(catalog_root, concurrent)?;
    if store_present(catalog_root, name, &n.hash) {
        // Another build fetched it while this one waited its turn.
        return Ok(());
    }
    let cat = Catalog::open(catalog_root)?;
    let obj = cat.get(&ObjectId::new(KIND_PKG, name)).ok();
    let field = |k: &str| {
        obj.as_ref().and_then(|o| o.desired.get(k).and_then(|u| u.as_str())).unwrap_or("")
    };
    let (url, file) = (field("url"), field("file"));
    if fetch_pkg_any(catalog_root, name, url, file, &n.hash)?.is_none() {
        return Err(Error::hint(
            format!("{} @ {} is not in the store", n.id, n.hash),
            "fetch it (pkg.url or host:local.origins) or pack it first",
        ));
    }
    if !store_present(catalog_root, name, &n.hash) {
        return Err(Error::hint(
            format!("fetch {} did not produce {}", n.id, n.hash),
            "oath schema pkg",
        ));
    }
    Ok(())
}

fn run_sandbox(
    packs: &[(PkgNeed, PathBuf)],
    script: &Path,
    out: &Path,
    log: &fs::File,
) -> Result<()> {
    let scratch = out.parent().unwrap().join("root");
    fs::create_dir_all(&scratch)?;
    sandbox_child(packs, script, out, &scratch, log)?;
    Ok(())
}

fn sandbox_child(
    packs: &[(PkgNeed, PathBuf)],
    script: &Path,
    out: &Path,
    scratch: &Path,
    log: &fs::File,
) -> Result<bool> {
    let packs: Vec<(String, String, PathBuf)> = packs
        .iter()
        .map(|(n, p)| {
            (n.id.strip_prefix("pkg:").unwrap_or(&n.id).to_string(), n.hash.clone(), p.clone())
        })
        .collect();
    let script = script.to_path_buf();
    let out = out.to_path_buf();
    let scratch = scratch.to_path_buf();

    let mut pipe_a = [0i32; 2];
    let mut pipe_b = [0i32; 2];
    if unsafe { libc::pipe(pipe_a.as_mut_ptr()) } != 0
        || unsafe { libc::pipe(pipe_b.as_mut_ptr()) } != 0
    {
        return Err(Error::Msg("pipe failed".into()));
    }
    let host_root = unsafe { libc::geteuid() } == 0;
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    // Not while another of this process's builds holds the catalog lock.
    let turn = store_turn();
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(Error::Msg("fork failed".into()));
    }
    if pid > 0 {
        drop(turn);
        unsafe {
            libc::close(pipe_a[1]);
            libc::close(pipe_b[0]);
        }
        if !host_root {
            let mut b = [0u8; 1];
            let _ = unsafe { libc::read(pipe_a[0], b.as_mut_ptr().cast(), 1) };
            let proc = format!("/proc/{pid}");
            let _ = fs::write(format!("{proc}/setgroups"), b"deny");
            let _ = fs::write(format!("{proc}/uid_map"), format!("0 {uid} 1\n"));
            let _ = fs::write(format!("{proc}/gid_map"), format!("0 {gid} 1\n"));
            let _ = unsafe { libc::write(pipe_b[1], [1u8].as_ptr().cast(), 1) };
        }
        unsafe {
            libc::close(pipe_a[0]);
            libc::close(pipe_b[1]);
        }
        let mut st: i32 = 0;
        let w = unsafe { libc::waitpid(pid, &mut st, 0) };
        if w < 0 {
            return Err(Error::Msg("waitpid failed".into()));
        }
        if libc::WIFEXITED(st) && libc::WEXITSTATUS(st) == 0 {
            return Ok(true);
        }
        let extra = fs::read_to_string(scratch.join("err"))
            .or_else(|_| fs::read_to_string(out.join(".err")))
            .unwrap_or_default();
        return Err(Error::hint(
            format!("sandbox exit status={st} signaled={} extra={extra}", libc::WIFSIGNALED(st)),
            "oath schema plan",
        ));
    }

    unsafe {
        libc::close(pipe_a[0]);
        libc::close(pipe_b[1]);
    }
    if !host_root {
        if unsafe { libc::unshare(libc::CLONE_NEWUSER) } != 0 {
            let _ = fs::write(scratch.join("err"), "unshare NEWUSER");
            unsafe { libc::_exit(1) };
        }
        let _ = unsafe { libc::write(pipe_a[1], [1u8].as_ptr().cast(), 1) };
        let mut b = [0u8; 1];
        let _ = unsafe { libc::read(pipe_b[0], b.as_mut_ptr().cast(), 1) };
    }
    unsafe {
        libc::close(pipe_a[1]);
        libc::close(pipe_b[0]);
    }
    // The child ends with _exit; its copy of the turn is never unlocked.
    std::mem::forget(turn);

    let err = scratch.join("err");
    let code = match sandbox_in_child(&packs, &script, &out, &scratch, host_root, log) {
        Ok(()) => 0,
        Err(e) => {
            let _ = fs::write(&err, format!("{e}"));
            1
        }
    };
    unsafe { libc::_exit(code) };
}

fn sandbox_in_child(
    packs: &[(String, String, PathBuf)],
    script: &Path,
    out: &Path,
    scratch: &Path,
    host_root: bool,
    log: &fs::File,
) -> Result<()> {
    // The script is PID 1 of its own PID namespace: it mounts a /proc that
    // shows only the build, and everything left over dies with it.
    let ns = libc::CLONE_NEWNS
        | libc::CLONE_NEWNET
        | libc::CLONE_NEWIPC
        | libc::CLONE_NEWUTS
        | libc::CLONE_NEWPID;
    if unsafe { libc::unshare(ns) } != 0 {
        return Err(Error::hint(
            "unshare failed (need mount/net/pid namespaces)",
            "oath schema plan",
        ));
    }
    // One host name on every builder, so a build that records it (vim's
    // `:version`) is the same on each.
    let host = b"build";
    if unsafe { libc::sethostname(host.as_ptr().cast(), host.len()) } != 0 {
        return Err(Error::Msg("sethostname failed".into()));
    }
    if unsafe {
        libc::mount(
            std::ptr::null(),
            b"/\0".as_ptr().cast(),
            std::ptr::null(),
            libc::MS_REC | libc::MS_PRIVATE,
            std::ptr::null(),
        )
    } != 0
    {
        return Err(Error::Msg("mount MS_PRIVATE failed".into()));
    }

    fs::create_dir_all(scratch.join("bin"))?;
    fs::create_dir_all(scratch.join("lib"))?;
    fs::create_dir_all(scratch.join("lib32"))?;
    fs::create_dir_all(scratch.join("out"))?;
    fs::create_dir_all(scratch.join("tmp"))?;
    fs::create_dir_all(scratch.join("dev"))?;
    fs::create_dir_all(scratch.join("oath/store/pkg"))?;
    // Nothing from the build host is visible: only the declared packs.
    // The unit tests copy the host's dynamically linked /bin/sh into packs,
    // and only they set OATH_SANDBOX_NIX_STORE to see /nix/store.
    if std::env::var_os("OATH_SANDBOX_NIX_STORE").is_some() && Path::new("/nix/store").is_dir() {
        fs::create_dir_all(scratch.join("nix/store"))?;
        bind(Path::new("/nix/store"), &scratch.join("nix/store"), true)?;
    }

    bind(out, &scratch.join("out"), false)?;
    chmod(out, 0o0777)?;
    let out_bin = out.join("bin");
    if out_bin.is_dir() {
        chmod(&out_bin, 0o0777)?;
    }
    chmod(&scratch.join("tmp"), 0o1777)?;
    fs::create_dir_all(scratch.join("proc"))?;
    for (name, hash, src) in packs {
        if !src.is_dir() {
            return Err(Error::hint(
                format!("missing pack tree {}", src.display()),
                "oath schema plan",
            ));
        }
        let dest = scratch.join("oath/store/pkg").join(name).join(hash);
        fs::create_dir_all(dest.parent().unwrap())?;
        fs::create_dir_all(&dest)?;
        bind(src, &dest, true)?;
        slot_compat(scratch, name, hash, src)?;
        // `/bin` and `/lib` are link farms of the packs' programs and
        // libraries, as on a machine (R17): glibc's loader is
        // /lib/ld-linux-x86-64.so.2 and it searches /lib. `/lib32` is the
        // packs' i386 libraries (R29), which a machine's root never shows.
        // The first pack to name an entry has it; pkg-config files stay
        // per pack.
        for dir in ["bin", "lib", "lib32"] {
            let from = dest.join(dir);
            if !from.is_dir() {
                continue;
            }
            for e in fs::read_dir(&from)? {
                let e = e?;
                if dir != "bin" && e.file_name() == "pkgconfig" {
                    continue;
                }
                let link = scratch.join(dir).join(e.file_name());
                if link.symlink_metadata().is_ok() {
                    continue;
                }
                let target = PathBuf::from("/oath/store/pkg")
                    .join(name)
                    .join(hash)
                    .join(dir)
                    .join(e.file_name());
                symlink(&target, &link)?;
            }
        }
    }
    // The i386 loader at the path 32-bit programs name, when a pack's
    // lib32/ has one (pkg:glibc-i386), so a 32-bit compiler's tests run.
    let ld32 = scratch.join("lib32/ld-linux.so.2");
    let at = scratch.join("lib/ld-linux.so.2");
    if ld32.symlink_metadata().is_ok() && at.symlink_metadata().is_err() {
        symlink(fs::read_link(&ld32)?, &at)?;
    }
    let build_sh = scratch.join("build.sh");
    fs::copy(script, &build_sh)?;
    let mut perm = fs::metadata(&build_sh)?.permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&build_sh, perm)?;
    for node in ["null", "zero", "urandom"] {
        let host = PathBuf::from("/dev").join(node);
        let guest = scratch.join("dev").join(node);
        let _ = fs::File::create(&guest);
        let _ = bind(&host, &guest, true);
    }

    let root_c = cstr(scratch)?;
    if unsafe { libc::chroot(root_c.as_ptr()) } != 0 {
        return Err(Error::Msg("chroot failed".into()));
    }
    std::env::set_current_dir("/tmp")?;

    let mut cmd = Command::new("/bin/sh");
    // SAFETY: mount(2), setgroups(2), setgid(2) and setuid(2) are
    // async-signal-safe; nothing else runs between fork and exec. Builds
    // that need no /proc still run without it.
    unsafe {
        cmd.pre_exec(move || {
            libc::mount(
                c"proc".as_ptr(),
                c"/proc".as_ptr(),
                c"proc".as_ptr(),
                libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC,
                std::ptr::null(),
            );
            // Drop to seat `home` only when we entered as host root (not a
            // user ns), and only after /proc: mounting it needs root. A
            // machine's builds had no /proc before (Bun reads its bundle
            // from /proc/self/exe).
            if host_root
                && (libc::setgroups(0, std::ptr::null()) != 0
                    || libc::setgid(crate::seat::GID) != 0
                    || libc::setuid(crate::seat::UID) != 0)
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let st = cmd
        .arg("/build.sh")
        .current_dir("/tmp")
        .env_clear()
        .env("PATH", "/bin")
        .env("HOME", "/tmp")
        .env("TMPDIR", "/tmp")
        .env("OUT", "/out")
        .env("XDG_CACHE_HOME", "/tmp")
        // The builder owns what it unpacks: GNU tar as root (in a user
        // namespace) would restore owners the namespace does not have.
        .env("TAR_OPTIONS", "--no-same-owner")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log.try_clone()?))
        .status()?;
    if !st.success() {
        let _ = fs::write("/out/.err", format!("build.sh {st:?}"));
        return Err(Error::hint("plan script failed", "oath schema plan"));
    }
    Ok(())
}

/// T32 slot face so wrappers that hardcode `/oath/store/pkg/<name>/libexec`
/// still work when the tree is bound at `<name>/<hash>`.
fn slot_compat(scratch: &Path, name: &str, hash: &str, src: &Path) -> Result<()> {
    let slot = scratch.join("oath/store/pkg").join(name);
    let live = slot.join(LIVE_NAME);
    if live.symlink_metadata().is_err() {
        symlink(Path::new(hash), &live)?;
    }
    for e in fs::read_dir(src)? {
        let e = e?;
        let fname = e.file_name();
        let n = fname.to_string_lossy();
        if n == LIVE_NAME || is_realization_id(&n) {
            continue;
        }
        let link = slot.join(&fname);
        if link.symlink_metadata().is_ok() {
            continue;
        }
        symlink(format!("{LIVE_NAME}/{n}"), &link)?;
    }
    Ok(())
}

fn chmod(path: &Path, mode: u32) -> Result<()> {
    let mut p = fs::metadata(path)?.permissions();
    p.set_mode(mode);
    fs::set_permissions(path, p)?;
    Ok(())
}

fn bind(src: &Path, dest: &Path, ro: bool) -> Result<()> {
    let s = cstr(src)?;
    let d = cstr(dest)?;
    if unsafe {
        libc::mount(
            s.as_ptr(),
            d.as_ptr(),
            std::ptr::null(),
            libc::MS_BIND | libc::MS_REC,
            std::ptr::null(),
        )
    } != 0
    {
        return Err(Error::Msg(format!("bind {} -> {} failed", src.display(), dest.display())));
    }
    if ro {
        let _ = unsafe {
            libc::mount(
                std::ptr::null(),
                d.as_ptr(),
                std::ptr::null(),
                libc::MS_BIND | libc::MS_REMOUNT | libc::MS_RDONLY | libc::MS_REC,
                std::ptr::null(),
            )
        };
    }
    Ok(())
}

fn cstr(p: &Path) -> Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(p.as_os_str().as_bytes())
        .map_err(|_| Error::Msg(format!("path contains NUL: {}", p.display())))
}
