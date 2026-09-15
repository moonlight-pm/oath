//! Canonical plan file (T43). Lint is a gate; hash is SHA-256 of the file.

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::kinds::PkgNeed;
use crate::packhash::{is_realization_id, HASH_PREFIX};

pub const PLAN_NAME: &str = "plan.plan";
const MAGIC: &str = "#!oath-plan";
const SEP: &str = "---";
const SHEBANG: &str = "#!/bin/sh";
const SETEU: &str = "set -eu";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanFile {
    pub name: String,
    pub produces: String,
    pub build_needs: Vec<PkgNeed>,
    pub run_needs: Vec<PkgNeed>,
    pub script: String,
}

pub fn file_hash(bytes: &[u8]) -> String {
    format!("{HASH_PREFIX}{}", hex(Sha256::digest(bytes).as_ref()))
}

pub fn input_set_hash(needs: &[PkgNeed]) -> String {
    let mut s = String::from("oath-plan-inputs-v1\n");
    for n in needs {
        s.push_str(&n.id);
        s.push('\t');
        s.push_str(&n.hash);
        s.push('\n');
    }
    format!("{HASH_PREFIX}{}", hex(Sha256::digest(s.as_bytes()).as_ref()))
}

pub fn parse_plan(text: &str) -> Result<PlanFile> {
    if text.starts_with('\u{feff}') {
        return Err(Error::hint("plan file has a BOM", "oath fmt <path>"));
    }
    let Some((head, script_part)) = text.split_once(&format!("\n{SEP}\n")) else {
        return Err(Error::hint("plan file missing --- separator", "oath fmt <path>"));
    };
    let Some(json_txt) = head.strip_prefix(MAGIC) else {
        return Err(Error::hint("plan file must start with #!oath-plan", "oath fmt <path>"));
    };
    let json_txt = json_txt.strip_prefix('\n').unwrap_or(json_txt);
    let v: serde_json::Value = serde_json::from_str(json_txt)
        .map_err(|e| Error::hint(format!("plan JSON: {e}"), "oath fmt <path>"))?;
    let obj = v
        .as_object()
        .ok_or_else(|| Error::hint("plan JSON must be an object", "oath fmt <path>"))?;
    for k in obj.keys() {
        if !matches!(k.as_str(), "name" | "produces" | "build_needs" | "run_needs") {
            return Err(Error::hint(
                format!("unknown plan field `{k}`"),
                "oath fmt <path>",
            ));
        }
    }
    for k in ["name", "produces", "build_needs", "run_needs"] {
        if !obj.contains_key(k) {
            return Err(Error::hint(format!("plan JSON missing `{k}`"), "oath fmt <path>"));
        }
    }
    let name = obj["name"]
        .as_str()
        .ok_or_else(|| Error::hint("name must be a string", "oath fmt <path>"))?
        .to_string();
    if !valid_name(&name) {
        return Err(Error::hint(
            format!("bad plan name `{name}`"),
            "name is [a-z0-9-]+ and matches the slot",
        ));
    }
    let produces = obj["produces"]
        .as_str()
        .ok_or_else(|| Error::hint("produces must be a string", "oath fmt <path>"))?
        .to_string();
    if produces != format!("pkg:{name}") {
        return Err(Error::hint(
            format!("produces must be pkg:{name}"),
            "one plan → one product; names match",
        ));
    }
    let build_needs = parse_needs(&obj["build_needs"], "build_needs")?;
    let run_needs = parse_needs(&obj["run_needs"], "run_needs")?;
    let script_part = script_part.strip_suffix('\n').unwrap_or(script_part);
    let mut lines = script_part.lines();
    if lines.next() != Some(SHEBANG) {
        return Err(Error::hint("script must start with #!/bin/sh", "oath fmt <path>"));
    }
    if lines.next() != Some(SETEU) {
        return Err(Error::hint("script second line must be set -eu", "oath fmt <path>"));
    }
    let rest: Vec<&str> = lines.collect();
    let script = if rest.is_empty() { String::new() } else { rest.join("\n") };
    Ok(PlanFile { name, produces, build_needs, run_needs, script })
}

pub fn fmt_plan(p: &PlanFile) -> String {
    let mut build_needs = p.build_needs.clone();
    let mut run_needs = p.run_needs.clone();
    build_needs.sort_by(|a, b| a.id.cmp(&b.id));
    run_needs.sort_by(|a, b| a.id.cmp(&b.id));
    let p = PlanFile {
        build_needs,
        run_needs,
        name: p.name.clone(),
        produces: p.produces.clone(),
        script: p.script.clone(),
    };
    let mut s = String::from(MAGIC);
    s.push('\n');
    s.push_str("{\n");
    s.push_str(&format!("  \"name\": {},\n", json_str(&p.name)));
    s.push_str(&format!("  \"produces\": {},\n", json_str(&p.produces)));
    s.push_str("  \"build_needs\": ");
    append_needs(&mut s, &p.build_needs);
    s.push_str(",\n");
    s.push_str("  \"run_needs\": ");
    append_needs(&mut s, &p.run_needs);
    s.push('\n');
    s.push('}');
    s.push('\n');
    s.push_str(SEP);
    s.push('\n');
    s.push_str(SHEBANG);
    s.push('\n');
    s.push_str(SETEU);
    s.push('\n');
    if !p.script.is_empty() {
        s.push_str(p.script.trim_end_matches('\n'));
        s.push('\n');
    }
    s
}

