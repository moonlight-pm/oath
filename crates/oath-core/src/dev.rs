//! Inventory: probe `/dev` and sysfs. Do not load modules.

use std::path::Path;

use crate::error::{Error, Result};
use crate::id::ObjectId;
use crate::kinds::{Dev, DevActual};

pub fn converge(id: &ObjectId, desired: &Dev) -> Result<DevActual> {
    if !desired.present {
        return Err(Error::hint(format!("{id} is not removable"), "oath schema dev"));
    }
    Ok(probe(&id.name))
}

pub fn probe(name: &str) -> DevActual {
    let (class, node) = match name {
        "vda" => ("block", "/dev/vda"),
        "net0" => ("net", "/sys/class/net/net0"),
        "ttyS0" => ("tty", "/dev/ttyS0"),
        _ => ("unknown", ""),
    };
    let present = !node.is_empty() && Path::new(node).exists();
    DevActual { present, class: class.into(), node: node.into() }
}
