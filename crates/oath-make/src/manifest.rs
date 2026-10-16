//! `build/manifest.json`, written by `cargo make build` beside the image:
//! every pack the image ships, where it ships, and what it was built from
//! (hash, version, licence, plan, and its source pins with their signing
//! keys, or for pkg:oath the commit of this tree).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::pack::INSTALLER_TOOLS;
use crate::tools::Tools;
use crate::util::{run_out, write_pretty};

pub fn write(root: &Path, out: &Path, tools: &Tools, profile: &str) -> Result<()> {
    // Where each pack ships: `rootfs` (the profile's store), `initrd`,
    // `installer` (the install initramfs's own tools), `esp`.
    let mut ships: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut at = |name: &str, place| {
        ships.entry(name.to_string()).or_default().insert(place);
    };
    let wanted =
        oath_core::Profile::load(profile).map_err(|e| anyhow::anyhow!("{e}"))?.present_pkgs();
    for name in &wanted {
        at(name, "rootfs");
    }
    at("busybox", "initrd");
    at("oath", "initrd");
    if tools.firmware().is_some() {
        at("firmware", "initrd");
    }
    // OATH_KERNEL / OATH_MODULES may name a kernel that is not pkg:linux.
    if tools.packs.get("linux").is_some_and(|p| tools.kernel.starts_with(p)) {
        at("linux", "initrd");
        at("linux", "esp");
    }
    for (pack, _) in INSTALLER_TOOLS {
        at(pack, "installer");
    }
    at("oath", "installer");
    at("oath", "esp");

    let lock = crate::pkgs::read_lock(root)?;
    let all = crate::pkgs::load(root)?;
    let mut packs = serde_json::Map::new();
    for (name, places) in ships {
        let entry = if name == "oath" {
            let b = crate::oath_pkg::built(root)?;
            let tree = crate::pack::host_store_cache(root).join("pkg/oath").join(&b.product);
            json!({
                "hash": b.product,
                "version": crate::oath_pkg::version(root)?,
                "license": crate::oath_pkg::license(root, &tree)?,
                "plan": b.plan,
                "ships_in": places,
                "commit": commit(root),
            })
        } else {
            let e =
                lock.get(&name).with_context(|| format!("pkg:{name} is not in pkgs/lock.json"))?;
            let p = all.get(&name).with_context(|| format!("pkgs/{name}/pkg.toml"))?;
            let sources: Vec<Value> = p
                .source
                .iter()
                .map(|s| {
                    let mut v = json!({ "url": s.url, "sha256": s.sha256 });
                    match (&s.key, s.unsigned.as_ref().or(p.unsigned.as_ref())) {
                        (Some(k), _) => v["key"] = json!(format!("pkgs/keys/{k}.asc")),
                        (None, Some(why)) => v["unsigned"] = json!(why),
                        (None, None) => {}
                    }
                    v
                })
                .collect();
            json!({
                "hash": e.product,
                "version": e.version,
                "license": e.license,
                "plan": e.plan,
                "ships_in": places,
                "sources": sources,
            })
        };
        packs.insert(name, entry);
    }
    let kernel = crate::kernel::check(&tools.kernel, &tools.modules)?;
    write_pretty(
        &out.join("manifest.json"),
        &json!({ "profile": profile, "kernel": kernel, "packs": packs }),
    )
}

/// HEAD of this tree, with `-dirty` when what pkg:oath packs has changed.
fn commit(root: &Path) -> String {
    let git = || {
        let mut c = Command::new("git");
        c.arg("-C").arg(root);
        c
    };
    let Ok(head) = run_out(git().args(["rev-parse", "HEAD"])) else {
        return "unknown".into();
    };
    let dirty = run_out(git().args(["status", "--porcelain", "--"]).args(crate::oath_pkg::TREE))
        .map_or(true, |s| !s.is_empty());
    if dirty {
        format!("{head}-dirty")
    } else {
        head
    }
}
