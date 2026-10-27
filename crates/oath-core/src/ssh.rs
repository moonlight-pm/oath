//! Host keys under `/oath/ssh/`, sshd's config, and authorized_keys (a
//! converge side effect). sshd is OpenSSH's (R36).

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};
use crate::kinds::{Ssh, SshActual};

pub const HOST_KEY: &str = "/oath/ssh/ssh_host_ed25519_key";
/// The key dropbear served before R36: carried over once, so the host
/// keeps the key its known_hosts pins name.
pub const DROPBEAR_HOST_KEY: &str = "/oath/ssh/host_ed25519";
pub const AUTHORIZED: &str = "/home/.ssh/authorized_keys";
pub const SSHD_CONFIG: &str = "/etc/ssh/sshd_config";

pub fn converge(desired: &Ssh) -> Result<SshActual> {
    fs::create_dir_all("/oath/ssh")?;
    ensure_host_key()?;
    let home = crate::seat::HOME;
    let authorized = AUTHORIZED;
    let ssh_dir = format!("{home}/.ssh");
    fs::create_dir_all(&ssh_dir)?;
    let mut body = String::new();
    for k in &desired.authorized {
        let k = k.trim();
        if !k.is_empty() {
            body.push_str(k);
            body.push('\n');
        }
    }
    fs::write(authorized, &body)?;
    let own = format!("{}:{}", crate::seat::UID, crate::seat::GID);
    let _ = Command::new("/bin/chown").args(["-R", &own, home]).status();
    let _ = Command::new("/bin/chmod").args(["755", home]).status();
    let _ = Command::new("/bin/chmod").args(["700", &ssh_dir]).status();
    let _ = Command::new("/bin/chmod").args(["600", authorized]).status();
    Ok(SshActual {
        authorized: desired.authorized.clone(),
        host_key: Path::new(HOST_KEY).is_file(),
    })
}

/// The host key: dropbear's, in OpenSSH's format, when the machine had
/// one; else a new one.
fn ensure_host_key() -> Result<()> {
    if Path::new(HOST_KEY).is_file() {
        return Ok(());
    }
    if let Ok(old) = fs::read(DROPBEAR_HOST_KEY) {
        let (private, public) = openssh_from_dropbear(&old)?;
        write_new(&format!("{HOST_KEY}.pub"), public.as_bytes(), 0o644)?;
        return write_new(HOST_KEY, private.as_bytes(), 0o600);
    }
    // -q: oath's stdout is for oath's answer.
    let st = Command::new("/bin/ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-C", "", "-f", HOST_KEY])
        .stdout(std::io::stderr())
        .status()
        .map_err(|e| Error::Msg(format!("ssh-keygen: {e}")))?;
    if !st.success() {
        return Err(Error::hint("ssh-keygen failed", "oath get pkg:openssh"));
    }
    Ok(())
}

/// `path` written whole with `mode`, through a temporary file.
fn write_new(path: &str, body: &[u8], mode: u32) -> Result<()> {
    let tmp = format!("{path}.oath-new");
    let _ = fs::remove_file(&tmp);
    let mut f = fs::OpenOptions::new().write(true).create_new(true).mode(mode).open(&tmp)?;
    f.write_all(body)?;
    f.sync_all()?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Dropbear's ed25519 file is `string "ssh-ed25519"`, `uint32 64`, the
/// 32-byte seed and the 32-byte public key. OpenSSH's is PROTOCOL.key's
/// unencrypted `openssh-key-v1` holding the same seed and public key.
/// Returns the private key file and the `.pub` line.
pub fn openssh_from_dropbear(b: &[u8]) -> Result<(String, String)> {
    let bad = || Error::Msg("not a dropbear ed25519 host key".into());
    let mut r = b;
    let kind = take_string(&mut r).ok_or_else(bad)?;
    let key = take_string(&mut r).ok_or_else(bad)?;
    if kind != b"ssh-ed25519" || key.len() != 64 || !r.is_empty() {
        return Err(bad());
    }
    let public = &key[32..];
    let mut blob = Vec::new();
    put_string(&mut blob, b"ssh-ed25519");
    put_string(&mut blob, public);
    // The check value only proves a decryption; any value will do.
    let check = &public[..4];
    let mut private = Vec::new();
    private.extend_from_slice(check);
    private.extend_from_slice(check);
    put_string(&mut private, b"ssh-ed25519");
    put_string(&mut private, public);
    put_string(&mut private, key);
    put_string(&mut private, b"");
    let mut pad = 1u8;
    while private.len() % 8 != 0 {
        private.push(pad);
        pad += 1;
    }
    let mut file = b"openssh-key-v1\0".to_vec();
    put_string(&mut file, b"none");
    put_string(&mut file, b"none");
    put_string(&mut file, b"");
    file.extend_from_slice(&1u32.to_be_bytes());
    put_string(&mut file, &blob);
    put_string(&mut file, &private);
    let text = base64(&file);
    let mut pem = String::from("-----BEGIN OPENSSH PRIVATE KEY-----\n");
    for line in text.as_bytes().chunks(70) {
        pem.push_str(std::str::from_utf8(line).unwrap_or_default());
        pem.push('\n');
    }
    pem.push_str("-----END OPENSSH PRIVATE KEY-----\n");
    Ok((pem, format!("ssh-ed25519 {}\n", base64(&blob))))
}

fn take_string<'a>(r: &mut &'a [u8]) -> Option<&'a [u8]> {
    let n = u32::from_be_bytes(r.get(..4)?.try_into().ok()?) as usize;
    let s = r.get(4..4 + n)?;
    *r = &r[4 + n..];
    Some(s)
}

