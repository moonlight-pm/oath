//! Inventory: probe `/dev` and sysfs. Do not load modules.

use std::fs;
use std::path::Path;

use crate::error::{Error, Result};
use crate::id::ObjectId;
use crate::kinds::{Dev, DevActual};

#[derive(Clone, Copy)]
enum InputKind {
    Keyboard,
    Pointer,
}

pub fn converge(id: &ObjectId, desired: &Dev) -> Result<DevActual> {
    if !desired.present {
        return Err(Error::hint(format!("{id} is not removable"), "oath schema dev"));
    }
    Ok(probe(&id.name))
}

pub fn probe(name: &str) -> DevActual {
    match name {
        "vda" => fixed("block", "/dev/vda"),
        "net0" => fixed("net", "/sys/class/net/net0"),
        "ttyS0" => fixed("tty", "/dev/ttyS0"),
        "card0" => fixed("drm", "/dev/dri/card0"),
        "kbd0" => input_dev(InputKind::Keyboard),
        "mouse0" => input_dev(InputKind::Pointer),
        _ => DevActual { present: false, class: "unknown".into(), node: String::new() },
    }
}

fn fixed(class: &str, node: &str) -> DevActual {
    DevActual { present: Path::new(node).exists(), class: class.into(), node: node.into() }
}

fn input_dev(kind: InputKind) -> DevActual {
    match find_input_node_in(Path::new("/sys/class/input"), kind) {
        Some(node) => DevActual { present: Path::new(&node).exists(), class: "input".into(), node },
        None => DevActual { present: false, class: "input".into(), node: String::new() },
    }
}

fn find_input_node_in(class_input: &Path, kind: InputKind) -> Option<String> {
    let mut found: Vec<(String, String)> = Vec::new();
    let rd = fs::read_dir(class_input).ok()?;
    for ent in rd.flatten() {
        let fname = ent.file_name();
        let n = fname.to_string_lossy();
        if !n.starts_with("event") {
            continue;
        }
        let nm = fs::read_to_string(ent.path().join("device/name")).unwrap_or_default();
        found.push((n.to_string(), nm));
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    for (event, name) in found {
        let lname = name.to_ascii_lowercase();
        let ok = match kind {
            InputKind::Keyboard => lname.contains("keyboard"),
            InputKind::Pointer => lname.contains("mouse") || lname.contains("pointer"),
        };
        if ok {
            return Some(format!("/dev/input/{event}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qemu_input_names() {
        let d = tempfile::tempdir().unwrap();
        let class = d.path();
        fs::create_dir_all(class.join("event0/device")).unwrap();
        fs::create_dir_all(class.join("event1/device")).unwrap();
        fs::write(class.join("event0/device/name"), "QEMU Virtio Keyboard\n").unwrap();
        fs::write(class.join("event1/device/name"), "QEMU Virtio Mouse\n").unwrap();
        assert_eq!(
            find_input_node_in(class, InputKind::Keyboard).as_deref(),
            Some("/dev/input/event0")
        );
        assert_eq!(
            find_input_node_in(class, InputKind::Pointer).as_deref(),
            Some("/dev/input/event1")
        );
    }
}
