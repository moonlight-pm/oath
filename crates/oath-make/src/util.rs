use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

pub fn repo_root() -> PathBuf {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_dir.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf()).unwrap_or(crate_dir)
}

pub fn out_dir(root: &Path) -> PathBuf {
    std::env::var("OATH_BUILD").map(PathBuf::from).unwrap_or_else(|_| root.join("build"))
}

pub fn utc_stamp() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

pub fn utc_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn run(cmd: &mut Command) -> Result<()> {
    let status = cmd.status().with_context(|| format!("spawn {cmd:?}"))?;
    if !status.success() {
        bail!("{cmd:?} failed: {status}");
    }
    Ok(())
}

pub fn run_out(cmd: &mut Command) -> Result<String> {
    let out = cmd.output().with_context(|| format!("spawn {cmd:?}"))?;
    if !out.status.success() {
        bail!("{cmd:?} failed: {}\n{}", out.status, String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn copy_file(src: &Path, dst: &Path) -> Result<()> {
    if let Some(p) = dst.parent() {
        fs::create_dir_all(p)?;
    }
    fs::copy(src, dst).with_context(|| format!("copy {} -> {}", src.display(), dst.display()))?;
    Ok(())
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

pub fn sudo(args: &[&str]) -> Result<()> {
    let mut c = Command::new("sudo");
    c.arg("-n").args(args).stdin(Stdio::null());
    run(&mut c)
}

pub fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|p| {
            let c = p.join(name);
            c.is_file().then_some(c)
        })
    })
}

pub fn kvm() -> bool {
    Path::new("/dev/kvm").exists()
}

pub fn write_pretty(path: &Path, v: &impl serde::Serialize) -> Result<()> {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let mut s = serde_json::to_string_pretty(v)?;
    if !s.ends_with('\n') {
        s.push('\n');
    }
    fs::write(path, s)?;
    Ok(())
}

pub fn prepend_path(dir: &Path) {
    let mut paths = vec![dir.to_path_buf()];
    if let Some(p) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&p));
    }
    std::env::set_var("PATH", std::env::join_paths(paths).unwrap());
}

pub fn chmod_exec(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut p = fs::metadata(path)?.permissions();
    p.set_mode(p.mode() | 0o755);
    fs::set_permissions(path, p)
}
