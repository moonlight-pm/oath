//! As-built architecture overview (Archify HTML).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::util::{run, which};

pub fn spec_path(root: &Path) -> PathBuf {
    root.join("docs").join("architecture.archify.json")
}

pub fn html_path(root: &Path) -> PathBuf {
    root.join("docs").join("architecture.html")
}

/// Print the overview path and, unless `print_only`, open it.
/// `render` rebuilds the HTML from the committed spec (Node + Archify).
pub fn show(root: &Path, print_only: bool, render: bool) -> Result<()> {
    if render {
        render_html(root)?;
    }
    let html = html_path(root);
    if !html.is_file() {
        bail!(
            "missing {} — commit the Archify deliverable or run: cargo make map --render",
            html.display()
        );
    }
    println!("{}", html.display());
    if print_only {
        return Ok(());
    }
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("no DISPLAY/WAYLAND_DISPLAY; open the path above in a browser");
        return Ok(());
    }
    open(&html)
}

fn render_html(root: &Path) -> Result<()> {
    let spec = spec_path(root);
    let html = html_path(root);
    if !spec.is_file() {
        bail!("missing Archify spec {}", spec.display());
    }
    let node = which("node").context("node not on PATH (needed for cargo make map --render)")?;
    let archify = find_archify()?;
    let mut c = Command::new(node);
    c.arg(&archify)
        .args(["deliver", "architecture"])
        .arg(&spec)
        .arg(&html)
        .args(["--quality", "showcase", "--repo-root"])
        .arg(root);
    run(&mut c).with_context(|| {
        format!(
            "archify deliver failed (spec {}). Showcase must pass; previous HTML is kept",
            spec.display()
        )
    })?;
    Ok(())
}

fn find_archify() -> Result<PathBuf> {
    if let Some(raw) = std::env::var_os("OATH_ARCHIFY") {
        let p = PathBuf::from(raw);
        let mjs = if p.is_dir() { p.join("bin").join("archify.mjs") } else { p };
        if mjs.is_file() {
            return Ok(mjs);
        }
        bail!("OATH_ARCHIFY is set but is not archify.mjs: {}", mjs.display());
    }
    if let Some(home) = std::env::var_os("HOME") {
        let grok = PathBuf::from(home).join(".grok/skills/archify/bin/archify.mjs");
        if grok.is_file() {
            return Ok(grok);
        }
    }
    bail!(
        "Archify CLI not found. Set OATH_ARCHIFY to archify.mjs (or the skill directory),\n\
         or keep the Archify skill at ~/.grok/skills/archify"
    )
}

fn open(html: &Path) -> Result<()> {
    let opener = which("xdg-open").context("xdg-open not on PATH")?;
    let mut c = Command::new(opener);
    c.arg(html).stdin(Stdio::null());
    run(&mut c).with_context(|| format!("xdg-open {}", html.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn spec_is_showcase_architecture() {
        let raw = include_str!("../../../docs/architecture.archify.json");
        let v: Value = serde_json::from_str(raw).expect("architecture.archify.json");
        assert_eq!(v["schema_version"], 1);
        assert_eq!(v["diagram_type"], "architecture");
        assert_eq!(v["meta"]["quality_profile"], "showcase");
        assert_eq!(v["meta"]["output"], "architecture.html");
        let n = v["components"].as_array().map(|a| a.len()).unwrap_or(0);
        assert!(n > 0 && n <= 12, "overview should stay at most 12 nodes, got {n}");
    }

    #[test]
    fn html_exists() {
        let html = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/architecture.html");
        assert!(html.is_file(), "missing {}", html.display());
        let n = std::fs::metadata(&html).unwrap().len();
        assert!(n > 10_000, "architecture.html too small ({n} bytes)");
    }
}
