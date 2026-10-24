//! Sigstore bundles for `cargo make pkgs`, for upstreams that sign only
//! that way (Python, Perl, Codex): a `hashedrekord` entry in Rekor (v1) and a
//! short-lived Fulcio certificate, checked against Sigstore's trusted root
//! pinned in `pkgs/keys/sigstore.json` and the identity pinned in
//! `pkg.toml`. Checked: the certificate chains to a pinned Fulcio CA and
//! names the identity and OIDC issuer; its key signed the artifact's
//! pinned SHA-256; Rekor's pinned key signed the log entry (its signed
//! entry timestamp) and, when the bundle carries one, the checkpoint whose
//! tree the entry's inclusion proof leads to; the entry records that
//! digest, signature and certificate, and its time falls inside the
//! certificate's validity. cosign's older bundle (a GitHub release's
//! `.sigstore`) has no inclusion proof, so its signed entry timestamp alone
//! vouches for the entry.

use anyhow::{bail, Context, Result};
use p256::ecdsa::signature::hazmat::PrehashVerifier;
use serde_json::{json, Value};
use sha2::{Digest, Sha256, Sha384};
use x509_cert::der::asn1::Utf8StringRef;
use x509_cert::der::{Decode, Encode};
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::SubjectAltName;
use x509_cert::spki::SubjectPublicKeyInfoOwned;
use x509_cert::Certificate;

use crate::openpgp::base64;

const OID_SAN: &str = "2.5.29.17";
/// Fulcio's OIDC issuer extension (v2, a DER UTF8String) and its v1 form.
const OID_ISSUER_V2: &str = "1.3.6.1.4.1.57264.1.8";
const OID_ISSUER_V1: &str = "1.3.6.1.4.1.57264.1.1";
const OID_ECDSA_SHA256: &str = "1.2.840.10045.4.3.2";
const OID_ECDSA_SHA384: &str = "1.2.840.10045.4.3.3";