pub fn lint_bytes(bytes: &[u8]) -> Result<PlanFile> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::hint("plan file is not UTF-8", "oath fmt <path>"))?;
    if !text.ends_with('\n') {
        return Err(Error::hint("plan file must end with a newline", "oath fmt <path>"));
    }
    let parsed = parse_plan(text)?;
    let canon = fmt_plan(&parsed);
    if canon.as_bytes() != bytes {
        return Err(Error::hint(
            "plan file is not canonical",
            "oath fmt -w <path>",
        ));
    }
    Ok(parsed)
}

pub fn lint_path(path: &std::path::Path) -> Result<(PlanFile, Vec<u8>)> {
    let bytes = std::fs::read(path)?;
    let plan = lint_bytes(&bytes)?;
    Ok((plan, bytes))
}

fn parse_needs(v: &serde_json::Value, field: &str) -> Result<Vec<PkgNeed>> {
    let arr = v
        .as_array()
        .ok_or_else(|| Error::hint(format!("{field} must be an array"), "oath fmt <path>"))?;
    let mut out = Vec::new();
    let mut prev = "";
    for item in arr {
        let o = item
            .as_object()
            .ok_or_else(|| Error::hint(format!("{field} entries must be objects"), "oath fmt <path>"))?;
        if o.keys().any(|k| k != "id" && k != "hash") || !o.contains_key("id") || !o.contains_key("hash")
        {
            return Err(Error::hint(
                format!("{field} entries must have id and hash only"),
                "oath fmt <path>",
            ));
        }
        let id = o["id"]
            .as_str()
            .ok_or_else(|| Error::hint("need id must be a string", "oath fmt <path>"))?;
        if !id.starts_with("pkg:") || id.len() < 5 {
            return Err(Error::hint(
                format!("need id must be pkg:<name>, got `{id}`"),
                "oath fmt <path>",
            ));
        }
        let hash = o["hash"]
            .as_str()
            .ok_or_else(|| Error::hint("need hash must be a string", "oath fmt <path>"))?;
        if !is_realization_id(hash) {
            return Err(Error::hint(
                format!("need hash must be sha256- + 64 hex, got `{hash}`"),
                "oath fmt <path>",
            ));
        }
        if !prev.is_empty() && id <= prev {
            return Err(Error::hint(
                format!("{field} must be sorted by id"),
                "oath fmt <path>",
            ));
        }
        prev = id;
        out.push(PkgNeed { id: id.to_string(), hash: hash.to_string() });
    }
    Ok(out)
}

fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && n.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'))
        && !n.starts_with('-')
        && !n.ends_with('-')
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

fn append_needs(out: &mut String, needs: &[PkgNeed]) {
    if needs.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push_str("[\n");
    for (i, n) in needs.iter().enumerate() {
        out.push_str("    {\n");
        out.push_str(&format!("      \"id\": {},\n", json_str(&n.id)));
        out.push_str(&format!("      \"hash\": {}\n", json_str(&n.hash)));
        out.push_str("    }");
        if i + 1 != needs.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]");
}

fn hex(bytes: &[u8]) -> String {
    const T: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(T[(b >> 4) as usize] as char);
        s.push(T[(b & 0xf) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PlanFile {
        PlanFile {
            name: "hello".into(),
            produces: "pkg:hello".into(),
            build_needs: vec![
                PkgNeed {
                    id: "pkg:busybox".into(),
                    hash: "sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                        .into(),
                },
                PkgNeed {
                    id: "pkg:cc".into(),
                    hash: "sha256-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                        .into(),
                },
            ],
            run_needs: vec![],
            script: "cp /oath/store/pkg/hello-src/x/bin/hello /out/bin/hello".into(),
        }
    }

    #[test]
    fn roundtrip_canonical() {
        let p = sample();
        let s = fmt_plan(&p);
        assert!(s.starts_with("#!oath-plan\n{"));
        assert!(s.ends_with('\n'));
        let parsed = parse_plan(&s).unwrap();
        assert_eq!(parsed, p);
        lint_bytes(s.as_bytes()).unwrap();
        assert_eq!(file_hash(s.as_bytes()).len(), 7 + 64);
    }

    #[test]
    fn lint_rejects_unsorted() {
        let s = "#!oath-plan\n{\n  \"name\": \"hello\",\n  \"produces\": \"pkg:hello\",\n  \"build_needs\": [\n    {\n      \"id\": \"pkg:cc\",\n      \"hash\": \"sha256-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\n    },\n    {\n      \"id\": \"pkg:busybox\",\n      \"hash\": \"sha256-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n    }\n  ],\n  \"run_needs\": []\n}\n---\n#!/bin/sh\nset -eu\n";
        let err = parse_plan(s).unwrap_err().to_string();
        assert!(err.contains("sorted"), "{err}");
    }

    #[test]
    fn lint_rejects_extra_whitespace() {
        let s = fmt_plan(&sample());
        let dirty = s.replace("  \"name\"", "   \"name\"");
        let err = lint_bytes(dirty.as_bytes()).unwrap_err().to_string();
        assert!(err.contains("canonical"), "{err}");
    }

    #[test]
    fn produces_must_match_name() {
        let mut p = sample();
        p.produces = "pkg:other".into();
        let s = fmt_plan(&p);
        let err = parse_plan(&s).unwrap_err().to_string();
        assert!(err.contains("produces"), "{err}");
    }
}
