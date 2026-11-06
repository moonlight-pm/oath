//! The seat's HID nodes: game controllers' (R30) and the vendor interfaces
//! of the devices `host:local.seat_hid` lists (R71). PID 1 opens them at
//! every plug; apply when `seat_hid` changes.

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

use crate::seat::GID;

/// Make the seat's HID nodes under `dev` root:home 0660: each game
/// controller's ([`pad_nodes`]) and each interface of a listed device that
/// [`vendor_interface`] admits ([`seat_nodes`]). A `hidraw*` the seat has
/// and no longer gets (its device left `seat_hid`) is root's 0600 again.
/// Returns the listed devices' nodes.
pub fn open_nodes(sys: &Path, dev: &Path, seat_hid: &[String]) -> Vec<String> {
    let seat = seat_nodes(sys, seat_hid);
    let mut nodes = pad_nodes(sys);
    nodes.extend(seat.iter().cloned());
    for n in &nodes {
        let p = dev.join(n);
        let _ = std::os::unix::fs::chown(&p, Some(0), Some(GID));
        let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o660));
    }
    for h in names(dev).into_iter().filter(|n| n.starts_with("hidraw")) {
        let p = dev.join(&h);
        if !nodes.contains(&h) && fs::metadata(&p).is_ok_and(|m| m.gid() == GID) {
            let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o600));
            let _ = std::os::unix::fs::chown(&p, Some(0), Some(0));
        }
    }
    seat
}

/// `host:local.seat_hid` as applied in the catalog at `root`.
pub fn applied_seat_hid(root: &Path) -> Vec<String> {
    crate::read_json::<serde_json::Value>(&root.join("objects/host/local/applied.json"))
        .ok()
        .and_then(|spec| serde_json::from_value(spec.get("seat_hid")?.clone()).ok())
        .unwrap_or_default()
}

/// The nodes (under `/dev`) of game controllers: every input device the
/// kernel's joystick driver took (it has a `js*`), its `js*` and `event*`
/// nodes, and the `hidraw*` of the HID device each is part of. joydev
/// leaves out touchpads, tablets and accelerometers.
pub fn pad_nodes(sys: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut pads = Vec::new();
    for js in names(&sys.join("class/input")).into_iter().filter(|n| n.starts_with("js")) {
        let Ok(dev) = fs::canonicalize(sys.join("class/input").join(&js)) else { continue };
        let Some(input) = dev.parent() else { continue };
        out.push(format!("input/{js}"));
        for ev in names(input).into_iter().filter(|n| n.starts_with("event")) {
            out.push(format!("input/{ev}"));
        }
        pads.push(input.to_path_buf());
    }
    for h in names(&sys.join("class/hidraw")) {
        let Ok(hid) = fs::canonicalize(sys.join("class/hidraw").join(&h).join("device")) else {
            continue;
        };
        if pads.iter().any(|p| p.starts_with(&hid)) {
            out.push(h);
        }
    }
    out.sort();
    out
}

/// The `hidraw*` nodes of the devices `seat_hid` lists (`vendor:product`,
/// either case) whose interface [`vendor_interface`] admits.
pub fn seat_nodes(sys: &Path, seat_hid: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    if seat_hid.is_empty() {
        return out;
    }
    for h in names(&sys.join("class/hidraw")) {
        let device = sys.join("class/hidraw").join(&h).join("device");
        let uevent = fs::read_to_string(device.join("uevent")).unwrap_or_default();
        let Some(id) = device_id(&uevent) else { continue };
        if !seat_hid.iter().any(|s| s.eq_ignore_ascii_case(&id)) {
            continue;
        }
        if vendor_interface(&fs::read(device.join("report_descriptor")).unwrap_or_default()) {
            out.push(h);
        }
    }
    out.sort();
    out
}

/// A `seat_hid` entry: four hex digits, `:`, four hex digits (`3554:f510`).
pub fn is_device_id(s: &str) -> bool {
    s.split_once(':').is_some_and(|(v, p)| {
        [v, p].iter().all(|x| x.len() == 4 && x.bytes().all(|b| b.is_ascii_hexdigit()))
    })
}

/// `vendor:product` from a HID device's uevent: `HID_ID=0003:00003554:0000F510`
/// is `3554:f510`.
fn device_id(uevent: &str) -> Option<String> {
    let id = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut parts = id.split(':').skip(1);
    let vendor = u32::from_str_radix(parts.next()?, 16).ok()?;
    let product = u32::from_str_radix(parts.next()?, 16).ok()?;
    Some(format!("{:04x}:{:04x}", vendor & 0xffff, product & 0xffff))
}