/// Check `bundle` (a Sigstore bundle, JSON) for the artifact whose SHA-256
/// is `sha256` (hex), signed by `identity` (the certificate's email or
/// URI) through OIDC `issuer`. Returns the log entry's index.
pub fn verify(
    trusted_root: &[u8],
    bundle: &[u8],
    sha256: &str,
    identity: &str,
    issuer: &str,
) -> Result<String> {
    let root: Value = serde_json::from_slice(trusted_root).context("trusted root")?;
    let b: Value = serde_json::from_slice(bundle).context("bundle")?;
    let Parts { leaf_der, sig, signed_digest, log_id, entry } = parts(&b)?;
    let digest = hex(sha256)?;
    let leaf = Certificate::from_der(&leaf_der).context("certificate")?;

    // Who signed.
    let (who, via) = identity_of(&leaf)?;
    if who != identity || via != issuer {
        bail!("signed by {who} via {via}, not the pinned {identity} via {issuer}");
    }

    // The certificate comes from a pinned Fulcio CA.
    let not_before = leaf.tbs_certificate.validity.not_before.to_unix_duration().as_secs();
    let not_after = leaf.tbs_certificate.validity.not_after.to_unix_duration().as_secs();
    let tbs = leaf.tbs_certificate.to_der()?;
    let alg = leaf.signature_algorithm.oid.to_string();
    let leaf_sig = leaf.signature.raw_bytes();
    let mut chained = false;
    for ca in root["certificateAuthorities"].as_array().context("trusted root CAs")? {
        if !valid_at(&ca["validFor"], not_before) {
            continue;
        }
        let Some(first) = ca["certChain"]["certificates"][0]["rawBytes"].as_str() else {
            continue;
        };
        let ca_cert = Certificate::from_der(&base64(first)?).context("CA certificate")?;
        if ca_cert.tbs_certificate.subject != leaf.tbs_certificate.issuer {
            continue;
        }
        let key = ca_cert.tbs_certificate.subject_public_key_info.subject_public_key.raw_bytes();
        let prehash: Vec<u8> = match alg.as_str() {
            OID_ECDSA_SHA256 => Sha256::digest(&tbs).to_vec(),
            OID_ECDSA_SHA384 => Sha384::digest(&tbs).to_vec(),
            _ => bail!("certificate signature algorithm {alg} is not supported"),
        };
        if ecdsa_ok(key, &prehash, leaf_sig) {
            chained = true;
            break;
        }
    }
    if !chained {
        bail!("the certificate does not chain to a pinned Fulcio CA");
    }

    // The certificate's key signed the artifact.
    if signed_digest.is_some_and(|d| d != digest) {
        bail!("the bundle signs another digest");
    }
    let leaf_key = leaf.tbs_certificate.subject_public_key_info.subject_public_key.raw_bytes();
    if !ecdsa_ok(leaf_key, &digest, &sig) {
        bail!("BAD signature: the artifact is not what {identity} signed");
    }

    // Rekor logged it, inside the certificate's ten minutes.
    let log = root["tlogs"]
        .as_array()
        .context("trusted root tlogs")?
        .iter()
        .find(|t| t["logId"]["keyId"].as_str().and_then(|k| base64(k).ok()) == Some(log_id.clone()))
        .context("the entry's log is not in the trusted root")?;
    if log["publicKey"]["keyDetails"] != "PKIX_ECDSA_P256_SHA_256" {
        bail!("log {} is not supported", log["baseUrl"]);
    }
    let time: i64 = num(&entry["integratedTime"])?;
    if !valid_at(&log["publicKey"]["validFor"], time as u64) {
        bail!("the log's key was not valid at {time}");
    }
    if (time as u64) < not_before || (time as u64) > not_after {
        bail!("logged at {time}, outside the certificate's validity");
    }
    let spki_der = base64(log["publicKey"]["rawBytes"].as_str().context("log key")?)?;
    let spki = SubjectPublicKeyInfoOwned::from_der(&spki_der).context("log key")?;
    let body_b64 = entry["canonicalizedBody"].as_str().context("entry body")?;
    let index: i64 = num(&entry["logIndex"])?;
    // The signed entry timestamp is over this canonical JSON (sorted keys).
    let payload = json!({
        "body": body_b64,
        "integratedTime": time,
        "logID": log_id.iter().map(|x| format!("{x:02x}")).collect::<String>(),
        "logIndex": index,
    })
    .to_string();
    let set = base64(
        entry["inclusionPromise"]["signedEntryTimestamp"].as_str().context("entry has no SET")?,
    )?;
    if !ecdsa_ok(spki.subject_public_key.raw_bytes(), &Sha256::digest(payload.as_bytes()), &set) {
        bail!("BAD signed entry timestamp from {}", log["baseUrl"]);
    }
    let body_bytes = base64(body_b64)?;
    let proof = &entry["inclusionProof"];
    if !proof.is_null() {
        included(proof, &body_bytes, &log_id, spki.subject_public_key.raw_bytes())
            .with_context(|| format!("inclusion in {}", log["baseUrl"]))?;
    }
    let body: Value = serde_json::from_slice(&body_bytes).context("entry body")?;
    if body["kind"] != "hashedrekord" {
        bail!("entry kind {} is not supported", body["kind"]);
    }
    let spec = &body["spec"];
    if spec["data"]["hash"]["algorithm"] != "sha256" || spec["data"]["hash"]["value"] != sha256 {
        bail!("the log entry records another artifact");
    }
    if base64(spec["signature"]["content"].as_str().unwrap_or(""))? != sig {
        bail!("the log entry records another signature");
    }
    let pem = base64(spec["signature"]["publicKey"]["content"].as_str().unwrap_or(""))?;
    if pem_der(&String::from_utf8_lossy(&pem))? != leaf_der {
        bail!("the log entry records another certificate");
    }
    Ok(index.to_string())
}

/// What a bundle holds: the leaf certificate (DER), the artifact's
/// signature, the digest it says it signs, and its Rekor entry: the log's
/// ID and the entry in the bundle's own form (`integratedTime`,
/// `logIndex`, `inclusionPromise`, `canonicalizedBody`, `inclusionProof`).
struct Parts {
    leaf_der: Vec<u8>,
    sig: Vec<u8>,
    signed_digest: Option<Vec<u8>>,
    log_id: Vec<u8>,
    entry: Value,
}

