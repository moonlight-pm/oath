//! `oath reap` (R62): an Oath machine holds what its packages prescribe.
//! Without `--confirm` it says what holds each version and what would go.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use oath_core::reap::{self, Policy};
use oath_core::{Catalog, Error, Result};
use serde_json::json;

use crate::live::Live;

pub struct Opts {
    pub confirm: bool,
    pub arriving: i64,
    pub boot: bool,
    pub json: bool,
}

pub fn run(root: &Path, cat: &Catalog, opts: Opts) -> Result<i32> {
    if opts.confirm && unsafe { libc::geteuid() } != 0 {
        return Err(Error::hint("reaping removes trees and snapshots", "sudo oath reap --confirm"));
    }
    let live = Live::new(root);
    let policy = Policy {
        now: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64),
        arriving: opts.arriving,
        boot: reap::boot_time(),
        // A preview says how much; removing only takes it.
        sizes: !opts.confirm,
    };
    if !opts.confirm {
        let r = reap::survey(cat, &live.generations(), &policy)?;
        if opts.json {
            println!("{}", serde_json::to_string_pretty(&r)?);
        } else {
            for line in preview(&r) {
                println!("{line}");
            }
        }
        return Ok(0);
    }

    if opts.boot {
        leftovers(root);
    }
    reap::empty_trash(root)?;
    let lock = cat.lock()?;
    let r = reap::survey(cat, &live.generations(), &policy)?;
    if !r.building.is_empty() {
        return Err(Error::hint(
            format!("a build is underway ({}): what it reads may have no pin", r.building.join(" ")),
            "reap when it is done (a boot does)",
        ));
    }
    r.execute(cat, &lock)?;
    for g in &r.generations_gone {
        live.remove_generation(*g)?;
    }
    drop(lock);
    reap::empty_trash(root)?;
    let going = r.going().count();
    if opts.json {
        println!(
            "{}",
            json!({
                "versions": going,
                "generations": r.generations_gone.len(),
                "plans": r.plans_gone.len(),
                "slots": r.slots_gone.len(),
            })
        );
    } else {
        println!(
            "reaped {going} versions, {} generations, {} plans, {} store slots",
            r.generations_gone.len(),
            r.plans_gone.len(),
            r.slots_gone.len()
        );
    }
    Ok(0)
}

/// What a boot finds and nothing is using any more: an update's or a
/// build's working directories, from before this boot.
fn leftovers(root: &Path) {
    for dir in ["run/update", "run/plan-build"] {
        let _ = fs::remove_dir_all(root.join(dir));
    }
    let _ = fs::remove_dir(root.join("run/update-esp"));
}

fn preview(r: &reap::Reap) -> Vec<String> {
    let mut out = Vec::new();
    let mut by_pkg: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
    for v in r.going() {
        let e = by_pkg.entry(&v.id).or_default();
        e.0 += 1;
        e.1 += v.size.unwrap_or(0);
    }
    let gb = |b: u64| b as f64 / 1e9;
    let total: u64 = by_pkg.values().map(|(_, b)| b).sum();
    for (id, (n, bytes)) in &by_pkg {
        out.push(format!("{id}: {n} versions, {:.2} GB", gb(*bytes)));
    }
    if !r.slots_gone.is_empty() {
        out.push(format!("store slots with no object: {}", r.slots_gone.join(" ")));
    }
    if !r.plans_gone.is_empty() {
        out.push(format!("{} filed plans nothing pins", r.plans_gone.len()));
    }
    let kept: Vec<String> = r.generations_kept.iter().map(u64::to_string).collect();
    out.push(format!(
        "generations: {} go, {} stay ({})",
        r.generations_gone.len(),
        kept.len(),
        kept.join(" ")
    ));
    if !r.building.is_empty() {
        out.push(format!("a build is underway, so nothing goes yet: {}", r.building.join(" ")));
    }
    if r.is_empty() {
        out.push("nothing to reap".to_string());
    } else {
        out.push(format!(
            "{:.2} GB of trees would go, freed once no snapshot holds them; \
             `sudo oath reap --confirm` removes them",
            gb(total)
        ));
    }
    out
}
