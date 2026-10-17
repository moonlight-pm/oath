//! Inventory: what sysfs says the machine has, as `dev:*` objects. PID 1
//! discovers at boot and again when a disk or NIC comes or goes; nothing
//! here loads modules.
//!
//! Names follow stable rules, so the same hardware keeps its name across
//! boots: a disk is `disk-` plus its bus path (`disk-pci-0000.00.1f.2-ata-1`),
//! the NIC the network uses is `net0` and any other physical NIC
//! `net1`, `net2`, … in kernel-name order, a serial port keeps its kernel
//! name (`ttyS0`, `ttyUSB0`) when a UART is really there.

use std::fs;
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

/// `name` as discovery finds it now; absent when it is not there.
pub fn probe(name: &str) -> DevActual {
    discover(Path::new("/sys"))
        .into_iter()
        .find(|(n, _)| n == name)
        .map(|(_, a)| a)
        .unwrap_or_default()
}

/// Every device worth an object, by name, sorted.
pub fn discover(sys: &Path) -> Vec<(String, DevActual)> {
    let mut out = Vec::new();
    out.extend(nics(sys));
    out.extend(disks(sys));
    out.extend(ttys(sys));
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn nics(sys: &Path) -> Vec<(String, DevActual)> {
    let class = sys.join("class/net");
    let mut names: Vec<String> = entries(&class)
        .into_iter()
        .filter(|n| n != "lo" && class.join(n).join("device").exists())
        .collect();
    names.sort();
    // net0 is the NIC PID 1 renamed for net:net0; the others number after it.
    let mut named: Vec<(String, String)> = Vec::new();
    if names.iter().any(|n| n == "net0") {
        named.push(("net0".into(), "net0".into()));
    }
    for (i, n) in names.iter().filter(|n| *n != "net0").enumerate() {
        named.push((format!("net{}", i + 1), n.clone()));
    }
    named
        .into_iter()
        .map(|(name, ifname)| {
            let dir = class.join(&ifname);
            (
                name,
                DevActual {
                    present: true,
                    class: "net".into(),
                    node: format!("/sys/class/net/{ifname}"),
                    path: bus_path(&dir.join("device")),
                    driver: driver(&dir.join("device")),
                    mac: read(&dir.join("address")),
                    ..Default::default()
                },
            )
        })
        .collect()
}

fn disks(sys: &Path) -> Vec<(String, DevActual)> {
    let block = sys.join("block");
    let mut out = Vec::new();
    for dev in entries(&block) {
        let skip = ["loop", "ram", "zram", "dm-", "md", "nbd"];
        let dir = block.join(&dev);
        if skip.iter().any(|p| dev.starts_with(p)) || !dir.join("device").exists() {
            continue;
        }
        let mut path = bus_path(&dir.join("device"));
        if dev.starts_with("nvme") {
            let ns = read(&dir.join("nsid"));
            let ns =
                if ns.is_empty() { dev.rsplit('n').next().unwrap_or("1").to_string() } else { ns };
            path = format!("{path}-nvme-{ns}");
        }
        if path.is_empty() {
            path = dev.clone();
        }
        // SCSI and ATA name the vendor and model; virtio has only a PCI
        // vendor id, which says nothing.
        let mut model = read(&dir.join("device/model"));
        let vendor = read(&dir.join("device/vendor"));
        if !model.is_empty() && !vendor.is_empty() && !vendor.starts_with("0x") {
            model = format!("{vendor} {model}");
        }
        out.push((
            format!("disk-{}", id_safe(&path)),
            DevActual {
                present: true,
                class: "block".into(),
                node: format!("/dev/{dev}"),
                path,
                driver: driver(&dir.join("device")),
                model,
                size: read(&dir.join("size")).parse::<u64>().unwrap_or(0) * 512,
                removable: read(&dir.join("removable")) == "1",
                ..Default::default()
            },
        ));
    }
    out
}

fn ttys(sys: &Path) -> Vec<(String, DevActual)> {
    let class = sys.join("class/tty");
    let mut out = Vec::new();
    for t in entries(&class) {
        let dir = class.join(&t);
        let usb = t.starts_with("ttyUSB") || t.starts_with("ttyACM");
        // An 8250 port with no UART behind it reports type 0.
        let uart = t.starts_with("ttyS") && !matches!(read(&dir.join("type")).as_str(), "" | "0");
        if !(usb || uart) || !dir.join("device").exists() {
            continue;
        }
        out.push((
            t.clone(),
            DevActual {
                present: true,
                class: "tty".into(),
                node: format!("/dev/{t}"),
                path: bus_path(&dir.join("device")),
                driver: driver(&dir.join("device")),
                ..Default::default()
            },
        ));
    }
    out
}

/// A by-path style name for the device `link` points at: the PCI address
/// of its controller, then the port on it (`pci-0000:00:1f.2-ata-1`,
/// `pci-0000:00:14.0-usb-1-2`). Off PCI, the device's own sysfs name.
fn bus_path(link: &Path) -> String {
    let Ok(real) = fs::canonicalize(link) else { return String::new() };
    let parts: Vec<String> =
        real.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let Some(pci) = parts.iter().rposition(|p| is_pci_addr(p)) else {
        return parts.last().cloned().unwrap_or_default();
    };
    let mut path = format!("pci-{}", parts[pci]);
    let below = &parts[pci + 1..];
    if let Some(ata) = below.iter().find_map(|p| p.strip_prefix("ata").filter(|n| is_num(n))) {
        path.push_str(&format!("-ata-{ata}"));
    } else if let Some(port) = below.iter().rev().find(|p| is_usb_port(p)) {
        path.push_str(&format!("-usb-{port}"));
    }
    path
}

fn is_pci_addr(s: &str) -> bool {
    // 0000:00:1f.2
    let b = s.as_bytes();
    b.len() == 12
        && b[4] == b':'
        && b[7] == b':'
        && b[10] == b'.'
        && s.chars().filter(|c| c.is_ascii_hexdigit()).count() == 9
}

fn is_usb_port(s: &str) -> bool {
    // 1-2 or 1-2.4 (a hub port), not 1-2:1.0 (an interface)
    s.split_once('-')
        .is_some_and(|(bus, port)| is_num(bus) && !port.is_empty() && port.split('.').all(is_num))
}

fn is_num(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// What an object name may hold: `:` and anything else becomes `.` or `_`.
fn id_safe(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| match c {
            ':' => '.',
            c if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' => c,
            _ => '_',
        })
        .collect();
    out.truncate(59);
    out
}

fn driver(device: &Path) -> String {
    fs::read_link(device.join("driver"))
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn entries(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    /// A sysfs with a virtio NIC renamed net0, a second NIC, a virtio disk,
    /// a SATA disk, a USB stick, a loop device, a real UART and an empty
    /// 8250 port.
    fn fake_sys() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let sys = d.path();
        let dev = |p: &str| {
            let p = sys.join("devices").join(p);
            fs::create_dir_all(&p).unwrap();
            p
        };
        let link = |class: &str, name: &str, target: &Path| {
            let dir = sys.join(class).join(name);
            fs::create_dir_all(&dir).unwrap();
            symlink(target, dir.join("device")).unwrap();
            dir
        };
        let n0 = link("class/net", "net0", &dev("pci0000:00/0000:00:03.0/virtio0"));
        fs::write(n0.join("address"), "52:54:00:12:34:56\n").unwrap();
        link("class/net", "enp2s0", &dev("pci0000:00/0000:00:1c.0/0000:02:00.0"));
        fs::create_dir_all(sys.join("class/net/lo")).unwrap();
        let vda = link("block", "vda", &dev("pci0000:00/0000:00:04.0/virtio1"));
        fs::write(vda.join("size"), "16777216\n").unwrap();
        let sda =
            link("block", "sda", &dev("pci0000:00/0000:00:1f.2/ata1/host0/target0:0:0/0:0:0:0"));
        fs::write(sda.join("size"), "1000\n").unwrap();
        fs::write(
            sys.join("devices/pci0000:00/0000:00:1f.2/ata1/host0/target0:0:0/0:0:0:0/model"),
            "Samsung SSD\n",
        )
        .unwrap();
        let sdb = link(
            "block",
            "sdb",
            &dev("pci0000:00/0000:00:14.0/usb1/1-2/1-2:1.0/host1/target1:0:0/1:0:0:0"),
        );
        fs::write(sdb.join("removable"), "1\n").unwrap();
        fs::create_dir_all(sys.join("block/loop0")).unwrap();
        let s0 = link("class/tty", "ttyS0", &dev("platform/serial8250/tty/ttyS0"));
        fs::write(s0.join("type"), "4\n").unwrap();
        let s1 = link("class/tty", "ttyS1", &dev("platform/serial8250/tty/ttyS1"));
        fs::write(s1.join("type"), "0\n").unwrap();
        d
    }

    #[test]
    fn discovery_names_by_stable_rules() {
        let d = fake_sys();
        let found = discover(d.path());
        let names: Vec<&str> = found.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "disk-pci-0000.00.04.0",
                "disk-pci-0000.00.14.0-usb-1-2",
                "disk-pci-0000.00.1f.2-ata-1",
                "net0",
                "net1",
                "ttyS0"
            ]
        );
        let get = |n: &str| found.iter().find(|(m, _)| m == n).unwrap().1.clone();
        let vda = get("disk-pci-0000.00.04.0");
        assert_eq!((vda.node.as_str(), vda.size), ("/dev/vda", 16777216 * 512));
        assert_eq!(get("disk-pci-0000.00.1f.2-ata-1").model, "Samsung SSD");
        assert!(get("disk-pci-0000.00.14.0-usb-1-2").removable);
        assert_eq!(get("net0").mac, "52:54:00:12:34:56");
        assert_eq!(get("net1").node, "/sys/class/net/enp2s0");
        for (n, _) in &found {
            n.parse::<ObjectId>().ok();
            assert!(format!("dev:{n}").parse::<ObjectId>().is_ok(), "{n}");
        }
    }

    #[test]
    fn pci_addresses_and_usb_ports() {
        assert!(is_pci_addr("0000:00:1f.2") && !is_pci_addr("pci0000:00"));
        assert!(is_usb_port("1-2") && is_usb_port("1-2.4") && !is_usb_port("1-2:1.0"));
        assert_eq!(id_safe("pci-0000:00:1f.2-ata-1"), "pci-0000.00.1f.2-ata-1");
    }
}
