//! `oath consort` (R87): this machine's consort, from its `oath-bus`. Bare,
//! it says who is in it, which machines nearby are not, and who asks to
//! join; `join`, `admit`, `remove` and `leave` change who is in it (root).

use std::path::Path;

use oath_core::consort::call;
use oath_core::Result;
use serde_json::{json, Value};

pub enum Action<'a> {
    Show,
    Join(&'a str),
    Admit(&'a str, &'a str),
    Remove(&'a str),
    Leave,
}

pub fn run(root: &Path, action: Action, as_json: bool) -> Result<i32> {
    let req = match action {
        Action::Show => json!({ "op": "state" }),
        Action::Join(to) => json!({ "op": "join", "to": to }),
        Action::Admit(name, code) => json!({ "op": "admit", "name": name, "code": code }),
        Action::Remove(name) => json!({ "op": "remove", "name": name }),
        Action::Leave => json!({ "op": "leave" }),
    };
    let op = req["op"].as_str().unwrap_or("").to_string();
    let mut v = call(root, &req)?;
    if as_json {
        if let Some(o) = v.as_object_mut() {
            o.remove("ok");
        }
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(0);
    }
    match op.as_str() {
        "state" => print!("{}", show(&v)),
        "join" if v["joined"] == true => println!("this machine is a member of consort {} already", s(&v["consort"])),
        "join" => println!(
            "asked {} to admit {} into consort {}.\ncode: {}\nOn any member: sudo oath consort admit {} --code {}",
            s(&v["via"]),
            s(&v["name"]),
            s(&v["consort"]),
            s(&v["code"]),
            s(&v["name"]),
            s(&v["code"]),
        ),
        "admit" => println!("admitted {}; every member will hear it", s(&v["admitted"])),
        "remove" => println!("removed {}; every member will hear it", s(&v["removed"])),
        _ => println!("left the consort; this machine is a consort of one"),
    }
    Ok(0)
}

fn s(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

/// The state as a person reads it.
fn show(v: &Value) -> String {
    let c = &v["consort"];
    let members = c["members"].as_array().cloned().unwrap_or_default();
    let mut out = format!(
        "consort {}: {} member{}\n",
        s(&c["short"]),
        members.len(),
        if members.len() == 1 { "" } else { "s" }
    );
    let w = members.iter().map(|m| s(&m["name"]).len()).max().unwrap_or(4).max(4);
    for m in &members {
        let st = &m["status"];
        let place = if m["me"] == true {
            "this machine".to_string()
        } else if m["online"] == true {
            s(&m["address"])
        } else if m["seen"].is_string() {
            format!("offline since {}", s(&m["seen"]))
        } else {
            "not heard yet".into()
        };
        let mut facts = Vec::new();
        if st.is_object() {
            facts.push(format!("{} {}", s(&st["profile"]), s(&st["output"])));
            facts.push(format!("generation {}", s(&st["generation"])));
            match st["drift"].as_u64() {
                Some(0) => facts.push("in sync".into()),
                Some(n) => facts.push(format!("{n} drifted")),
                None => {}
            }
            let failed: Vec<String> = st["failed"].as_array().into_iter().flatten().map(s).collect();
            if !failed.is_empty() {
                facts.push(format!("failed: {}", failed.join(", ")));
            }
        }
        out += &format!("  {:w$}  {:24}  {}\n", s(&m["name"]), place, facts.join(", "));
    }
    if let Some(j) = v["joining"].as_object() {
        out += &format!(
            "this machine asks {} to admit it into consort {}: code {}{}\n",
            s(&j["to"]),
            s(&j["consort"]),
            s(&j["code"]),
            if j["pending"] == true { ", waiting for the owner" } else { "" }
        );
    }
    let nearby = v["nearby"].as_array().cloned().unwrap_or_default();
    if !nearby.is_empty() {
        out += "nearby, not members:\n";
        for n in nearby {
            let k = n["members"].as_u64().unwrap_or(1);
            out += &format!(
                "  {:w$}  {:24}  consort {} ({k} member{})\n",
                s(&n["name"]),
                s(&n["address"]),
                s(&n["consort"]),
                if k == 1 { "" } else { "s" }
            );
        }
    }
    let asks = v["asks"].as_array().cloned().unwrap_or_default();
    if !asks.is_empty() {
        out += "asking to join:\n";
        for a in asks {
            let name = s(&a["name"]);
            out += &format!("  {name:w$}  {:24}  asked {}\n", s(&a["address"]), s(&a["via"]));
            out += &format!(
                "  {:w$}  sudo oath consort admit {name} --code <the code {name} printed>\n",
                ""
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_names_members_nearby_and_asks_but_not_the_code() {
        let v = json!({
            "consort": { "short": "3f2a9c1b04d2", "members": [
                { "name": "aulos", "me": true, "online": true,
                  "status": { "profile": "base", "output": "c1d413829997", "generation": 1, "drift": 0, "failed": [] } },
                { "name": "buccina", "online": true, "address": "10.0.0.149",
                  "status": { "profile": "desk", "output": "42a8028ae7bf", "generation": 189, "drift": 2, "failed": ["audit"] } },
            ]},
            "nearby": [{ "name": "dulcia", "address": "10.0.0.146", "consort": "9a1b2c3d4e5f", "members": 1 }],
            "asks": [{ "name": "koeurax", "address": "10.0.0.156", "via": "aulos", "code": "123456" }],
            "joining": null,
        });
        let text = show(&v);
        assert!(text.starts_with("consort 3f2a9c1b04d2: 2 members\n"));
        assert!(text.contains("aulos    this machine"));
        assert!(text.contains("base c1d413829997, generation 1, in sync"));
        assert!(text.contains("2 drifted, failed: audit"));
        assert!(text.contains("dulcia"));
        assert!(text.contains("sudo oath consort admit koeurax --code <the code koeurax printed>"));
        // The owner types the code the joining machine shows: this side never prints it.
        assert!(!text.contains("123456"));
    }
}