/// An interface the seat may have (R71): its report descriptor has a
/// vendor-defined top-level collection (usage page 0xFF00 to 0xFFFF) and
/// none for what a person types or points with (Generic Desktop pointer,
/// mouse, keyboard or keypad; any digitizer), so its reports carry no keys
/// or motion (R44).
pub fn vendor_interface(descriptor: &[u8]) -> bool {
    let tops = top_collections(descriptor);
    let typed_or_pointed = |&(page, usage): &(u32, u32)| {
        page == 0x0d || (page == 0x01 && matches!(usage, 0x01 | 0x02 | 0x06 | 0x07))
    };
    tops.iter().any(|&(page, _)| page >= 0xff00) && !tops.iter().any(typed_or_pointed)
}

/// The (usage page, usage) of each top-level collection in a HID report
/// descriptor. A truncated descriptor gives what it has.
fn top_collections(d: &[u8]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let (mut page, mut usage, mut depth) = (0u32, None::<u32>, 0usize);
    let mut pushed = Vec::new();
    let mut i = 0;
    while i < d.len() {
        let prefix = d[i];
        // A long item: its data size, its tag, its data.
        if prefix == 0xfe {
            i += 3 + d.get(i + 1).copied().unwrap_or(0) as usize;
            continue;
        }
        let size = [0, 1, 2, 4][usize::from(prefix & 3)];
        let Some(data) = d.get(i + 1..i + 1 + size) else { break };
        let value = data.iter().rev().fold(0u32, |v, &b| (v << 8) | u32::from(b));
        match prefix & 0xfc {
            0x04 => page = value,
            0xa4 => pushed.push(page),
            0xb4 => page = pushed.pop().unwrap_or(page),
            // A four-byte usage carries its own page.
            0x08 if usage.is_none() => {
                usage = Some(if size == 4 { value } else { (page << 16) | value });
            }
            0xa0 => {
                if depth == 0 {
                    let u = usage.unwrap_or(page << 16);
                    out.push((u >> 16, u & 0xffff));
                }
                depth += 1;
            }
            0xc0 => depth = depth.saturating_sub(1),
            _ => {}
        }
        // A main item ends the local items before it.
        if prefix & 0x0c == 0 {
            usage = None;
        }
        i += 1 + size;
    }
    out
}

