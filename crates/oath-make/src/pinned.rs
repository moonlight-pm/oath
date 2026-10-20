//! Packs a seed profile pins that Oath did not build (R28): a `pkg` entry
//! with a `hash`, its `needs` and optionally a `url`, and no `pkgs/<name>/`.
//! Its owner builds and publishes it. The image build fetches it from its
//! `url` or the profile's origins into the host store, `cargo make update`
//! keeps the pin a machine has, and `cargo make publish` leaves it out.
//! When a pack it needs was rebuilt, the image moves the need to the new
//! hash and says so: the owner has to build it again against this desk.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

/// One pack a profile pins from elsewhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pinned {
    pub hash: String,
    /// `(name, hash)` of each runtime need, as the profile pins it.
    pub needs: Vec<(String, String)>,
    pub url: String,
}

/// A need whose pin is not what the image ships.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stale {
    pub pkg: String,
    pub need: String,
    pub pinned: String,
    pub ships: String,
}

/// The packs `profile` pins from elsewhere, by name.
pub fn of_profile(repo: &Path, profile: &str) -> Result<BTreeMap<String, Pinned>> {
    let p = oath_core::Profile::load(profile).map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut out = BTreeMap::new();
    for (name, doc) in p.pinned_pkgs() {
        if crate::pkgs::pkgs_dir(repo).join(&name).is_dir() {
            bail!(
                "profile {profile} pins pkg:{name}, and pkgs/{name} builds it: a package is built \
                 here or pinned from elsewhere, not both"
            );
        }
        out.insert(name.clone(), parse(&name, &doc).with_context(|| format!("profile {profile}"))?);
    }
    Ok(out)
}

/// Every pack any profile pins from elsewhere.
pub fn all(repo: &Path) -> Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for (profile, _) in oath_core::PROFILES {
        out.extend(of_profile(repo, profile)?.into_keys());
    }
    Ok(out)
}

fn parse(name: &str, doc: &Value) -> Result<Pinned> {
    let hash = doc["hash"].as_str().unwrap_or_default().to_string();
    if !oath_core::is_realization_id(&hash) {
        bail!("pkg:{name}: hash {hash:?} is not sha256-<64 hex>");
    }
    let mut needs = Vec::new();
    for n in doc["needs"].as_array().into_iter().flatten() {
        let (Some(id), Some(h)) = (n["id"].as_str(), n["hash"].as_str()) else {
            bail!("pkg:{name}: a pinned pack's needs are {{\"id\", \"hash\"}}, not {n}");
        };
        if !oath_core::is_realization_id(h) {
            bail!("pkg:{name}: need {id} has no hash");
        }
        needs.push((id.trim_start_matches("pkg:").to_string(), h.to_string()));
    }
    Ok(Pinned { hash, needs, url: doc["url"].as_str().unwrap_or_default().to_string() })
}

/// Put each pinned pack in the host store (`cache`), unless it is there: from its
/// `url`, else from `origins` in order, each refused unless it serves the
/// pinned tree. Returns each pack's tree.
pub fn fetch(
    cache: &Path,
    pins: &BTreeMap<String, Pinned>,
    origins: &[String],
) -> Result<BTreeMap<String, PathBuf>> {
    let catalog = cache.parent().context("the host store has no parent")?.to_path_buf();
    let mut out = BTreeMap::new();
    for (name, p) in pins {
        let tree = cache.join("pkg").join(name).join(&p.hash);
        if !tree.is_dir() {
            let from: Vec<String> =
                if p.url.is_empty() { origins.to_vec() } else { vec![p.url.clone()] };
            let mut failed = Vec::new();
            for o in &from {
                match fetch_one(&catalog, name, o, &p.hash) {
                    Ok(()) => {
                        eprintln!(">> pkg:{name} {} from {o}", p.hash);
                        break;
                    }
                    Err(e) => failed.push(format!("{o}: {e:#}")),
                }
            }
            if !tree.is_dir() {
                bail!(
                    "pkg:{name} {} is pinned from elsewhere, and nowhere served it ({})",
                    p.hash,
                    if failed.is_empty() {
                        "no url and no origins".into()
                    } else {
                        failed.join("; ")
                    }
                );
            }
        }
        out.insert(name.clone(), tree);
    }
    Ok(out)
}

