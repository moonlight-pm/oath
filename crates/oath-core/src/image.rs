//! Desk image vs Wicket image. Absence of the marker file means desk.

use std::fs;

use serde_json::{json, Value};

use crate::seat::is_seat_svc;

pub const IMAGE_PATH: &str = "/lib/oath/image";
pub const IMAGE_WICKET: &str = "wicket";

pub const STATE_SUBVOL: &str = "@wicket-state";
pub const APPDATA_SUBVOL: &str = "@appdata";
pub const STATE_MOUNT: &str = "/var/lib/wicket";
pub const APPDATA_MOUNT: &str = "/var/lib/wicket-appdata";

pub fn is_wicket_text(text: &str) -> bool {
    text.trim() == IMAGE_WICKET
}

pub fn is_wicket_image() -> bool {
    fs::read_to_string(IMAGE_PATH).map(|s| is_wicket_text(&s)).unwrap_or(false)
}

/// `svc:wicketd` stays stopped until L1 is a real mount, not a directory on `@`.
pub fn wicketd_needs_state(id: &str) -> bool {
    id == "svc:wicketd" || id == "wicketd"
}

pub fn mountinfo_has(mountinfo: &str, target: &str) -> bool {
    mountinfo.lines().any(|line| mountinfo_target(line) == Some(target))
}

fn mountinfo_target(line: &str) -> Option<&str> {
    line.split(' ').nth(4)
}

/// Dropbear argv for the installed `svc:sshd`. `-s` keeps passwords off.
/// `-w` (no root) stays on the desk image only.
pub fn sshd_exec(wicket: bool) -> Vec<&'static str> {
    if wicket {
        vec!["/bin/dropbear", "-F", "-E", "-s", "-r", "/oath/ssh/host_ed25519", "-p", "22"]
    } else {
        vec!["/bin/dropbear", "-F", "-E", "-s", "-w", "-r", "/oath/ssh/host_ed25519", "-p", "22"]
    }
}

pub fn ssh_key_file(wicket: bool) -> &'static str {
    if wicket {
        "/root/.ssh/authorized_keys"
    } else {
        "/home/.ssh/authorized_keys"
    }
}

pub fn ssh_home(wicket: bool) -> &'static str {
    if wicket {
        "/root"
    } else {
        "/home"
    }
}

/// DHCP installs record `ipv4=dhcp`. The observed lease is not copied in.
pub fn dhcp_net_document(observed_lease: &str) -> String {
    let _ = observed_lease;
    json!({"up": true, "ipv4": "dhcp", "gateway": "", "lease": null}).to_string()
}

/// Wicket catalog adjustments for one `svc` desired document.
/// Returns whether the document changed.
pub fn adjust_wicket_svc(name: &str, desired: &mut Value) -> bool {
    let mut changed = false;
    if name == "sshd" {
        desired["exec"] = json!(sshd_exec(true));
        changed = true;
    }
    if is_seat_svc(name) {
        if desired.get("enabled") != Some(&json!(false)) {
            desired["enabled"] = json!(false);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn marker_and_mount_gate() {
        assert!(is_wicket_text("wicket\n"));
        assert!(!is_wicket_text("desk"));
        assert!(wicketd_needs_state("svc:wicketd"));
        assert!(!wicketd_needs_state("svc:sshd"));
        let info = "36 35 0:1 / /var/lib/wicket rw - btrfs /dev/sda2 rw,subvol=@wicket-state\n";
        assert!(mountinfo_has(info, "/var/lib/wicket"));
        assert!(!mountinfo_has(info, "/var/lib/wicket-appdata"));
    }

    #[test]
    fn root_ssh_keeps_passwords_off() {
        let desk = sshd_exec(false);
        let wicket = sshd_exec(true);
        assert!(desk.contains(&"-w"));
        assert!(desk.contains(&"-s"));
        assert!(!wicket.contains(&"-w"));
        assert!(wicket.contains(&"-s"));
        assert_eq!(ssh_key_file(true), "/root/.ssh/authorized_keys");
        assert_eq!(ssh_key_file(false), "/home/.ssh/authorized_keys");
    }

    #[test]
    fn dhcp_document_drops_the_lease() {
        let doc = dhcp_net_document("10.0.0.149/24");
        assert!(doc.contains("\"ipv4\":\"dhcp\""));
        assert!(!doc.contains("10.0.0.149"));
    }

    #[test]
    fn wicket_sshd_allows_root_and_seat_stays_off() {
        let mut sshd = json!({"exec": ["/bin/dropbear", "-F", "-s", "-w"], "enabled": true});
        assert!(adjust_wicket_svc("sshd", &mut sshd));
        let exec = sshd["exec"].as_array().unwrap();
        assert!(exec.iter().all(|v| v != "-w"));
        assert!(exec.iter().any(|v| v == "-s"));

        let mut river = json!({"exec": ["/bin/river"], "enabled": true});
        assert!(adjust_wicket_svc("river", &mut river));
        assert_eq!(river["enabled"], false);

        let mut hold = json!({"exec": ["/bin/sleep"], "enabled": true});
        assert!(!adjust_wicket_svc("hold", &mut hold));
        assert_eq!(hold["enabled"], true);
    }
}
