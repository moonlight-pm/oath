//! Host publisher: query OSV from pack `components.json`, write
//! `~/.local/share/oath/store/adv/<name>/<hash>.json`. Guest never calls OSV.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use oath_core::{
    is_realization_id, read_components, write_advisories_store, PkgAdvisory, COMPONENTS_NAME,
};
use serde_json::Value;

use crate::pack::host_store_cache;

const DEFAULT_OSV: &str = "https://api.osv.dev/v1/query";

pub fn fill_advisories(repo: &Path, only: Option<&str>) -> Result<()> {
    let cache = host_store_cache(repo);
    let osv = std::env::var("OATH_OSV").unwrap_or_else(|_| DEFAULT_OSV.to_string());
    fill_advisories_at(&cache, only, &osv)
}

pub fn fill_advisories_at(cache: &Path, only: Option<&str>, osv: &str) -> Result<()> {
    let pkg_root = cache.join("pkg");
    if !pkg_root.is_dir() {
        bail!(
            "no host store cache at {} — cargo make store --name <n> --from <dir>",
            pkg_root.display()
        );
    }
    let mut names: Vec<String> = std::fs::read_dir(&pkg_root)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| only.map(|o| o == n || o == format!("pkg:{n}")).unwrap_or(true))
        .collect();
    names.sort();
    if names.is_empty() {
        bail!("no packs in {}", pkg_root.display());
    }
    let mut errors = 0u32;
    for name in names {
        let slot = pkg_root.join(&name);
        let mut hashes: Vec<String> = std::fs::read_dir(&slot)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| is_realization_id(n))
            .collect();
        hashes.sort();
        for hash in hashes {
            let tree = slot.join(&hash);
            if !tree.join(COMPONENTS_NAME).is_file() {
                println!("pkg:{name} {hash} no {COMPONENTS_NAME}");
                continue;
            }
            match query_tree(&tree, osv) {
                Ok(advs) => {
                    write_advisories_store(cache, &name, &hash, advs.clone())
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    if advs.is_empty() {
                        println!("pkg:{name} {hash} none");
                    } else {
                        println!("pkg:{name} {hash} {} advisories", advs.len());
                    }
                }
                Err(e) => {
                    eprintln!("pkg:{name} {hash} {e:#}");
                    errors += 1;
                }
            }
        }
    }
    if errors > 0 {
        bail!("{errors} pack(s) failed OSV query");
    }
    Ok(())
}

fn query_tree(tree: &Path, osv: &str) -> Result<Vec<PkgAdvisory>> {
    let comps = read_components(tree).context("components.json")?;
    let mut out: Vec<PkgAdvisory> = Vec::new();
    for c in comps {
        let vulns = query_osv(osv, &c.purl)?;
        out.extend(vulns);
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out.dedup_by(|a, b| a.id == b.id);
    Ok(out)
}

fn query_osv(osv: &str, purl: &str) -> Result<Vec<PkgAdvisory>> {
    let body = serde_json::json!({ "package": { "purl": purl } }).to_string();
    let raw = http_post_json(osv, &body)?;
    let v: Value = serde_json::from_str(&raw).with_context(|| format!("OSV JSON for {purl}"))?;
    let Some(arr) = v.get("vulns").and_then(|x| x.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for e in arr {
        let id = e.get("id").and_then(|x| x.as_str()).unwrap_or("").trim();
        if id.is_empty() {
            continue;
        }
        let summary = e.get("summary").and_then(|x| x.as_str()).unwrap_or("").trim();
        let mut summary = summary.to_string();
        if summary.len() > 240 {
            summary.truncate(240);
        }
        out.push(PkgAdvisory {
            id: id.to_string(),
            url: format!("https://osv.dev/vulnerability/{id}"),
            summary,
        });
    }
    Ok(out)
}

fn http_post_json(url: &str, body: &str) -> Result<String> {
    let curl = find_bin("curl");
    let mut child = Command::new(&curl)
        .args([
            "-sS",
            "-f",
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            "@-",
            "-o",
            "-",
            url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("curl {}", curl.display()))?;
    {
        let mut stdin = child.stdin.take().context("curl stdin")?;
        stdin.write_all(body.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("OSV query failed: {err}");
    }
    Ok(String::from_utf8(out.stdout)?)
}

fn find_bin(name: &str) -> PathBuf {
    for dir in ["/bin", "/usr/bin"] {
        let p = PathBuf::from(dir).join(name);
        if p.is_file() {
            return p;
        }
    }
    if let Some(p) = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path).find_map(|d| {
            let c = d.join(name);
            c.is_file().then_some(c)
        })
    }) {
        return p;
    }
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    fn serve_osv(body: &[u8]) -> (String, std::thread::JoinHandle<()>) {
        let body = body.to_vec();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let h = std::thread::spawn(move || {
            for _ in 0..8 {
                let Ok((mut s, _)) = listener.accept() else {
                    break;
                };
                let mut buf = [0u8; 8192];
                let _ = s.read(&mut buf);
                let hdr = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(hdr.as_bytes());
                let _ = s.write_all(&body);
            }
        });
        (format!("http://{addr}/v1/query"), h)
    }

    #[test]
    fn queries_osv_and_writes_adv() {
        let d = tempfile::tempdir().unwrap();
        let h = "sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let tree = d.path().join("pkg/curl").join(h);
        std::fs::create_dir_all(tree.join("bin")).unwrap();
        std::fs::write(tree.join("bin/curl"), b"x\n").unwrap();
        std::fs::write(
            tree.join(COMPONENTS_NAME),
            "{\n  \"components\": [\n    {\n      \"name\": \"curl\",\n      \"version\": \"8.5.0\",\n      \"purl\": \"pkg:generic/curl@8.5.0\"\n    }\n  ]\n}\n",
        )
        .unwrap();
        let osv_body = br#"{"vulns":[{"id":"CVE-TEST-1","summary":"fixture"}]}"#;
        let (osv, _srv) = serve_osv(osv_body);
        fill_advisories_at(d.path(), Some("curl"), &osv).unwrap();
        let got = oath_core::read_advisories_store(d.path(), "curl", h).expect("adv");
        assert_eq!(got[0].id, "CVE-TEST-1");
        assert_eq!(got[0].url, "https://osv.dev/vulnerability/CVE-TEST-1");
        assert_eq!(got[0].summary, "fixture");
    }

    #[test]
    fn missing_components_skips() {
        let d = tempfile::tempdir().unwrap();
        let h = "sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let tree = d.path().join("pkg/hello").join(h);
        std::fs::create_dir_all(tree.join("bin")).unwrap();
        std::fs::write(tree.join("bin/hello"), b"x\n").unwrap();
        let (osv, _srv) = serve_osv(b"{\"vulns\":[]}");
        fill_advisories_at(d.path(), Some("hello"), &osv).unwrap();
        assert!(oath_core::read_advisories_store(d.path(), "hello", h).is_none());
    }
}
