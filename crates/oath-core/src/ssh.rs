//! Host keys under `/oath/ssh/`; authorized_keys is a converge side effect.

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};
use crate::kinds::{Ssh, SshActual};

pub const HOST_KEY: &str = "/oath/ssh/host_ed25519";
pub const AUTHORIZED: &str = "/root/.ssh/authorized_keys";

pub fn converge(desired: &Ssh) -> Result<SshActual> {
    fs::create_dir_all("/oath/ssh")?;
    let host = Path::new(HOST_KEY);
    if !host.is_file() {
        let st = Command::new("/bin/dropbearkey")
            .args(["-t", "ed25519", "-f", HOST_KEY])
            .status()
            .map_err(|e| Error::Msg(format!("dropbearkey: {e}")))?;
        if !st.success() {
            return Err(Error::hint("dropbearkey failed", "oath get pkg:dropbear"));
        }
    }
    fs::create_dir_all("/root/.ssh")?;
    let mut body = String::new();
    for k in &desired.authorized {
        let k = k.trim();
        if !k.is_empty() {
            body.push_str(k);
            body.push('\n');
        }
    }
    fs::write(AUTHORIZED, &body)?;
    let _ = Command::new("/bin/chown").args(["-R", "0:0", "/root/.ssh"]).status();
    let _ = Command::new("/bin/chmod").args(["700", "/root/.ssh"]).status();
    let _ = Command::new("/bin/chmod").args(["600", AUTHORIZED]).status();
    Ok(SshActual { authorized: desired.authorized.clone(), host_key: host.is_file() })
}