/// Fetch `{origin}/pkg/<name>/<hash>.tar` (or a `.tar` url) and store it
/// with `oath store`'s refusals; a tree that is not the pin is not kept.
fn fetch_one(catalog: &Path, name: &str, origin: &str, hash: &str) -> Result<()> {
    let url = oath_core::fetch_url(origin, name, hash);
    let tmp = tempfile::tempdir()?;
    let blob = tmp.path().join("pack.tar");
    crate::pkgs::download(&url, &blob, true)?;
    let slot = catalog.join("store/pkg").join(name);
    let before: BTreeSet<String> = fs::read_dir(&slot)
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    let got = oath_core::store_pkg(catalog, name, &blob).map_err(|e| anyhow::anyhow!("{e}"))?;
    if got != hash {
        if !before.contains(&got) {
            let _ = fs::remove_dir_all(slot.join(&got));
        }
        bail!("{url} is {got}, not the pin");
    }
    Ok(())
}

/// The needs each pinned pack names at another hash than the image ships
/// (`ships(need)`), so it was built against packs since rebuilt.
pub fn stale(
    pins: &BTreeMap<String, Pinned>,
    ships: impl Fn(&str) -> Option<String>,
) -> Vec<Stale> {
    let mut out = Vec::new();
    for (pkg, p) in pins {
        for (need, pinned) in &p.needs {
            if let Some(s) = ships(need).filter(|s| s != pinned) {
                out.push(Stale {
                    pkg: pkg.clone(),
                    need: need.clone(),
                    pinned: pinned.clone(),
                    ships: s,
                });
            }
        }
    }
    out
}

/// The manifest's entry for a pinned pack.
pub fn manifest_entry(p: &Pinned, stale: &[Stale], places: &BTreeSet<&str>) -> Value {
    let mut v = json!({
        "hash": p.hash,
        "pinned": true,
        "needs": p.needs.iter().map(|(n, h)| json!({ "id": format!("pkg:{n}"), "hash": h }))
            .collect::<Vec<_>>(),
        "ships_in": places,
    });
    if !p.url.is_empty() {
        v["url"] = json!(p.url);
    }
    if !stale.is_empty() {
        v["stale_needs"] = json!(stale
            .iter()
            .map(
                |s| json!({ "id": format!("pkg:{}", s.need), "pinned": s.pinned, "ships": s.ships })
            )
            .collect::<Vec<_>>());
    }
    v
}

/// Say, from a build output's manifest, which pinned packs were built
/// against packs this output ships at other hashes.
pub fn warn_stale(manifest: &Value) {
    for (name, e) in manifest["packs"].as_object().into_iter().flatten() {
        for s in e["stale_needs"].as_array().into_iter().flatten() {
            eprintln!(
                "warning: pkg:{name} (pinned, built elsewhere) was built against {} {}; this \
                 output ships {}. The image moves the need; pkg:{name}'s owner should build it \
                 again against this desk.",
                s["id"].as_str().unwrap_or("?"),
                s["pinned"].as_str().unwrap_or("?"),
                s["ships"].as_str().unwrap_or("?"),
            );
        }
    }
}

/// The packages a build output's profile pins from elsewhere, shipped
/// or not (its manifest's `pinned`).
pub fn in_manifest(manifest: &Value) -> BTreeSet<String> {
    manifest["pinned"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n.as_str().map(str::to_string))
        .collect()
}

