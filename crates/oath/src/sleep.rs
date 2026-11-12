//! `host:local.power=sleep` (R84): suspend to RAM and return on wake.
//! Before it sleeps, each keyboard and mouse is armed to wake the machine
//! (`net:net0.wake` arms the NIC, and the power button always wakes it),
//! and NVIDIA's driver is suspended through its own interface, the only
//! way it keeps video memory (`NVreg_PreserveVideoMemoryAllocations=1`,
//! PID 1's default). After the wake, or when the kernel refuses, the
//! driver resumes.

use std::fs;
use std::path::{Path, PathBuf};

use oath_core::{now_rfc3339, tel, Error, Result, Slept};
use serde_json::json;

const STATE: &str = "/sys/power/state";
const MEM_SLEEP: &str = "/sys/power/mem_sleep";
/// NVIDIA's closed modules take no kernel suspend notifiers, so whoever
/// suspends writes `suspend` here first and `resume` after (its README,
/// "Configuring Power Management Support").
const NVIDIA: &str = "/proc/driver/nvidia/suspend";

pub fn sleep() -> Result<Slept> {
    let sys = Path::new("/sys");
    let states = fs::read_to_string(STATE).unwrap_or_default();
    if !states.split_whitespace().any(|s| s == "mem") {
        return Err(Error::hint(
            format!("this machine offers no sleep: {STATE} is {:?}", states.trim()),
            format!("cat {STATE}"),
        ));
    }
    let state = chosen(&fs::read_to_string(MEM_SLEEP).unwrap_or_default());
    let armed = arm_inputs(sys);
    let mut wake: Vec<String> = Vec::new();
    for (_, label) in &armed {
        if !wake.contains(label) {
            wake.push(label.clone());
        }
    }
    let net0 = fs::canonicalize(sys.join("class/net/net0/device")).ok();
    if net0.as_ref().is_some_and(|d| enabled(d)) {
        wake.push("net0".into());
    }
    let before = wake_counts(sys);
    let nvidia = Path::new(NVIDIA).exists();
    if nvidia {
        if let Err(e) = fs::write(NVIDIA, "suspend") {
            let _ = fs::write(NVIDIA, "resume");
            return Err(Error::hint(
                format!("could not sleep: NVIDIA's driver would not suspend ({e})"),
                "sudo dmesg | tail -n 30",
            ));
        }
    }
    let at = now_rfc3339();
    tel("oath", "sleep", json!({ "state": state, "wake": wake }));
    // Blocks until the machine wakes, or fails with no sleep.
    let slept = fs::write(STATE, "mem");
    let woke = now_rfc3339();
    let resumed = if nvidia { fs::write(NVIDIA, "resume") } else { Ok(()) };
    let mut woken_by: Vec<String> = Vec::new();
    for (dev, name, n) in wake_counts(sys) {
        let was = before.iter().find(|(d, w, _)| *d == dev && *w == name).map_or(0, |b| b.2);
        if n <= was {
            continue;
        }
        let label = if net0.as_ref() == Some(&dev) {
            "net0".to_string()
        } else {
            armed.iter().find(|(d, _)| *d == dev).map_or(name, |(_, l)| l.clone())
        };
        if !woken_by.contains(&label) {
            woken_by.push(label);
        }
    }
    tel(
        "oath",
        "woke",
        json!({ "ok": slept.is_ok(), "err": slept.as_ref().err().map(|e| e.to_string()), "woken_by": woken_by }),
    );
    if let Err(e) = slept {
        return Err(Error::hint(
            format!(
                "could not sleep: the kernel refused ({e}): a device would not suspend, or a \
                 wake came first"
            ),
            "sudo dmesg | tail -n 30",
        ));
    }
    if let Err(e) = resumed {
        return Err(Error::hint(
            format!("woke at {woke}, but NVIDIA's driver would not resume ({e})"),
            "sudo dmesg | tail -n 30",
        ));
    }
    Ok(Slept { state, at, woke, wake, woken_by })
}

/// The bracketed choice in `/sys/power/mem_sleep` (`s2idle [deep]`).
fn chosen(mem_sleep: &str) -> String {
    mem_sleep
        .split_whitespace()
        .find_map(|w| w.strip_prefix('[')?.strip_suffix(']'))
        .unwrap_or("s2idle")
        .to_string()
}

/// Arm each keyboard and mouse on USB or a PS/2 port to wake the machine,
/// as Windows does: `power/wakeup` on the device and on each one above it
/// (hubs, the controller), up to and including the first PCI device. A
/// pad, a pen or a Bluetooth device is left as it is. Returns each armed
/// device with its name (the USB product, else the input's).
fn arm_inputs(sys: &Path) -> Vec<(PathBuf, String)> {
    let mut armed: Vec<(PathBuf, String)> = Vec::new();
    let Ok(entries) = fs::read_dir(sys.join("class/input")) else { return armed };
    let mut inputs: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("input"))
        .map(|e| e.path())
        .collect();
    inputs.sort();
    for input in inputs {
        let read = |f: &str| fs::read_to_string(input.join(f)).unwrap_or_default();
        // BUS_USB and BUS_I8042 (linux/input.h).
        if !matches!(read("id/bustype").trim(), "0003" | "0011") {
            continue;
        }
        let (key, rel) = (read("capabilities/key"), read("capabilities/rel"));
        if !keyboard(&key) && !mouse(&key, &rel) {
            continue;
        }
        let Ok(dev) = fs::canonicalize(&input) else { continue };
        let mut label = read("name").trim().to_string();
        let mut wakes: Option<PathBuf> = None;
        for up in dev.ancestors().skip(1) {
            if up == sys.join("devices") || !up.starts_with(sys.join("devices")) {
                break;
            }
            if wakes.is_none() {
                if let Ok(p) = fs::read_to_string(up.join("product")) {
                    label = p.trim().to_string();
                }
            }
            if up.join("power/wakeup").exists() {
                let _ = fs::write(up.join("power/wakeup"), "enabled");
                wakes.get_or_insert_with(|| up.to_path_buf());
            }
            let pci = fs::read_link(up.join("subsystem"))
                .is_ok_and(|s| s.file_name().is_some_and(|n| n == "pci"));
            if pci {
                break;
            }
        }
        if let Some(w) = wakes {
            if !armed.iter().any(|(d, _)| *d == w) {
                armed.push((w, label));
            }
        }
    }
    armed
}

