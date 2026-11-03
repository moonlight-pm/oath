//! `oath audit` (R48, R61): the daily security audit's report, from the
//! first origin whose copy a key in `host:local.publishers` signed. It
//! speaks for the version and build it names: a package at another version
//! here was not assessed, which is not the same as clean.

use std::collections::BTreeMap;
use std::fs;

use oath_core::update as up;
use oath_core::{Catalog, Error, ObjectId, Result};
use serde_json::{json, Value};

use crate::update::{fetch, trust};

pub fn run(cat: &Catalog, id: Option<&ObjectId>, as_json: bool) -> Result<i32> {
    let (origins, trusted) = trust(cat)?;
    let (mut report, origin, signer) = fetch_report(&origins, &trusted)?;
    if let Some(id) = id {
        if let Some(p) = report["packages"].as_array_mut() {
            p.retain(|p| report_name_is(p, &id.name));
        }
    }
    if as_json {
        report["origin"] = json!(origin);
        report["signer"] = json!(signer);
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(0);
    }
    let mut here = BTreeMap::new();
    for pkg in cat.ls(Some("pkg"))? {
        if id.is_none_or(|i| *i == pkg) {
            let actual = cat.get(&pkg)?.actual;
            if actual["present"] == json!(true) {
                let comps = serde_json::from_value(actual["components"].clone()).unwrap_or_default();
                let comps: Vec<oath_core::PkgComponent> = comps;
                here.insert(pkg.name.clone(), oath_core::own_version(&comps, &pkg.name));
            }
        }
    }
    let commit = report["commit"].as_str().unwrap_or("");
    println!(
        "audit of {}, commit {}, signed by {signer}, from {origin}",
        report["generated"].as_str().unwrap_or("an unknown time"),
        &commit[..commit.len().min(12)]
    );
    for line in lines(&report, &here) {
        println!("{line}");
    }
    Ok(0)
}

fn report_name_is(p: &Value, name: &str) -> bool {
    p["name"].as_str() == Some(report_name(name))
}

/// The report's name for a package: an `-i386` one is its sibling's, the
/// same sources.
fn report_name(name: &str) -> &str {
    name.strip_suffix("-i386").filter(|b| *b != "glibc").unwrap_or(name)
}

fn fetch_report(origins: &[String], trusted: &[String]) -> Result<(Value, String, String)> {
    let work = std::env::temp_dir().join(format!("oath-audit-{}", std::process::id()));
    fs::create_dir_all(&work)?;
    let (doc, sig) = (work.join("report.json"), work.join("report.json.sig"));
    let mut why = Vec::new();
    let mut found = None;
    for o in origins {
        let o = o.trim_end_matches('/');
        let got = fetch(&format!("{o}/{}", up::AUDIT_KEY), &doc)
            .and_then(|_| fetch(&format!("{o}/{}.sig", up::AUDIT_KEY), &sig))
            .and_then(|_| {
                let bytes = fs::read(&doc)?;
                let signer = oath_core::sshsig::verify(
                    &bytes,
                    &fs::read_to_string(&sig)?,
                    up::AUDIT_NAMESPACE,
                    trusted,
                )?;
                Ok((serde_json::from_slice::<Value>(&bytes)?, signer))
            });
        match got {
            Ok((report, signer)) => {
                found = Some((report, o.to_string(), signer));
                break;
            }
            Err(e) => why.push(format!("{o}: {e}")),
        }
    }
    let _ = fs::remove_dir_all(&work);
    found.ok_or_else(|| {
        Error::hint(
            format!("no origin served an audit report this machine trusts:\n  {}", why.join("\n  ")),
            "oath get host:local (origins, publishers)",
        )
    })
}

/// What the report says of the packages `here` (name to version), for a
/// person. A source pack (`-src`) counts as nothing: its product is what
/// runs.
fn lines(report: &Value, here: &BTreeMap<String, Option<String>>) -> Vec<String> {
    let mut out = Vec::new();
    for (source, status) in report["sources"].as_object().into_iter().flatten() {
        if let Some(s) = status.as_str().filter(|s| s.starts_with("error")) {
            out.push(format!("a source failed, so this may miss findings: {source}: {s}"));
        }
    }
    let packages: BTreeMap<&str, &Value> = report["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p["name"].as_str()?, p)))
        .collect();
    let (mut read, mut absent) = (0, Vec::new());
    for (name, version) in here.iter().filter(|(n, _)| !n.ends_with("-src")) {
        let Some(p) = packages.get(report_name(name)) else {
            absent.push(name.as_str());
            continue;
        };
        let theirs = p["version"].as_str().unwrap_or("");
        let at = version.as_deref().unwrap_or("an unknown version");
        if theirs != at && !name.ends_with("-i386") {
            out.push(format!("pkg:{name} is {at} here; the audit read {theirs}: not assessed"));
            continue;
        }
        read += 1;
        for h in p["hits"].as_array().into_iter().flatten() {
            let fixed: Vec<&str> = h["fixed"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let severity = h["severity"].as_str().unwrap_or("");
            out.push(format!(
                "pkg:{name} {at}: {}{}{} {}{}{}",
                h["id"].as_str().unwrap_or("?"),
                if severity.is_empty() { "" } else { " " },
                severity,
                h["status"].as_str().unwrap_or("unknown"),
                if h["kev"] == json!(true) { ", known exploited" } else { "" },
                if fixed.is_empty() {
                    String::new()
                } else {
                    format!(", fixed in {}", fixed.join(" "))
                }
            ));
        }
        if let Some(n) = p["newer"].as_str() {
            out.push(format!("pkg:{name} {at}: upstream has {n}"));
        }
    }
    out.push(format!("{read} packages read"));
    if !absent.is_empty() {
        out.push(format!("not in the report: {}", absent.join(" ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_speaks_only_for_the_version_it_read() {
        let report = json!({
            "sources": { "OSV": "ok, 3 queries", "NVD": "error: timed out" },
            "packages": [
                { "name": "tar", "version": "1.35", "newer": "1.36", "hits": [
                    { "id": "CVE-1", "severity": "HIGH", "status": "affected", "kev": true,
                      "fixed": ["1.36"] }] },
                { "name": "curl", "version": "8.9", "hits": [{ "id": "CVE-2", "status": "affected" }] },
                { "name": "zlib", "version": "1.3", "hits": [{ "id": "CVE-3", "status": "unknown" }] },
            ],
        });
        let v = |s: &str| Some(s.to_string());
        let here = BTreeMap::from([
            ("tar".to_string(), v("1.35")),
            ("curl".to_string(), v("8.8")),
            ("zlib-i386".to_string(), v("1.3")),
            ("lzo".to_string(), v("2.10")),
            ("steam-src".to_string(), None),
        ]);
        assert_eq!(
            lines(&report, &here),
            [
                "a source failed, so this may miss findings: NVD: error: timed out",
                "pkg:curl is 8.8 here; the audit read 8.9: not assessed",
                "pkg:tar 1.35: CVE-1 HIGH affected, known exploited, fixed in 1.36",
                "pkg:tar 1.35: upstream has 1.36",
                "pkg:zlib-i386 1.3: CVE-3 unknown",
                "2 packages read",
                "not in the report: lzo",
            ]
        );
    }

    #[test]
    fn an_i386_package_is_its_siblings() {
        assert_eq!(report_name("mesa-i386"), "mesa");
        assert_eq!(report_name("glibc-i386"), "glibc-i386");
        assert_eq!(report_name("tar"), "tar");
    }
}