/// A Sigstore bundle, or cosign's older one (`base64Signature`, `cert`
/// as base64 PEM, `rekorBundle`: the entry's payload and signed entry
/// timestamp, no inclusion proof), which GitHub releases still carry.
fn parts(b: &Value) -> Result<Parts> {
    if b["rekorBundle"].is_object() {
        let pem = base64(b["cert"].as_str().context("bundle has no certificate")?)?;
        let p = &b["rekorBundle"]["Payload"];
        return Ok(Parts {
            leaf_der: pem_der(&String::from_utf8_lossy(&pem))?,
            sig: base64(b["base64Signature"].as_str().context("bundle has no signature")?)?,
            signed_digest: None,
            log_id: hex(p["logID"].as_str().context("bundle has no tlog entry")?)?,
            entry: json!({
                "integratedTime": p["integratedTime"],
                "logIndex": p["logIndex"],
                "inclusionPromise": { "signedEntryTimestamp": b["rekorBundle"]["SignedEntryTimestamp"] },
                "canonicalizedBody": p["body"],
            }),
        });
    }
    let media = b["mediaType"].as_str().unwrap_or("");
    if !media.starts_with("application/vnd.dev.sigstore.bundle") {
        bail!("not a Sigstore bundle ({media:?})");
    }
    let vm = &b["verificationMaterial"];
    let leaf_b64 = vm["certificate"]["rawBytes"]
        .as_str()
        .or_else(|| vm["x509CertificateChain"]["certificates"][0]["rawBytes"].as_str())
        .context("bundle has no certificate")?;
    let ms = &b["messageSignature"];
    let entry = &vm["tlogEntries"][0];
    Ok(Parts {
        leaf_der: base64(leaf_b64)?,
        sig: base64(ms["signature"].as_str().context("bundle has no messageSignature")?)?,
        signed_digest: ms["messageDigest"]["digest"].as_str().map(base64).transpose()?,
        log_id: base64(entry["logId"]["keyId"].as_str().context("bundle has no tlog entry")?)?,
        entry: entry.clone(),
    })
}

/// The entry is in the tree its RFC 6962 proof leads to, and Rekor's key
/// signed the checkpoint for that tree.
fn included(proof: &Value, body: &[u8], log_id: &[u8], rekor_key: &[u8]) -> Result<()> {
    let index = num(&proof["logIndex"])? as u64;
    let size = num(&proof["treeSize"])? as u64;
    let root = base64(proof["rootHash"].as_str().context("proof root")?)?;
    let hashes = proof["hashes"]
        .as_array()
        .context("proof hashes")?
        .iter()
        .map(|h| base64(h.as_str().unwrap_or("")))
        .collect::<Result<Vec<_>>>()?;
    let node = |l: &[u8], r: &[u8]| {
        Sha256::new().chain_update([1]).chain_update(l).chain_update(r).finalize().to_vec()
    };
    // RFC 9162, 2.1.3.2.
    if index >= size {
        bail!("leaf {index} is outside a tree of {size}");
    }
    let (mut f, mut s) = (index, size - 1);
    let mut r = Sha256::new().chain_update([0]).chain_update(body).finalize().to_vec();
    for p in &hashes {
        if s == 0 {
            bail!("the proof is too long");
        }
        if f & 1 == 1 || f == s {
            r = node(p, &r);
            while f & 1 == 0 && f != 0 {
                f >>= 1;
                s >>= 1;
            }
        } else {
            r = node(&r, p);
        }
        f >>= 1;
        s >>= 1;
    }
    if s != 0 || r != root {
        bail!("the proof does not lead to the tree's root");
    }
    // The checkpoint is a signed note: its text, a blank line, then
    // `— <name> <base64(key hint ‖ signature)>`.
    let envelope = proof["checkpoint"]["envelope"].as_str().context("proof has no checkpoint")?;
    let (text, sigs) = envelope.split_once("\n\n").context("checkpoint without signatures")?;
    let text = format!("{text}\n");
    let mut lines = text.lines();
    let (_origin, tree, head) = (lines.next(), lines.next(), lines.next());
    if tree != Some(size.to_string().as_str()) || head.map(base64).transpose()? != Some(root) {
        bail!("the checkpoint is for another tree");
    }
    let signed = sigs.lines().filter_map(|l| l.rsplit(' ').next()).any(|b| {
        base64(b).is_ok_and(|raw| {
            raw.len() > 4
                && raw[..4] == log_id[..4]
                && ecdsa_ok(rekor_key, &Sha256::digest(text.as_bytes()), &raw[4..])
        })
    });
    if !signed {
        bail!("BAD checkpoint signature");
    }
    Ok(())
}