fn put_string(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(&(s.len() as u32).to_be_bytes());
    out.extend_from_slice(s);
}

fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// `/etc/ssh/sshd_config`, written with `/etc/profile` from `host:local`.
/// sshd reads it when it starts. `SetEnv` gives every session `host:local`
/// env, as `/etc/profile` gives a login shell; sshd keeps only the first
/// `SetEnv` line, so it is one line, and the account's own variables are
/// sshd's to set.
pub fn sshd_config(env: &BTreeMap<String, String>) -> String {
    let mut body = format!(
        "# generated by oath from host:local env (root-owned)\n\
         HostKey {HOST_KEY}\n\
         AuthorizedKeysFile .ssh/authorized_keys\n\
         PermitRootLogin no\n\
         PasswordAuthentication no\n\
         KbdInteractiveAuthentication no\n\
         PidFile none\n\
         PrintMotd no\n\
         Subsystem sftp /bin/sftp-server\n"
    );
    let own = ["USER", "LOGNAME", "HOME", "SHELL", "PATH", "MAIL"];
    let set: Vec<String> = env
        .iter()
        .filter(|(k, v)| {
            crate::seat::valid_env_name(k)
                && !own.contains(&k.as_str())
                && !v.contains(['\n', '\0'])
        })
        .map(|(k, v)| format!("\"{k}={}\"", v.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    if !set.is_empty() {
        body.push_str("SetEnv ");
        body.push_str(&set.join(" "));
        body.push('\n');
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway key from dropbearkey 2025.89, and what `dropbearkey -y`
    /// printed for it.
    const DROPBEAR: &str = "0000000b7373682d6564323535313900000040b08b757f00c326c4cad6f9b58e698d\
                            dead52f236320c5e74320eb1f12539d3aeed5a7bb2e0d3c67e7432167e96ce0c7e1d\
                            6a87efcc28ac5afaaea66baf2876fa";
    const PUBLIC: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIO1ae7Lg08Z+dDIWfpbODH4daofvzCisWvqupmuvKHb6";

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn dropbear_key_becomes_the_same_openssh_key() {
        let (private, public) = openssh_from_dropbear(&unhex(DROPBEAR)).unwrap();
        assert_eq!(public.trim(), PUBLIC);
        assert!(private.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEA"));
        assert!(private.lines().all(|l| l.len() <= 70 || l.starts_with("-----")));
        assert!(openssh_from_dropbear(&unhex(DROPBEAR)[..82]).is_err());
        assert!(openssh_from_dropbear(b"junk").is_err());
    }

    /// OpenSSH reads the converted file back to the same public key (where
    /// the host running the tests has ssh-keygen).
    #[test]
    fn ssh_keygen_reads_the_converted_key() {
        let (private, _) = openssh_from_dropbear(&unhex(DROPBEAR)).unwrap();
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("key");
        write_new(f.to_str().unwrap(), private.as_bytes(), 0o600).unwrap();
        let Ok(out) = Command::new("ssh-keygen").arg("-y").arg("-f").arg(&f).output() else {
            return;
        };
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), PUBLIC);
    }

    #[test]
    fn base64_pads() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
    }

    #[test]
    fn sshd_config_sets_host_env_on_one_line() {
        let mut env = BTreeMap::new();
        env.insert("LANG".to_string(), "en_US.UTF-8".to_string());
        env.insert("TZ".to_string(), "MST7MDT,M3.2.0,M11.1.0".to_string());
        env.insert("ODD".to_string(), "a \"b\" \\c".to_string());
        env.insert("PATH".to_string(), "/nope".to_string());
        let c = sshd_config(&env);
        assert!(c.contains("PermitRootLogin no\n"));
        assert!(c.contains(&format!("HostKey {HOST_KEY}\n")));
        assert_eq!(c.matches("SetEnv").count(), 1);
        assert!(c.contains(
            "SetEnv \"LANG=en_US.UTF-8\" \"ODD=a \\\"b\\\" \\\\c\" \"TZ=MST7MDT,M3.2.0,M11.1.0\"\n"
        ));
        assert!(!c.contains("/nope"));
        assert!(!sshd_config(&BTreeMap::new()).contains("SetEnv"));
    }
}