/// Bit `n` of a sysfs capability bitmap: hex words, the highest first.
fn bit(bitmap: &str, n: usize) -> bool {
    let words: Vec<&str> = bitmap.split_whitespace().rev().collect();
    words
        .get(n / 64)
        .and_then(|w| u64::from_str_radix(w, 16).ok())
        .is_some_and(|w| w >> (n % 64) & 1 == 1)
}

/// Escape, the digits and Q to D, as udev's `input_id` tells a keyboard.
fn keyboard(key: &str) -> bool {
    (1..32).all(|k| bit(key, k))
}

/// REL_X, REL_Y and BTN_LEFT.
fn mouse(key: &str, rel: &str) -> bool {
    bit(rel, 0) && bit(rel, 1) && bit(key, 0x110)
}

fn enabled(dev: &Path) -> bool {
    fs::read_to_string(dev.join("power/wakeup")).is_ok_and(|w| w.trim() == "enabled")
}

/// Each wakeup source's device, name and wake count.
fn wake_counts(sys: &Path) -> Vec<(PathBuf, String, u64)> {
    let Ok(entries) = fs::read_dir(sys.join("class/wakeup")) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let dev = fs::canonicalize(p.join("device")).ok()?;
            let name = fs::read_to_string(p.join("name")).ok()?.trim().to_string();
            let n = fs::read_to_string(p.join("wakeup_count")).ok()?.trim().parse().ok()?;
            Some((dev, name, n))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bracketed_state_is_the_one_used() {
        assert_eq!(chosen("s2idle [deep]\n"), "deep");
        assert_eq!(chosen("[s2idle]\n"), "s2idle");
    }

    #[test]
    fn keyboards_and_mice_by_their_capabilities() {
        // A USB keyboard (Lamzu receiver's interface 0) and a mouse, as
        // sysfs prints them on x86_64.
        let kbd = "1000000000007 ff9f207ac14057ff febeffdfffefffff fffffffffffffffe";
        assert!(keyboard(kbd));
        let (mkey, mrel) = ("1f0000 0 0 0 0", "1943");
        assert!(mouse(mkey, mrel) && !keyboard(mkey));
        // A pad: buttons but no REL_X, no letters.
        assert!(!mouse("7fdb000000000000 0 0 0 0", "0") && !keyboard("7fdb000000000000 0 0 0 0"));
        assert!(!keyboard("") && !mouse("", ""));
    }

    #[test]
    fn arms_a_usb_mouse_and_its_hub_and_controller_only() {
        let t = std::env::temp_dir().join(format!("oath-sleep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        let sys = t.join("sys");
        let ctl = sys.join("devices/pci0000:00/0000:00:14.0");
        let root_port = sys.join("devices/pci0000:00");
        let usb = ctl.join("usb1/1-4");
        let input = usb.join("1-4:1.0/0003:3554:F510.0001/input/input7");
        for d in [&root_port, &ctl, &ctl.join("usb1"), &usb, &input] {
            fs::create_dir_all(d.join("power")).unwrap();
            fs::write(d.join("power/wakeup"), "disabled\n").unwrap();
        }
        fs::create_dir_all(input.join("id")).unwrap();
        fs::create_dir_all(input.join("capabilities")).unwrap();
        fs::write(input.join("id/bustype"), "0003\n").unwrap();
        fs::write(input.join("capabilities/key"), "1f0000 0 0 0 0\n").unwrap();
        fs::write(input.join("capabilities/rel"), "1943\n").unwrap();
        fs::write(input.join("name"), "Lamzu Mouse\n").unwrap();
        fs::write(usb.join("product"), "Lamzu 4K Receiver\n").unwrap();
        fs::create_dir_all(sys.join("bus/pci")).unwrap();
        std::os::unix::fs::symlink(sys.join("bus/pci"), ctl.join("subsystem")).unwrap();
        fs::create_dir_all(sys.join("class/input")).unwrap();
        std::os::unix::fs::symlink(&input, sys.join("class/input/input7")).unwrap();

        let armed = arm_inputs(&sys);
        assert_eq!(armed, [(fs::canonicalize(&usb).unwrap(), "Lamzu 4K Receiver".to_string())]);
        let state = |d: &Path| fs::read_to_string(d.join("power/wakeup")).unwrap();
        for d in [&usb, &ctl.join("usb1"), &ctl] {
            assert_eq!(state(d), "enabled", "{}", d.display());
        }
        // The input node itself has no wake of its own to arm here, and
        // nothing above the controller is touched.
        assert_eq!(state(&root_port), "disabled\n");
        let _ = fs::remove_dir_all(&t);
    }
}
