//! Isolated `oath build` (T43). Network off. Product to the pkg store only.

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::json;

use crate::error::{Error, Result};
use crate::id::ObjectId;
use crate::kinds::{Meta, PkgNeed, PlanActual};
use crate::packhash::{fetch_url, hash_tree, is_realization_id, realization_dir, LIVE_NAME};
use crate::plan::{file_hash, fmt_plan, input_set_hash, lint_bytes, lint_path, PlanFile, PLAN_NAME};
use crate::pkg::{copy_tree, store_present};
use crate::write_json;
use crate::{Catalog, KIND_PKG, KIND_PLAN};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildReport {
    pub name: String,
    pub plan_hash: String,
    pub product: String,
    pub inputs: String,
    pub skipped: bool,
}

/// Copy a linted plan file into the store and pin `plan:<name>`.
pub fn file_plan(catalog_root: &Path, bytes: &[u8]) -> Result<String> {
    let plan = lint_bytes(bytes)?;
    let hash = file_hash(bytes);
    let dir = catalog_root.join("store/plan").join(&plan.name).join(&hash);
    fs::create_dir_all(&dir)?;
    let dest = dir.join(PLAN_NAME);
    fs::write(&dest, bytes)?;
    let extra: Vec<_> = fs::read_dir(&dir)?
        .flatten()
        .filter(|e| e.file_name() != PLAN_NAME)
        .collect();
    if !extra.is_empty() {
        return Err(Error::hint(
            "plan store dir must contain only plan.plan",
            "oath schema plan",
        ));
    }
    let dir_obj = catalog_root.join("objects/plan").join(&plan.name);
    fs::create_dir_all(&dir_obj)?;
    write_json(&dir_obj.join("desired.json"), &json!({ "hash": hash }))?;
    let actual = if dir_obj.join("actual.json").is_file() {
        let mut a: PlanActual = crate::read_json(&dir_obj.join("actual.json"))?;
        a.hash = hash.clone();
        a
    } else {
        PlanActual { hash: hash.clone(), product: String::new(), inputs: String::new() }
    };
    write_json(&dir_obj.join("actual.json"), &actual)?;
    write_json(&dir_obj.join("meta.json"), &Meta::new(KIND_PLAN, &plan.name, "mutate"))?;
    Ok(hash)
}

pub fn build_file(catalog_root: &Path, path: &Path) -> Result<BuildReport> {
    let (plan, bytes) = lint_path(path)?;
    build_inner(catalog_root, &plan, &file_hash(&bytes), None)
}

pub fn build_pinned(catalog_root: &Path, name: &str) -> Result<BuildReport> {
    let cat = Catalog::open(catalog_root)?;
    let id = ObjectId::new(KIND_PLAN, name);
    let obj = cat.get(&id)?;
    let hash = obj
        .desired
        .get("hash")
        .and_then(|h| h.as_str())
        .unwrap_or("")
        .to_string();
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
    build_inner(catalog_root, &plan, &hash, Some(&id))
}

