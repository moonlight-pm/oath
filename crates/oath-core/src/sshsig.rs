//! OpenSSH signatures (`ssh-keygen -Y sign`, the SSHSIG format) made with
//! ed25519 keys: how a machine checks that a build host it trusts
//! published an output (R38). Only `ssh-ed25519` keys, armored signatures,
//! and SHA-256 or SHA-512 message hashes.

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256, Sha512};

use crate::{Error, Result};

const MAGIC: &[u8] = b"SSHSIG";
const ED25519: &str = "ssh-ed25519";

/// An `ssh-ed25519 AAAA… comment` line: its key blob and comment.
pub fn parse_public_key(line: &str) -> Result<(Vec<u8>, String)> {
    let mut parts = line.split_whitespace();
    let bad = || {
        Error::hint(
            format!("not an ssh-ed25519 public key: {line:?}"),
            "a line of an .pub file: ssh-ed25519 AAAA… name",
        )
    };
    if parts.next() != Some(ED25519) {
        return Err(bad());
    }
    let blob = base64(parts.next().ok_or_else(bad)?).ok_or_else(bad)?;
    let mut r = Reader(&blob);
    if r.string().ok_or_else(bad)? != ED25519.as_bytes()
        || r.string().ok_or_else(bad)?.len() != 32
        || !r.0.is_empty()
    {
        return Err(bad());
    }
    Ok((blob, parts.collect::<Vec<_>>().join(" ")))
}

/// Check `armored` over `message` in `namespace` against `trusted` (public
/// key lines). Returns the comment of the key that signed it.
pub fn verify(
    message: &[u8],
    armored: &str,
    namespace: &str,
    trusted: &[String],
) -> Result<String> {
    let bad = |why: &str| Error::security(format!("signature: {why}"), "republish the output");
    let body: String = armored
        .lines()
        .map(str::trim)
        .skip_while(|l| *l != "-----BEGIN SSH SIGNATURE-----")
        .skip(1)
        .take_while(|l| *l != "-----END SSH SIGNATURE-----")
        .collect();
    let blob = base64(&body).ok_or_else(|| bad("not an armored SSH signature"))?;
    let mut r = Reader(&blob);
    if r.take(MAGIC.len()) != Some(MAGIC) || r.u32() != Some(1) {
        return Err(bad("not SSHSIG version 1"));
    }
    let key = r.string().ok_or_else(|| bad("no key"))?;
    let ns = r.string().ok_or_else(|| bad("no namespace"))?;
    let reserved = r.string().ok_or_else(|| bad("truncated"))?;
    let hash_alg = r.string().ok_or_else(|| bad("no hash"))?;
    let sig = r.string().ok_or_else(|| bad("no signature"))?;
    if ns != namespace.as_bytes() {
        return Err(bad(&format!(
            "made for namespace {:?}, not {namespace:?}",
            String::from_utf8_lossy(ns)
        )));
    }
    let signer = trusted
        .iter()
        .filter_map(|l| parse_public_key(l).ok())
        .find(|(blob, _)| blob.as_slice() == key)
        .map(|(_, comment)| comment)
        .ok_or_else(|| {
            Error::security(
                "signed by a key this machine does not trust",
                "host:local.publishers lists the keys it trusts; a person adds one (R38)",
            )
        })?;
    let digest = match hash_alg {
        b"sha512" => Sha512::digest(message).to_vec(),
        b"sha256" => Sha256::digest(message).to_vec(),
        other => return Err(bad(&format!("hash {:?}", String::from_utf8_lossy(other)))),
    };
    let mut k = Reader(key);
    let (_, raw) = (k.string(), k.string().ok_or_else(|| bad("bad key"))?);
    let vk = VerifyingKey::from_bytes(raw.try_into().map_err(|_| bad("bad key"))?)
        .map_err(|_| bad("bad key"))?;
    let mut s = Reader(sig);
    if s.string() != Some(ED25519.as_bytes()) {
        return Err(bad("not an ed25519 signature"));
    }
    let raw = s.string().ok_or_else(|| bad("bad signature"))?;
    let sig = Signature::from_slice(raw).map_err(|_| bad("bad signature"))?;
    let mut signed = MAGIC.to_vec();
    for field in [namespace.as_bytes(), reserved, hash_alg, &digest] {
        signed.extend_from_slice(&(field.len() as u32).to_be_bytes());
        signed.extend_from_slice(field);
    }
    vk.verify_strict(&signed, &sig)
        .map_err(|_| bad("does not match the file (changed since it was signed?)"))?;
    Ok(signer)
}

/// SSH wire format: big-endian u32 lengths before byte strings.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn string(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }
}

/// Standard base64 with padding.
fn base64(s: &str) -> Option<Vec<u8>> {
    let s = s.trim_end_matches('=');
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ssh-keygen -t ed25519 -C test@oath; ssh-keygen -Y sign -n oath-output msg
    const KEY: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOLHLHCwyfQuElKcnTM149ewbf/H1zglpurRTVmhQyWq test@oath";
    const MSG: &[u8] = b"{\"output\":\"000000000000\",\"profile\":\"base\"}\n";
    const SIG: &str = "-----BEGIN SSH SIGNATURE-----
U1NIU0lHAAAAAQAAADMAAAALc3NoLWVkMjU1MTkAAAAg4scscLDJ9C4SUpydMzXj17Bt/8
fXOCWm6tFNWaFDJaoAAAALb2F0aC1vdXRwdXQAAAAAAAAABnNoYTUxMgAAAFMAAAALc3No
LWVkMjU1MTkAAABAVs/lcV+N5cVwtDLrIorjecmtS4cpUddrxv/4v/DnidiaaHza9rS+Pu
VXt2S1bOjHWAkohjx1Hx5Nz6aGvft/CQ==
-----END SSH SIGNATURE-----
";
    const OTHER: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIH+YKD3yXYsGJMxwpBIWSYP7VuQvjhzGG+LWvYhKTESD oath@buccina";

    #[test]
    fn a_trusted_key_verifies() {
        let who = verify(MSG, SIG, "oath-output", &[OTHER.into(), KEY.into()]).unwrap();
        assert_eq!(who, "test@oath");
    }

    #[test]
    fn a_changed_file_fails() {
        let e = verify(b"{\"output\":\"000000000001\"}\n", SIG, "oath-output", &[KEY.into()]);
        assert!(e.unwrap_err().to_string().contains("does not match"));
    }

    #[test]
    fn an_untrusted_key_fails() {
        let e = verify(MSG, SIG, "oath-output", &[OTHER.into()]);
        assert!(e.unwrap_err().to_string().contains("does not trust"));
        assert!(verify(MSG, SIG, "oath-output", &[]).is_err());
    }

    #[test]
    fn another_namespace_fails() {
        let e = verify(MSG, SIG, "file", &[KEY.into()]);
        assert!(e.unwrap_err().to_string().contains("namespace"));
    }

    #[test]
    fn public_key_lines() {
        let (blob, comment) = parse_public_key(KEY).unwrap();
        assert_eq!(blob.len(), 51);
        assert_eq!(comment, "test@oath");
        assert!(parse_public_key("ssh-rsa AAAAB3NzaC1yc2E x").is_err());
        assert!(parse_public_key("ssh-ed25519 !!").is_err());
    }
}
