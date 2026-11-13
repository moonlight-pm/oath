//! Writing an origin (R25): the R2 bucket behind
//! `https://store.oath.moonlight.pm`, through R2's S3 API with curl
//! (`--aws-sigv4`). Public read, private write: the write key is on a
//! publisher's machine only, in the environment or
//! `~/.config/oath/origin.env`, never in a tree. `cargo make publish` (a
//! build host's release) and `oath publish` (one package's head, R72) both
//! write through it, and sign with the key beside it ([`sign_key`]).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{find_bin, Error, Result};

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

/// The signing key: `$OATH_SIGN_KEY`, else `sign_ed25519` beside
/// `origin.env`.
pub fn sign_key() -> PathBuf {
    std::env::var_os("OATH_SIGN_KEY")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| config_file().with_file_name("sign_ed25519"))
}

/// This machine's public signing key (`ssh-ed25519 AAAA… name`).
pub fn public_key() -> Result<String> {
    let key = sign_key();
    let pub_file = PathBuf::from(format!("{}.pub", key.display()));
    let line = fs::read_to_string(&pub_file).map_err(|e| {
        Error::hint(
            format!("no signing key at {}: {e}", key.display()),
            format!(
                "ssh-keygen -t ed25519 -N '' -C oath@$(hostname) -f {} (machines trust it once \
                 a person adds it to host:local.publishers)",
                key.display()
            ),
        )
    })?;
    Ok(line.trim().to_string())
}

/// Sign `file` in `namespace` with [`sign_key`]: `<file>.sig` beside it.
pub fn sign(file: &Path, namespace: &str) -> Result<PathBuf> {
    let sig = PathBuf::from(format!("{}.sig", file.display()));
    let _ = fs::remove_file(&sig);
    let out = Command::new(find_bin("ssh-keygen"))
        .args(["-Y", "sign", "-q", "-n", namespace, "-f"])
        .arg(sign_key())
        .arg(file)
        .output()
        .map_err(|e| Error::Msg(format!("ssh-keygen: {e}")))?;
    if !out.status.success() {
        return Err(Error::Msg(format!(
            "ssh-keygen -Y sign: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(sig)
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
                .ok_or_else(|| {
                    Error::hint(
                        format!(
                            "no write key for the default origin: {k} is in neither the \
                             environment nor {}",
                            file.display()
                        ),
                        "docs/manual/packages.md, \"Publishing\"",
                    )
                })
        };
        let s3 = get(S3)?.trim_end_matches('/').to_string();
        if !s3.starts_with("https://") {
            return Err(Error::Msg(format!("{S3} must be an https:// bucket URL")));
        }
        Ok(Self { s3, id: get(KEY_ID)?, secret: get(SECRET)? })
    }

    /// curl with the key on stdin (`-K -`), so it is not in argv.
    fn curl(&self, args: &[&str]) -> Result<String> {
        let mut child = Command::new(find_bin("curl"))
            .args(["-fsS", "--aws-sigv4", "aws:amz:auto:s3", "-K", "-"])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::Msg(format!("spawn curl: {e}")))?;
        let mut stdin = child.stdin.take().ok_or_else(|| Error::Msg("curl stdin".into()))?;
        let key = format!("user = \"{}:{}\"\n", self.id, self.secret);
        crate::write_to_peer(&mut stdin, key.as_bytes())?;
        drop(stdin);
        let out = child.wait_with_output()?;
        if !out.status.success() {
            return Err(Error::Msg(format!(
                "curl {}: {}",
                args.last().unwrap_or(&""),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Every key in the bucket (ListObjectsV2, a page at a time).
    pub fn keys(&self) -> Result<BTreeSet<String>> {
        self.keys_under("")
    }

    /// Every key that starts with `prefix`.
    pub fn keys_under(&self, prefix: &str) -> Result<BTreeSet<String>> {
        let mut keys = BTreeSet::new();
        let mut token: Option<String> = None;
        loop {
            let mut url = format!("{}?list-type=2&max-keys=1000", self.s3);
            if !prefix.is_empty() {
                url.push_str(&format!("&prefix={}", pct(prefix)));
            }
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
        // Only a plain file goes up: curl would follow a link to anything this
        // user can read, and the audit's box can leave one where a pack goes (R77).
        let plain = fs::symlink_metadata(file).is_ok_and(|m| m.file_type().is_file());
        if !plain {
            return Err(Error::Msg(format!("put {key}: {} is not a plain file", file.display())));
        }
        let url = format!("{}/{key}", self.s3);
        let cache = if is_immutable(key) {
            "Cache-Control: public, max-age=31536000, immutable"
        } else if is_head(key) {
            // A head moves whenever its publisher publishes (R72).
            "Cache-Control: public, max-age=60"
        } else {
            "Cache-Control: public, max-age=300"
        };
        let ctype = format!("Content-Type: {}", content_type(key));
        let path =
            file.to_str().ok_or_else(|| Error::Msg(format!("{}: not UTF-8", file.display())))?;
        self.curl(&["-T", path, "-H", &ctype, "-H", cache, &url])
            .map(|_| ())
            .map_err(|e| Error::Msg(format!("put {key}: {e}")))
    }
}

/// Keys named by a hash never change: packs and sources
/// (`pkg/<name>/<hash>.tar`) and plans, and an image named by its output,
/// which is published once (R86). A head (`pkg/<name>.json`, R72), `adv/`
/// and `INDEX.md` do.
pub fn is_immutable(key: &str) -> bool {
    (key.starts_with("pkg/") && !is_head(key))
        || key.starts_with("plan/")
        || key.starts_with("image/")
}

/// `pkg/<name>.json` or its `.sig`: a package's head (R72).
pub fn is_head(key: &str) -> bool {
    key.strip_prefix("pkg/").is_some_and(|rest| {
        !rest.contains('/') && (rest.ends_with(".json") || rest.ends_with(".json.sig"))
    })
}

fn content_type(key: &str) -> &'static str {
    match key.rsplit('.').next() {
        Some("tar") => "application/x-tar",
        Some("xz") => "application/x-xz",
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
    fn put_refuses_a_link() {
        let dir = std::env::temp_dir().join(format!("oath-put-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let secret = dir.join("secret");
        fs::write(&secret, "key").unwrap();
        let link = dir.join("update.tar");
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        let o = Origin { s3: "http://127.0.0.1:1".into(), id: "x".into(), secret: "x".into() };
        let err = o.put("out/x/update.tar", &link).unwrap_err().to_string();
        assert!(err.contains("not a plain file"), "{err}");
        fs::remove_dir_all(&dir).unwrap();
    }

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
        assert!(is_immutable("image/oath-desk-42a8028ae7bf.img.xz"));
        assert_eq!(content_type("image/oath-desk-42a8028ae7bf.img.xz"), "application/x-xz");
        let sum = "image/oath-desk-42a8028ae7bf.img.xz.sha256";
        assert_eq!(content_type(sum), "text/plain; charset=utf-8");
    }

    #[test]
    fn a_head_is_not_immutable() {
        assert!(is_head("pkg/sola.json") && is_head("pkg/sola.json.sig"));
        assert!(!is_immutable("pkg/sola.json") && !is_immutable("pkg/sola.json.sig"));
        assert!(!is_head("pkg/sola/sha256-0.tar") && !is_head("out/desk.json"));
    }
}