/// The certificate's subject (email or URI) and OIDC issuer.
fn identity_of(cert: &Certificate) -> Result<(String, String)> {
    let (mut who, mut via) = (String::new(), String::new());
    for ext in cert.tbs_certificate.extensions.as_deref().unwrap_or_default() {
        let value = ext.extn_value.as_bytes();
        match ext.extn_id.to_string().as_str() {
            OID_SAN => {
                for name in SubjectAltName::from_der(value)?.0 {
                    match name {
                        GeneralName::Rfc822Name(s) => who = s.to_string(),
                        GeneralName::UniformResourceIdentifier(s) => who = s.to_string(),
                        _ => {}
                    }
                }
            }
            OID_ISSUER_V2 => via = Utf8StringRef::from_der(value)?.to_string(),
            OID_ISSUER_V1 if via.is_empty() => via = String::from_utf8_lossy(value).into_owned(),
            _ => {}
        }
    }
    if who.is_empty() || via.is_empty() {
        bail!("the certificate names no identity or issuer");
    }
    Ok((who, via))
}

/// An ECDSA (P-256 or P-384) signature, DER, over a prehashed message.
fn ecdsa_ok(sec1_key: &[u8], prehash: &[u8], der_sig: &[u8]) -> bool {
    if let Ok(k) = p256::ecdsa::VerifyingKey::from_sec1_bytes(sec1_key) {
        return p256::ecdsa::Signature::from_der(der_sig)
            .is_ok_and(|s| k.verify_prehash(prehash, &s).is_ok());
    }
    if let Ok(k) = p384::ecdsa::VerifyingKey::from_sec1_bytes(sec1_key) {
        return p384::ecdsa::Signature::from_der(der_sig)
            .is_ok_and(|s| k.verify_prehash(prehash, &s).is_ok());
    }
    false
}

/// Whether `t` (Unix seconds) is inside a trusted-root `validFor`.
fn valid_at(range: &Value, t: u64) -> bool {
    let at = |k: &str| {
        range[k]
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.timestamp())
    };
    let t = t as i64;
    at("start").is_none_or(|s| t >= s) && at("end").is_none_or(|e| t <= e)
}

/// A bundle number, which may be a JSON string.
fn num(v: &Value) -> Result<i64> {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .with_context(|| format!("not a number: {v}"))
}

fn pem_der(pem: &str) -> Result<Vec<u8>> {
    let b64: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    base64(&b64)
}