/// What `cargo make update` sets on a machine's pinned package that is
/// not at the output's pin: the machine's own pin stays, and each need the
/// output ships at another hash moves to it. `None` when nothing moves.
/// `machine` is the machine's desired doc; `ships(need)` is the output's pin.
pub fn keep_machine_pin(
    machine: &Value,
    ships: impl Fn(&str) -> Option<String>,
) -> Option<(Value, Vec<String>)> {
    let mut needs = machine["needs"].as_array().cloned().unwrap_or_default();
    let mut moved = Vec::new();
    for n in needs.iter_mut() {
        let Some(id) = n["id"].as_str().map(str::to_string) else { continue };
        let name = id.trim_start_matches("pkg:");
        if let Some(s) = ships(name).filter(|s| n["hash"].as_str() != Some(s.as_str())) {
            moved.push(format!("{id} {} -> {s}", n["hash"].as_str().unwrap_or("")));
            n["hash"] = json!(s);
        }
    }
    (!moved.is_empty()).then(|| (json!({ "needs": needs }), moved))
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "sha256-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "sha256-cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

    #[test]
    fn parse_a_pin() {
        let doc = json!({
            "present": true,
            "hash": A,
            "needs": [{ "id": "pkg:wayland", "hash": B }],
            "url": "https://example.invalid/sola"
        });
        let p = parse("sola", &doc).unwrap();
        assert_eq!(p.hash, A);
        assert_eq!(p.needs, vec![("wayland".to_string(), B.to_string())]);
        assert!(parse("sola", &json!({ "hash": "sha256-1" })).is_err());
        assert!(parse("sola", &json!({ "hash": A, "needs": ["pkg:wayland"] })).is_err());
    }

    #[test]
    fn stale_needs_and_the_machine_pin() {
        let mut pins = BTreeMap::new();
        pins.insert(
            "sola".to_string(),
            Pinned {
                hash: A.into(),
                needs: vec![("wayland".into(), B.into()), ("mesa".into(), C.into())],
                url: String::new(),
            },
        );
        let ships = |n: &str| match n {
            "wayland" => Some(C.to_string()),
            "mesa" => Some(C.to_string()),
            _ => None,
        };
        let s = stale(&pins, ships);
        assert_eq!(
            s,
            vec![Stale {
                pkg: "sola".into(),
                need: "wayland".into(),
                pinned: B.into(),
                ships: C.into()
            }]
        );
        let e = manifest_entry(&pins["sola"], &s, &BTreeSet::from(["rootfs"]));
        assert_eq!(e["pinned"], json!(true));
        assert_eq!(e["stale_needs"][0]["id"], json!("pkg:wayland"));
        let m = json!({ "pinned": ["sola", "sola-src"], "packs": { "sola": e, "bash": { "hash": A } } });
        assert_eq!(in_manifest(&m), BTreeSet::from(["sola".to_string(), "sola-src".to_string()]));
        // A machine keeps its own pin; only the needs this output moved move.
        let machine = json!({
            "present": true,
            "hash": B,
            "needs": [{ "id": "pkg:wayland", "hash": B }, { "id": "pkg:mesa", "hash": C }]
        });
        let (set, moved) = keep_machine_pin(&machine, ships).unwrap();
        assert_eq!(
            set,
            json!({ "needs": [{ "id": "pkg:wayland", "hash": C }, { "id": "pkg:mesa", "hash": C }] })
        );
        assert_eq!(moved.len(), 1);
        assert!(keep_machine_pin(&machine, |_| None).is_none());
    }

    #[test]
    fn fetch_from_an_origin_into_the_host_store() {
        let d = tempfile::tempdir().unwrap();
        let tree = d.path().join("tree");
        fs::create_dir_all(tree.join("bin")).unwrap();
        fs::write(tree.join("bin/sola"), b"sola\n").unwrap();
        let pin = oath_core::hash_tree(&tree).unwrap();
        let origin = d.path().join("origin");
        fs::create_dir_all(origin.join("pkg/sola")).unwrap();
        let tar = origin.join("pkg/sola").join(format!("{pin}.tar"));
        let st = std::process::Command::new("tar")
            .args(["-C", tree.to_str().unwrap(), "-cf", tar.to_str().unwrap(), "."])
            .status()
            .unwrap();
        assert!(st.success());
        let store = d.path().join("host/store");
        fs::create_dir_all(&store).unwrap();
        let mut pins = BTreeMap::new();
        pins.insert(
            "sola".to_string(),
            Pinned { hash: pin.clone(), needs: vec![], url: String::new() },
        );
        let gone = format!("file://{}/gone", d.path().display());
        let good = format!("file://{}", origin.display());
        let got = fetch(&store, &pins, &[gone.clone(), good]).unwrap();
        assert_eq!(got["sola"], store.join("pkg/sola").join(&pin));
        assert!(got["sola"].join("bin/sola").is_file());
        // Nothing but the realization: no live link, no object.
        let names: Vec<String> = fs::read_dir(store.join("pkg/sola"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![pin.clone()]);
        // A pin nothing serves is an error that names where it looked.
        pins.get_mut("sola").unwrap().hash = A.into();
        let e = format!("{:#}", fetch(&store, &pins, &[gone.clone()]).unwrap_err());
        assert!(e.contains("nowhere served it") && e.contains(&gone), "{e}");
    }
}
