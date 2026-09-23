//! Catalog round-trip for the svc account fields. The process drop is
//! covered by `svc_run::live_child_is_uid_1_sees_env_and_low_port_cap`.

use oath_core::{seed, svc_spawn, Catalog, ObjectId, Svc, SvcUser};
use serde_json::{json, Map, Value};

#[test]
fn catalog_accepts_fields_and_old_documents() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path()).unwrap();
    let cat = Catalog::open(dir.path()).unwrap();
    let id: ObjectId = "svc:hold".parse().unwrap();
    let spec: Svc = serde_json::from_value(cat.get(&id).unwrap().desired).unwrap();
    assert!(spec.user.is_none());
    assert!(spec.env.is_empty());
    assert!(!spec.bind_low_ports);
    assert_eq!(svc_spawn("svc:hold", &spec).cred.user, SvcUser::Root);

    let mut fields = Map::new();
    fields.insert("user".into(), json!("home"));
    fields.insert("bind_low_ports".into(), json!(true));
    fields.insert("env".into(), json!({"WICKET_STATE_DIR": "/var/lib/wicket"}));
    cat.set_fields(&id, fields).unwrap();
    let desired = cat.get(&id).unwrap().desired;
    let spec: Svc = serde_json::from_value(desired).unwrap();
    assert_eq!(spec.user, Some(SvcUser::Home));
    assert!(spec.bind_low_ports);
    assert_eq!(spec.env.get("WICKET_STATE_DIR").map(String::as_str), Some("/var/lib/wicket"));
    assert_eq!(svc_spawn("svc:hold", &spec).cred.uid, 1);
    assert!(svc_spawn("svc:hold", &spec).cred.bind_low_ports);

    let mut unknown = Map::new();
    unknown.insert("postgres".into(), json!(true));
    assert!(cat.set_fields(&id, unknown).is_err());

    let schema: Value = serde_json::from_str(&cat.schema_json("svc").unwrap()).unwrap();
    let props = schema["properties"].as_object().unwrap();
    assert!(props.contains_key("user"));
    assert!(props.contains_key("env"));
    assert!(props.contains_key("bind_low_ports"));
    let required = schema["required"].as_array().unwrap();
    assert!(!required.iter().any(|v| v == "user"));
    assert!(!required.iter().any(|v| v == "env"));
    assert!(!required.iter().any(|v| v == "bind_low_ports"));
}
