//! Each kind's JSON Schema, generated from its desired type (`kinds.rs`),
//! so the schema an agent reads, the keys `set` accepts and the parse
//! `apply` runs cannot disagree. The prose (`<kind>.md`) is hand-written.

use schemars::generate::SchemaSettings;
use schemars::JsonSchema;
use serde_json::{json, Value};

use crate::kinds::{Dev, Host, Net, Pkg, Plan, Snap, Ssh, Svc};

/// Every kind, with its prose.
pub const KINDS: &[(&str, &str)] = &[
    ("host", include_str!("../schema/host.md")),
    ("svc", include_str!("../schema/svc.md")),
    ("snap", include_str!("../schema/snap.md")),
    ("pkg", include_str!("../schema/pkg.md")),
    ("net", include_str!("../schema/net.md")),
    ("ssh", include_str!("../schema/ssh.md")),
    ("dev", include_str!("../schema/dev.md")),
    ("plan", include_str!("../schema/plan.md")),
];

/// The schema of `kind`'s desired document, or `None` for no such kind.
pub fn schema(kind: &str) -> Option<Value> {
    let mut v = match kind {
        "host" => of::<Host>(),
        "svc" => of::<Svc>(),
        "snap" => of::<Snap>(),
        "pkg" => of::<Pkg>(),
        "net" => of::<Net>(),
        "ssh" => of::<Ssh>(),
        "dev" => of::<Dev>(),
        "plan" => of::<Plan>(),
        _ => return None,
    };
    let o = v.as_object_mut()?;
    o.insert("$id".into(), json!(format!("oath:kind:{kind}")));
    o.insert("title".into(), json!(kind));
    o.insert("additionalProperties".into(), json!(false));
    Some(v)
}

/// [`schema`] as the file `schema/<kind>.json` holds it.
pub fn text(kind: &str) -> Option<String> {
    let mut s = serde_json::to_string_pretty(&schema(kind)?).ok()?;
    s.push('\n');
    Some(s)
}

/// `kind`'s prose (`schema/<kind>.md`).
pub fn prose(kind: &str) -> Option<&'static str> {
    KINDS.iter().find(|(k, _)| *k == kind).map(|(_, md)| *md)
}

fn of<T: JsonSchema>() -> Value {
    let mut settings = SchemaSettings::draft2020_12();
    // One flat document per kind: an agent reads enums in place, not $refs.
    settings.inline_subschemas = true;
    let mut v = settings.into_generator().into_root_schema_for::<T>().to_value();
    tidy(&mut v);
    v
}

/// Doc comments arrive wrapped at 80 columns: a description becomes one
/// line per paragraph. An optional field is left out, never null, so
/// `null` leaves its type and enum.
fn tidy(v: &mut Value) {
    match v {
        Value::Object(o) => {
            if let Some(Value::Array(types)) = o.get_mut("type") {
                types.retain(|t| t != "null");
                if types.len() == 1 {
                    let t = types.remove(0);
                    o.insert("type".into(), t);
                }
            }
            if let Some(Value::Array(e)) = o.get_mut("enum") {
                e.retain(|x| !x.is_null());
            }
            for (k, x) in o.iter_mut() {
                if k == "description" {
                    if let Value::String(s) = x {
                        let paras: Vec<String> = s
                            .split("\n\n")
                            .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
                            .collect();
                        *s = paras.join("\n\n");
                    }
                } else {
                    tidy(x);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(tidy),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_closed_schema_and_prose() {
        for (kind, md) in KINDS {
            let v = schema(kind).unwrap();
            assert_eq!(v["title"], *kind);
            assert_eq!(v["additionalProperties"], false);
            assert!(v["properties"].as_object().is_some_and(|p| !p.is_empty()), "{kind}");
            assert!(!md.trim().is_empty(), "{kind}.md");
        }
        assert!(schema("nope").is_none());
    }

    #[test]
    fn required_and_defaults_follow_serde() {
        let svc = schema("svc").unwrap();
        assert_eq!(svc["required"], json!(["exec"]));
        assert_eq!(svc["properties"]["restart"]["default"], "always");
        assert_eq!(svc["properties"]["enabled"]["default"], true);
        assert_eq!(
            svc["properties"]["restart"]["enum"],
            json!(["never", "always", "on-failure"])
        );
        let net = schema("net").unwrap();
        assert!(net["properties"].get("lease").is_none(), "actual-only fields stay out");
        assert_eq!(schema("host").unwrap()["properties"]["hostname"]["maxLength"], 63);
    }

    #[test]
    #[ignore]
    fn print() {
        for (k, _) in KINDS {
            println!("{}", text(k).unwrap());
        }
    }
}