fn hex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        bail!("bad hex {s:?}");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).with_context(|| format!("bad hex {s:?}")))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: &[u8] = include_bytes!("../../../pkgs/keys/sigstore.json");
    const PYTHON: &[u8] = include_bytes!("../testdata/sigstore/Python-3.15.0.tar.xz.sigstore");
    const PYTHON_SHA256: &str = "ba4bed1ba346b916890b76d9e320451420aa69f6408997d33c66482eeae3d575";
    // CPAN's bundle for the .tar.xz signs the .tar.gz.
    const PERL: &[u8] = include_bytes!("../testdata/sigstore/perl-5.44.0.tar.xz.sigstore.bundle");
    const PERL_XZ: &str = "505cf43912e9480495c344c70260452e32aa2a73c546a026b3f100053b23ce91";
    const PERL_GZ: &str = "3b855066b92491cb40e86affb1ca57d1a388aa43e51b91c7806a32c2f65f96c3";
    const GITHUB: &str = "https://github.com/login/oauth";
    // cosign's older bundle, from a GitHub release.
    const CODEX: &[u8] = include_bytes!(
        "../testdata/sigstore/codex-package-x86_64-unknown-linux-musl.tar.gz.sigstore"
    );
    const CODEX_SHA256: &str = "3ec55053109608d5038f7e50750f7d25f768dfd740fef071aa710e2559b63aac";
    const CODEX_WORKFLOW: &str =
        "https://github.com/openai/codex/.github/workflows/rust-release.yml@refs/tags/rust-v0.164.0";
    const ACTIONS: &str = "https://token.actions.githubusercontent.com";

    #[test]
    fn python_release_verifies() {
        verify(ROOT, PYTHON, PYTHON_SHA256, "hugo@python.org", GITHUB).unwrap();
    }

    #[test]
    fn cosign_bundle_verifies() {
        verify(ROOT, CODEX, CODEX_SHA256, CODEX_WORKFLOW, ACTIONS).unwrap();
        let e = verify(ROOT, CODEX, PYTHON_SHA256, CODEX_WORKFLOW, ACTIONS).unwrap_err();
        assert!(e.to_string().contains("BAD signature"), "{e}");
        let main = CODEX_WORKFLOW.replace("refs/tags/rust-v0.164.0", "refs/heads/main");
        let e = verify(ROOT, CODEX, CODEX_SHA256, &main, ACTIONS).unwrap_err();
        assert!(e.to_string().contains("not the pinned"), "{e}");
        let mut b: Value = serde_json::from_slice(CODEX).unwrap();
        let t = num(&b["rekorBundle"]["Payload"]["integratedTime"]).unwrap();
        b["rekorBundle"]["Payload"]["integratedTime"] = json!(t + 1);
        let e = verify(ROOT, b.to_string().as_bytes(), CODEX_SHA256, CODEX_WORKFLOW, ACTIONS)
            .unwrap_err();
        assert!(e.to_string().contains("BAD signed entry timestamp"), "{e}");
    }

    #[test]
    fn wrong_identity_digest_or_entry_fails() {
        let e = verify(ROOT, PYTHON, PYTHON_SHA256, "someone@python.org", GITHUB).unwrap_err();
        assert!(e.to_string().contains("signed by hugo@python.org"), "{e}");
        let e = verify(ROOT, PYTHON, PERL_XZ, "hugo@python.org", GITHUB).unwrap_err();
        assert!(e.to_string().contains("another digest"), "{e}");
        let e = verify(ROOT, PERL, PERL_XZ, "fawaka@gmail.com", GITHUB).unwrap_err();
        assert!(e.to_string().contains("another digest"), "{e}");
        verify(ROOT, PERL, PERL_GZ, "fawaka@gmail.com", GITHUB).unwrap();
        // A log entry whose time was moved fails Rekor's signature.
        let mut b: Value = serde_json::from_slice(PYTHON).unwrap();
        let t = num(&b["verificationMaterial"]["tlogEntries"][0]["integratedTime"]).unwrap();
        b["verificationMaterial"]["tlogEntries"][0]["integratedTime"] = json!((t + 1).to_string());
        let e = verify(ROOT, b.to_string().as_bytes(), PYTHON_SHA256, "hugo@python.org", GITHUB)
            .unwrap_err();
        assert!(e.to_string().contains("BAD signed entry timestamp"), "{e}");
        // A proof hash changed leads to another root.
        let mut b: Value = serde_json::from_slice(PYTHON).unwrap();
        b["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["hashes"][3] =
            json!("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=");
        let e = verify(ROOT, b.to_string().as_bytes(), PYTHON_SHA256, "hugo@python.org", GITHUB)
            .unwrap_err();
        assert!(format!("{e:#}").contains("does not lead to the tree's root"), "{e:#}");
        // A trusted root without the Fulcio CA does not chain.
        let mut r: Value = serde_json::from_slice(ROOT).unwrap();
        r["certificateAuthorities"] = json!([]);
        let e = verify(r.to_string().as_bytes(), PYTHON, PYTHON_SHA256, "hugo@python.org", GITHUB)
            .unwrap_err();
        assert!(e.to_string().contains("does not chain"), "{e}");
    }
}
