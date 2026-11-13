//! Two `oath-bus` daemons on this machine, as whoever runs the tests, each
//! in a root of its own: one joins the other's consort by address, the
//! owner admits it with the code it printed, and each sees the other's
//! facts over the link.

use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use oath_core::consort::call;
use serde_json::{json, Value};

struct Bus(Child);

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn start(root: &Path, name: &str, port: u16, beacon: u16) -> Bus {
    let child = Command::new(env!("CARGO_BIN_EXE_oath-bus"))
        .args(["--root", root.to_str().unwrap(), "--name", name])
        .args(["--port", &port.to_string(), "--beacon-port", &beacon.to_string()])
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let b = Bus(child);
    wait(|| call(root, &json!({ "op": "state" })).is_ok());
    b
}

fn wait(mut ok: impl FnMut() -> bool) {
    let t = Instant::now();
    while !ok() {
        assert!(t.elapsed() < Duration::from_secs(20), "timed out");
        thread::sleep(Duration::from_millis(200));
    }
}

fn state(root: &Path) -> Value {
    call(root, &json!({ "op": "state" })).unwrap()
}

fn members(root: &Path) -> Vec<(String, bool)> {
    let mut m: Vec<(String, bool)> = state(root)["consort"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| (m["name"].as_str().unwrap().to_string(), m["online"] == true))
        .collect();
    m.sort();
    m
}

#[test]
fn a_machine_joins_by_address_and_the_two_share_facts() {
    let d = tempfile::tempdir().unwrap();
    let (a, b) = (d.path().join("a"), d.path().join("b"));
    let beacon = free_port();
    let (pa, pb) = (free_port(), free_port());
    let _ba = start(&a, "ta", pa, beacon);
    let _bb = start(&b, "tb", pb, beacon);
    assert_ne!(state(&a)["consort"]["id"], state(&b)["consort"]["id"]);

    let asked = call(&b, &json!({ "op": "join", "to": format!("127.0.0.1:{pa}") })).unwrap();
    let code = asked["code"].as_str().unwrap().to_string();
    assert_eq!(code.len(), 6);
    assert_eq!(asked["consort"], state(&a)["consort"]["short"]);
    wait(|| state(&a)["asks"].as_array().is_some_and(|x| x.len() == 1));

    let wrong = if code == "000000" { "000001" } else { "000000" };
    assert!(call(&a, &json!({ "op": "admit", "name": "tb", "code": wrong })).is_err());
    call(&a, &json!({ "op": "admit", "name": "tb", "code": code })).unwrap();

    let both = vec![("ta".to_string(), true), ("tb".to_string(), true)];
    wait(|| members(&a) == both && members(&b) == both);
    assert_eq!(state(&a)["consort"]["id"], state(&b)["consort"]["id"]);
    assert!(state(&a)["asks"].as_array().unwrap().is_empty());

    // b's facts reach a.
    wait(|| {
        call(&a, &json!({ "op": "records", "peer": "tb", "topic": "load" }))
            .is_ok_and(|v| v["records"].as_array().is_some_and(|r| !r.is_empty()))
    });

    // a removes b: b is a consort of one again, and a has only itself.
    call(&a, &json!({ "op": "remove", "name": "tb" })).unwrap();
    wait(|| members(&a).len() == 1 && members(&b).len() == 1);
    assert_ne!(state(&a)["consort"]["id"], state(&b)["consort"]["id"]);
}