fn names(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn bytes(hex: &str) -> Vec<u8> {
        hex.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    /// novus's Lamzu receiver (3554:f510), 2026-11-06: interface 1 has
    /// five vendor collections (report 8 answers with the battery) beside
    /// its media and power keys; interface 0 is a keyboard, 2 the mouse.
    const LAMZU_1: &str = "06 05 ff 09 00 a1 01 85 10 09 00 15 00 26 ff 00 75 08 95 07 81 02 c0 \
        06 03 ff 09 00 a1 01 85 02 09 00 15 00 26 ff 00 75 08 95 07 81 02 c0 05 0c 09 01 a1 01 \
        85 05 15 00 26 3c 02 19 00 2a 3c 02 75 10 95 01 81 00 c0 05 01 09 80 a1 01 85 03 19 81 \
        29 83 15 00 25 01 95 03 75 01 81 02 95 01 75 05 81 01 c0 06 02 ff 09 02 a1 01 85 08 15 \
        00 26 ff 00 75 08 95 10 09 02 81 00 09 02 91 00 c0 06 04 ff 09 02 a1 01 85 06 09 02 15 \
        00 26 ff 00 75 08 95 07 b1 02 c0 06 06 ff 09 02 a1 01 85 09 15 00 26 ff 00 75 08 95 30 \
        09 02 81 00 09 02 91 00 c0";
    const LAMZU_0: &str = "05 01 09 06 a1 01 05 08 19 01 29 03 15 00 25 01 75 01 95 03 91 02 \
        95 05 91 01 05 07 19 e0 29 e7 95 08 81 02 75 08 95 01 81 01 05 07 19 00 2a ff 00 26 ff \
        00 95 06 81 00 c0";
    /// The first bytes of interface 2's descriptor.
    const LAMZU_2: &str = "05 01 09 02 a1 01 09 01 a1 00 05 09 19 01 29 05 15 00 25 01 95 05 \
        75 01 81 02 95 01 75 03 81 01 05 01 09 30 09 31 16 00 80 26 ff 7f 75 10 95 02 81 06 c0";
    /// The first bytes of an Apple Magic Keyboard's interface 0: a keyboard,
    /// then a vendor collection.
    const APPLE_0: &str = "05 01 09 06 a1 01 06 00 ff 09 0b 15 00 26 ff 00 75 08 96 04 00 85 \
        e0 81 22 09 0b 96 01 00 85 9a 81 22 c0 06 00 ff 09 14 a1 01 85 90";

    #[test]
    fn top_collections_of_a_receiver() {
        assert_eq!(
            top_collections(&bytes(LAMZU_1)),
            [
                (0xff05, 0),
                (0xff03, 0),
                (0x0c, 1),
                (0x01, 0x80),
                (0xff02, 2),
                (0xff04, 2),
                (0xff06, 2)
            ]
        );
        assert_eq!(top_collections(&bytes(LAMZU_2)), [(0x01, 0x02)]);
        assert_eq!(top_collections(&bytes(APPLE_0)), [(0x01, 0x06), (0xff00, 0x14)]);
        assert!(top_collections(&[0x06, 0x00]).is_empty());
    }

    #[test]
    fn only_a_vendor_interface_with_no_keys_or_motion() {
        assert!(vendor_interface(&bytes(LAMZU_1)));
        assert!(!vendor_interface(&bytes(LAMZU_0)));
        assert!(!vendor_interface(&bytes(LAMZU_2)));
        assert!(!vendor_interface(&bytes(APPLE_0)));
        assert!(!vendor_interface(&[]));
    }

    #[test]
    fn seat_nodes_are_listed_devices_vendor_interfaces() {
        let d = tempfile::tempdir().unwrap();
        let sys = d.path();
        fs::create_dir_all(sys.join("class/hidraw")).unwrap();
        for (node, id, desc) in [
            ("hidraw2", "0003:000005AC:0000026C", APPLE_0),
            ("hidraw4", "0003:00003554:0000F510", LAMZU_0),
            ("hidraw5", "0003:00003554:0000F510", LAMZU_1),
            ("hidraw6", "0003:00003554:0000F510", LAMZU_2),
        ] {
            let hid = sys.join("devices/usb1").join(node);
            fs::create_dir_all(hid.join("hidraw").join(node)).unwrap();
            fs::write(hid.join("uevent"), format!("DRIVER=hid-generic\nHID_ID={id}\n")).unwrap();
            fs::write(hid.join("report_descriptor"), bytes(desc)).unwrap();
            symlink(&hid, hid.join("hidraw").join(node).join("device")).unwrap();
            symlink(hid.join("hidraw").join(node), sys.join("class/hidraw").join(node)).unwrap();
        }
        assert_eq!(seat_nodes(sys, &["3554:F510".into()]), ["hidraw5"]);
        assert_eq!(seat_nodes(sys, &["05ac:026c".into(), "3554:f511".into()]), [] as [&str; 0]);
        assert!(seat_nodes(sys, &[]).is_empty());
        assert_eq!(device_id("HID_ID=0005:0000046D:0000B023\n").as_deref(), Some("046d:b023"));
    }

    #[test]
    fn pad_nodes_are_joydevs_and_their_hidraw() {
        // A DualSense (its input with js0 and event7, hidraw0) and a
        // keyboard (event3, hidraw1), laid out as sysfs does.
        let d = tempfile::tempdir().unwrap();
        let sys = d.path();
        let pad = sys.join("devices/usb1/1-1/0003:054C:0CE6.0001");
        let kbd = sys.join("devices/usb1/1-2/0003:05AC:024F.0002");
        for dir in [
            pad.join("input/input5/js0"),
            pad.join("input/input5/event7"),
            pad.join("hidraw/hidraw0"),
            kbd.join("input/input2/event3"),
            kbd.join("hidraw/hidraw1"),
            sys.join("class/input"),
            sys.join("class/hidraw"),
        ] {
            fs::create_dir_all(dir).unwrap();
        }
        let input = sys.join("class/input");
        symlink(pad.join("input/input5/js0"), input.join("js0")).unwrap();
        symlink(pad.join("input/input5/event7"), input.join("event7")).unwrap();
        symlink(kbd.join("input/input2/event3"), input.join("event3")).unwrap();
        for (node, hid) in [("hidraw0", &pad), ("hidraw1", &kbd)] {
            symlink(hid, hid.join("hidraw").join(node).join("device")).unwrap();
            symlink(hid.join("hidraw").join(node), sys.join("class/hidraw").join(node)).unwrap();
        }
        assert_eq!(pad_nodes(sys), ["hidraw0", "input/event7", "input/js0"]);
        assert!(pad_nodes(&sys.join("nothing")).is_empty());
    }
}