fn build_inner(
    catalog_root: &Path,
    plan: &PlanFile,
    plan_hash: &str,
    plan_id: Option<&ObjectId>,
) -> Result<BuildReport> {
    let inputs = input_set_hash(&plan.build_needs);
    if let Some(id) = plan_id {
        if let Ok(obj) = Catalog::open(catalog_root)?.get(id) {
            let prev_p = obj.actual.get("product").and_then(|v| v.as_str()).unwrap_or("");
            let prev_i = obj.actual.get("inputs").and_then(|v| v.as_str()).unwrap_or("");
            if prev_i == inputs && is_realization_id(prev_p) {
                let dest = realization_dir(catalog_root, &plan.name, prev_p);
                if dest.is_dir() {
                    return Ok(BuildReport {
                        name: plan.name.clone(),
                        plan_hash: plan_hash.to_string(),
                        product: prev_p.to_string(),
                        inputs,
                        skipped: true,
                    });
                }
            }
        }
    }

    for n in &plan.build_needs {
        ensure_need(catalog_root, n)?;
    }

    let scratch = catalog_root.join("run/plan-build").join(&plan.name);
    let _ = fs::remove_dir_all(&scratch);
    let out = scratch.join("out");
    fs::create_dir_all(out.join("bin"))?;
    let script = fmt_plan(plan);
    let script_path = scratch.join("build.sh");
    let body = format!("#!/bin/sh\nset -eu\n{}", plan.script);
    fs::write(&script_path, if plan.script.is_empty() { "#!/bin/sh\nset -eu\n".into() } else { body })?;
    let mut perm = fs::metadata(&script_path)?.permissions();
    perm.set_mode(0o755);
    fs::set_permissions(&script_path, perm)?;

    let packs: Vec<(PkgNeed, PathBuf)> = plan
        .build_needs
        .iter()
        .map(|n| {
            let name = n.id.strip_prefix("pkg:").unwrap();
            let p = realization_dir(catalog_root, name, &n.hash);
            (n.clone(), p)
        })
        .collect();

    run_sandbox(&packs, &script_path, &out)?;

    let product = hash_tree(&out)?;
    if let Some(id) = plan_id {
        if let Ok(obj) = Catalog::open(catalog_root)?.get(id) {
            let prev_p = obj.actual.get("product").and_then(|v| v.as_str()).unwrap_or("");
            let prev_i = obj.actual.get("inputs").and_then(|v| v.as_str()).unwrap_or("");
            if prev_i == inputs && is_realization_id(prev_p) && prev_p != product {
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
        copy_tree(&out, &dest)?;
    }

    ensure_pkg_object(catalog_root, plan, &product)?;
    if let Some(id) = plan_id {
        let dir = catalog_root.join("objects/plan").join(&id.name);
        write_json(
            &dir.join("actual.json"),
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
    })
}

fn ensure_pkg_object(catalog_root: &Path, plan: &PlanFile, product: &str) -> Result<()> {
    let dir = catalog_root.join("objects/pkg").join(&plan.name);
    if dir.join("desired.json").is_file() {
        return Ok(());
    }
    fs::create_dir_all(&dir)?;
    let mut desired = json!({ "present": false, "hash": product, "removable": true });
    if !plan.run_needs.is_empty() {
        desired["needs"] = serde_json::to_value(&plan.run_needs)?;
    }
    write_json(&dir.join("desired.json"), &desired)?;
    write_json(
        &dir.join("actual.json"),
        &json!({ "present": false, "links": [], "removable": true }),
    )?;
    write_json(&dir.join("meta.json"), &Meta::new(KIND_PKG, &plan.name, "mutate"))?;
    Ok(())
}

fn ensure_need(catalog_root: &Path, n: &PkgNeed) -> Result<()> {
    let name = n.id.strip_prefix("pkg:").ok_or_else(|| {
        Error::hint(format!("build_needs id must be pkg:*, got {}", n.id), "oath schema plan")
    })?;
    if store_present(catalog_root, name, &n.hash) {
        let got = hash_tree(&realization_dir(catalog_root, name, &n.hash))?;
        if got != n.hash {
            return Err(Error::hint(
                format!("{} tree hash is {got}, plan wants {}", n.id, n.hash),
                "oath schema plan",
            ));
        }
        return Ok(());
    }
    let cat = Catalog::open(catalog_root)?;
    let obj = cat.get(&ObjectId::new(KIND_PKG, name)).ok();
    let url = obj
        .as_ref()
        .and_then(|o| o.desired.get("url").and_then(|u| u.as_str()))
        .unwrap_or("");
    if url.is_empty() {
        return Err(Error::hint(
            format!("{} @ {} is not in the store", n.id, n.hash),
            "fetch it (pkg.url) or pack it first",
        ));
    }
    fetch_pkg(catalog_root, name, url, &n.hash)?;
    if !store_present(catalog_root, name, &n.hash) {
        return Err(Error::hint(
            format!("fetch {} did not produce {}", n.id, n.hash),
            "oath schema pkg",
        ));
    }
    Ok(())
}

fn run_sandbox(packs: &[(PkgNeed, PathBuf)], script: &Path, out: &Path) -> Result<()> {
    let scratch = out.parent().unwrap().join("root");
    fs::create_dir_all(&scratch)?;
    sandbox_child(packs, script, out, &scratch)?;
    Ok(())
}

fn sandbox_child(
    packs: &[(PkgNeed, PathBuf)],
    script: &Path,
    out: &Path,
    scratch: &Path,
) -> Result<bool> {
    let packs: Vec<(String, String, PathBuf)> = packs
        .iter()
        .map(|(n, p)| {
            (
                n.id.strip_prefix("pkg:").unwrap_or(&n.id).to_string(),
                n.hash.clone(),
                p.clone(),
            )
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
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(Error::Msg("fork failed".into()));
    }
    if pid > 0 {
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
            format!(
                "sandbox exit status={st} signaled={} extra={extra}",
                libc::WIFSIGNALED(st)
            ),
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

    let log = scratch.join("err");
    let code = match sandbox_in_child(&packs, &script, &out, &scratch, host_root) {
        Ok(()) => 0,
        Err(e) => {
            let _ = fs::write(&log, format!("{e}"));
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
) -> Result<()> {
    let ns = libc::CLONE_NEWNS | libc::CLONE_NEWNET | libc::CLONE_NEWIPC | libc::CLONE_NEWUTS;
    if unsafe { libc::unshare(ns) } != 0 {
        return Err(Error::hint(
            "unshare failed (need mount/net namespaces)",
            "oath schema plan",
        ));
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
    fs::create_dir_all(scratch.join("out"))?;
    fs::create_dir_all(scratch.join("tmp"))?;
    fs::create_dir_all(scratch.join("dev"))?;
    fs::create_dir_all(scratch.join("oath/store/pkg"))?;
    // Host tests copy dynamically linked /bin/sh from NixOS. Appliance
    // packs are musl/static or declare pkg:glibc. /nix/store is absent on Oath.
    if Path::new("/nix/store").is_dir() {
        fs::create_dir_all(scratch.join("nix/store"))?;
        bind(Path::new("/nix/store"), &scratch.join("nix/store"), true)?;
    }

    bind(out, &scratch.join("out"), false)?;
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
        let bin = dest.join("bin");
        if bin.is_dir() {
            for e in fs::read_dir(&bin)? {
                let e = e?;
                let link = scratch.join("bin").join(e.file_name());
                if link.exists() {
                    continue;
                }
                let target = PathBuf::from("/oath/store/pkg")
                    .join(name)
                    .join(hash)
                    .join("bin")
                    .join(e.file_name());
                symlink(&target, &link)?;
            }
        }
        let live = scratch.join("oath/store/pkg").join(name).join(LIVE_NAME);
        if !live.exists() {
            let _ = symlink(Path::new(hash), &live);
        }
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
    std::env::set_current_dir("/")?;
    // Drop to seat `home` only when we entered as host root (not a user ns).
    if host_root {
        unsafe {
            libc::setgid(crate::seat::GID);
            libc::setuid(crate::seat::UID);
        }
    }
    std::env::set_var("PATH", "/bin");
    std::env::set_var("HOME", "/tmp");
    std::env::set_var("TMPDIR", "/tmp");
    std::env::remove_var("HTTP_PROXY");
    std::env::remove_var("http_proxy");
    std::env::remove_var("HTTPS_PROXY");
    std::env::remove_var("ALL_PROXY");

    let st = Command::new("/bin/sh")
        .arg("/build.sh")
        .env_clear()
        .env("PATH", "/bin")
        .env("HOME", "/tmp")
        .env("TMPDIR", "/tmp")
        .env("OUT", "/out")
        .stdin(Stdio::null())
        .status()?;
    if !st.success() {
        let _ = fs::write("/out/.err", format!("build.sh {st:?}"));
        return Err(Error::hint("plan script failed", "oath schema plan"));
    }
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

fn fetch_pkg(catalog_root: &Path, name: &str, url: &str, hash: &str) -> Result<()> {
    if store_present(catalog_root, name, hash) {
        return Ok(());
    }
    let url = fetch_url(url, name, hash);
    if url.is_empty() {
        return Ok(());
    }
    let tmpdir = catalog_root.join("store/pkg").join(format!(".{name}.wget"));
    let _ = fs::remove_dir_all(&tmpdir);
    fs::create_dir_all(&tmpdir)?;
    let blob = tmpdir.join("blob");
    let wget = ["/bin/wget", "/usr/bin/wget"]
        .iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("wget"));
    let st = Command::new(&wget)
        .args(["-q", "-O", blob.to_str().unwrap(), &url])
        .status()
        .map_err(|e| Error::Msg(format!("wget: {e}")))?;
    if !st.success() {
        let _ = fs::remove_dir_all(&tmpdir);
        return Err(Error::hint(format!("fetch pkg:{name} failed"), "oath schema pkg"));
    }
    let tree = tmpdir.join("tree");
    fs::create_dir_all(&tree)?;
    let gzip = {
        let mut fd = fs::File::open(&blob)?;
        let mut b = [0u8; 2];
        matches!(std::io::Read::read(&mut fd, &mut b), Ok(2) if b == [0x1f, 0x8b])
    };
    let mut tar = Command::new("tar");
    tar.arg("-C").arg(&tree);
    if gzip {
        tar.arg("-xzf");
    } else {
        tar.arg("-xf");
    }
    tar.arg(&blob);
    let st = tar.status().map_err(|e| Error::Msg(format!("tar: {e}")))?;
    if !st.success() {
        let _ = fs::remove_dir_all(&tmpdir);
        return Err(Error::hint("fetch pack tar extract failed", "oath schema pkg"));
    }
    let result = crate::pkg::install_tree(catalog_root, name, &tree, None, hash);
    let _ = fs::remove_dir_all(&tmpdir);
    result.map(|_| ())
}
