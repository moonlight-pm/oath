//! The default origin (R25): the R2 bucket behind
//! `https://store.oath.moonlight.pm`, which `cargo make publish` writes
//! through R2's S3 API with curl (`--aws-sigv4`). Public read, private
//! write: the write key is on the build host only, in the environment or
//! `~/.config/oath/origin.env`, never in this tree.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

const S3: &str = "OATH_ORIGIN_S3";
const KEY_ID: &str = "OATH_ORIGIN_KEY_ID";
const SECRET: &str = "OATH_ORIGIN_SECRET";

/// Write access to the origin's bucket: its S3 URL
/// (`https://<account>.r2.cloudflarestorage.com/<bucket>`) and an R2 key.
pub struct Origin {
    s3: String,
    id: String,
    secret: String,
}

/// `~/.config/oath/origin.env` (`$XDG_CONFIG_HOME/oath/origin.env`).
pub fn config_file() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_default()
        .join("oath/origin.env")
}

impl Origin {
    /// The key from the environment, else `config_file()` (`NAME=value`
    /// lines, `#` comments).
    pub fn from_env() -> Result<Self> {
        let file = config_file();
        let mut vars = BTreeMap::new();
        if let Ok(text) = fs::read_to_string(&file) {
            for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'))
            {
                if let Some((k, v)) = line.split_once('=') {
                    vars.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
        }
        let get = |k: &str| {
            std::env::var(k)
                .ok()
                .filter(|v| !v.is_empty())
                .or_else(|| vars.get(k).cloned())
                .with_context(|| {
                    format!(
                        "no write key for the default origin: {k} is in neither the environment \
                         nor {} (docs/manual/packages.md, \"Publishing\")",
                        file.display()
                    )
                })
        };
        let s3 = get(S3)?.trim_end_matches('/').to_string();
        if !s3.starts_with("https://") {
            bail!("{S3} must be an https:// bucket URL");
        }
        Ok(Self { s3, id: get(KEY_ID)?, secret: get(SECRET)? })
    }

    /// curl with the key on stdin (`-K -`), so it is not in argv.
    fn curl(&self, args: &[&str]) -> Result<String> {
        let mut child = Command::new("curl")
            .args(["-fsS", "--aws-sigv4", "aws:amz:auto:s3", "-K", "-"])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn curl")?;
        child
            .stdin
            .take()
            .context("curl stdin")?
            .write_all(format!("user = \"{}:{}\"\n", self.id, self.secret).as_bytes())?;
        let out = child.wait_with_output()?;
        if !out.status.success() {
            bail!(
                "curl {}: {}",
                args.last().unwrap_or(&""),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Every key in the bucket (ListObjectsV2, a page at a time).
    pub fn keys(&self) -> Result<BTreeSet<String>> {
        let mut keys = BTreeSet::new();
        let mut token: Option<String> = None;
        loop {
            let mut url = format!("{}?list-type=2&max-keys=1000", self.s3);
            if let Some(t) = &token {
                url.push_str(&format!("&continuation-token={}", pct(t)));
            }
            let body = self.curl(&[&url])?;
            keys.extend(tags(&body, "Key").into_iter().map(|k| unescape(&k)));
            token = match tags(&body, "IsTruncated").first().map(String::as_str) {
                Some("true") => tags(&body, "NextContinuationToken").into_iter().next(),
                _ => None,
            };
            if token.is_none() {
                return Ok(keys);
            }
        }
    }

    /// Upload `file` as `key`.
    pub fn put(&self, key: &str, file: &Path) -> Result<()> {
        let url = format!("{}/{key}", self.s3);
        let cache = if is_immutable(key) {
            "Cache-Control: public, max-age=31536000, immutable"
        } else {
            "Cache-Control: public, max-age=300"
        };
        let ctype = format!("Content-Type: {}", content_type(key));
        self.curl(&["-T", file.to_str().context("path")?, "-H", &ctype, "-H", cache, &url])
            .map(|_| ())
            .with_context(|| format!("put {key}"))
    }
}

/// Keys named by a hash never change: packs, sources and plans. `adv/`
/// and `INDEX.md` do.
pub fn is_immutable(key: &str) -> bool {
    key.starts_with("pkg/") || key.starts_with("plan/")
}

fn content_type(key: &str) -> &'static str {
    match key.rsplit('.').next() {
        Some("tar") => "application/x-tar",
        Some("json") => "application/json",
        Some("md") => "text/markdown; charset=utf-8",
        _ => "text/plain; charset=utf-8",
    }
}

fn tags(xml: &str, tag: &str) -> Vec<String> {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i + open.len()..];
        let Some(j) = after.find(&close) else { break };
        out.push(after[..j].to_string());
        rest = &after[j + close.len()..];
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_page_keys_and_token() {
        let xml =
            "<ListBucketResult><IsTruncated>true</IsTruncated><Contents><Key>pkg/bash/x.tar</Key>\
                   </Contents><Contents><Key>INDEX.md</Key></Contents>\
                   <NextContinuationToken>a/b+c=</NextContinuationToken></ListBucketResult>";
        assert_eq!(tags(xml, "Key"), ["pkg/bash/x.tar", "INDEX.md"]);
        assert_eq!(tags(xml, "IsTruncated"), ["true"]);
        assert_eq!(pct("a/b+c="), "a%2Fb%2Bc%3D");
        assert_eq!(unescape("a&amp;b"), "a&b");
    }

    #[test]
    fn hashed_keys_are_immutable() {
        assert!(is_immutable("pkg/bash/sha256-0.tar") && is_immutable("plan/bash/sha256-0.plan"));
        assert!(!is_immutable("INDEX.md") && !is_immutable("adv/curl/sha256-0.json"));
        assert_eq!(content_type("INDEX.md"), "text/markdown; charset=utf-8");
        assert_eq!(content_type("plan/bash/sha256-0.plan"), "text/plain; charset=utf-8");
    }
}
