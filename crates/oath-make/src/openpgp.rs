//! Upstream signature checks for `cargo make pkgs`: detached OpenPGP v4
//! signatures (RFC 9580) by RSA, DSA or Ed25519 keys over SHA-1 or SHA-2,
//! which is what upstream release signatures use, and cleartext-signed
//! lists (a Debian `.dsc`). The keys come from files pinned
//! in `pkgs/keys/`; every key in such a file is trusted as it stands, so
//! there are no expiry, revocation or subkey-binding checks.

use std::io::Read;

use anyhow::{bail, Context, Result};
use dsa::signature::hazmat::PrehashVerifier;
use ed25519_dalek::Verifier;
use rsa::{Pkcs1v15Sign, RsaPublicKey};
use sha1::Sha1;
use sha2::digest::DynDigest;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512};

const TAG_SIGNATURE: u8 = 2;
/// Signature types: a binary document, and canonical text (cleartext).
const BINARY: u8 = 0x00;
const TEXT: u8 = 0x01;
const TAG_PUBLIC_KEY: u8 = 6;
const TAG_PUBLIC_SUBKEY: u8 = 14;

struct Key {
    fingerprint: String,
    material: Material,
}

enum Material {
    Rsa(RsaPublicKey),
    Dsa(dsa::VerifyingKey),
    Ed25519(ed25519_dalek::VerifyingKey),
}

/// Ed25519's curve OID in a v4 EdDSA key (1.3.6.1.4.1.11591.15.1).
const ED25519_OID: &[u8] = &[0x2b, 0x06, 0x01, 0x04, 0x01, 0xda, 0x47, 0x0f, 0x01];

struct Signature {
    hash: u8,
    /// Version through the hashed subpackets: what is hashed after the data.
    hashed: Vec<u8>,
    left16: [u8; 2],
    /// RSA: `[s]`; DSA and EdDSA: `[r, s]`.
    mpis: Vec<Vec<u8>>,
    /// Issuer fingerprint or key ID, for messages.
    issuer: String,
}

/// Check that `sig` (armored or binary) is a signature over everything
/// `data` yields by a key in `keys` (armored or binary). Returns the
/// signing key's fingerprint.
pub fn verify(keys: &[u8], sig: &[u8], data: impl Read) -> Result<String> {
    verify_class(keys, sig, BINARY, data)
}

/// Whether `data` is a cleartext-signed message (`-----BEGIN PGP SIGNED
/// MESSAGE-----`), which `verify_cleartext` checks.
pub fn is_cleartext(data: &[u8]) -> bool {
    data.trim_ascii_start().starts_with(b"-----BEGIN PGP SIGNED MESSAGE-----")
}

/// Check a cleartext-signed message (RFC 9580 section 7), as Debian's
/// `.dsc` files are: a text signature (type 0x01) over the message with
/// its dash-escapes undone, each line's trailing spaces and tabs dropped
/// and its lines joined by CR LF. Returns the signing key's fingerprint
/// and the signed text, lines joined by LF.
pub fn verify_cleartext(keys: &[u8], msg: &[u8]) -> Result<(String, String)> {
    let text = std::str::from_utf8(msg).context("a cleartext-signed message is not UTF-8")?;
    let mut lines = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l));
    if lines.by_ref().find(|l| !l.trim().is_empty()) != Some("-----BEGIN PGP SIGNED MESSAGE-----")
    {
        bail!("not a cleartext-signed message");
    }
    // Hash: headers end at the first blank line.
    lines.by_ref().find(|l| l.is_empty());
    let mut body = Vec::new();
    let mut armor = None;
    for l in lines.by_ref() {
        if l == "-----BEGIN PGP SIGNATURE-----" {
            armor = Some(l);
            break;
        }
        body.push(l.strip_prefix("- ").unwrap_or(l).trim_end_matches([' ', '\t']));
    }
    let Some(begin) = armor else { bail!("the cleartext message has no signature") };
    let sig = std::iter::once(begin).chain(lines).collect::<Vec<_>>().join("\n");
    let who = verify_class(keys, sig.as_bytes(), TEXT, body.join("\r\n").as_bytes())?;
    Ok((who, body.join("\n")))
}

fn verify_class(keys: &[u8], sig: &[u8], class: u8, mut data: impl Read) -> Result<String> {
    let keys = parse_keys(&dearmor(keys)?)?;
    if keys.is_empty() {
        bail!("the key file holds no v4 RSA, DSA or Ed25519 key");
    }
    let sigs = parse_signatures(&dearmor(sig)?, class)?;
    if sigs.is_empty() {
        let what = if class == TEXT { "text" } else { "binary document" };
        bail!("no v4 RSA, DSA or EdDSA signature over a {what}");
    }
    let mut hashers = sigs.iter().map(|s| hasher(s.hash)).collect::<Result<Vec<_>>>()?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = data.read(&mut buf)?;
        if n == 0 {
            break;
        }
        for h in &mut hashers {
            h.update(&buf[..n]);
        }
    }
    for (s, mut h) in sigs.iter().zip(hashers) {
        h.update(&s.hashed);
        h.update(&[4, 0xff]);
        h.update(&(s.hashed.len() as u32).to_be_bytes());
        let digest = h.finalize();
        if digest[..2] != s.left16 {
            continue;
        }
        for k in &keys {
            if k.verifies(s, &digest)? {
                return Ok(k.fingerprint.clone());
            }
        }
    }
    let issuers: Vec<&str> = sigs
        .iter()
        .map(|s| if s.issuer.is_empty() { "an unnamed key" } else { s.issuer.as_str() })
        .collect();
    let known = sigs
        .iter()
        .any(|s| keys.iter().any(|k| !s.issuer.is_empty() && k.fingerprint.ends_with(&s.issuer)));
    if known {
        bail!("BAD signature by {}: the data is not what it signed", issuers.join(", "));
    }
    bail!("signed by {}, which is not a pinned key", issuers.join(", "))
}

impl Key {
    fn verifies(&self, s: &Signature, digest: &[u8]) -> Result<bool> {
        Ok(match (&self.material, s.mpis.as_slice()) {
            (Material::Rsa(k), [sig]) => {
                // The MPI drops leading zeros; PKCS #1 wants the modulus length.
                let size = rsa::traits::PublicKeyParts::size(k);
                if sig.len() > size {
                    return Ok(false);
                }
                let mut padded = vec![0u8; size - sig.len()];
                padded.extend_from_slice(sig);
                k.verify(pkcs1(s.hash)?, digest, &padded).is_ok()
            }
            (Material::Dsa(k), [r, sv]) => {
                let (r, sv) = (dsa::BigUint::from_bytes_be(r), dsa::BigUint::from_bytes_be(sv));
                if r == dsa::BigUint::default() || sv == dsa::BigUint::default() {
                    return Ok(false);
                }
                match dsa::Signature::from_components(r, sv) {
                    Ok(sig) => k.verify_prehash(digest, &sig).is_ok(),
                    Err(_) => false,
                }
            }
            // EdDSA signs the digest itself; r and s are 32-byte MPIs that
            // dropped their leading zeros.
            (Material::Ed25519(k), [r, sv]) => {
                if r.len() > 32 || sv.len() > 32 {
                    return Ok(false);
                }
                let mut rs = [0u8; 64];
                rs[32 - r.len()..32].copy_from_slice(r);
                rs[64 - sv.len()..].copy_from_slice(sv);
                k.verify(digest, &ed25519_dalek::Signature::from_bytes(&rs)).is_ok()
            }
            _ => false,
        })
    }
}

fn hasher(algo: u8) -> Result<Box<dyn DynDigest>> {
    Ok(match algo {
        2 => Box::new(Sha1::new()),
        8 => Box::new(Sha256::new()),
        9 => Box::new(Sha384::new()),
        10 => Box::new(Sha512::new()),
        11 => Box::new(Sha224::new()),
        _ => bail!("hash algorithm {algo} is not supported"),
    })
}

fn pkcs1(algo: u8) -> Result<Pkcs1v15Sign> {
    Ok(match algo {
        2 => Pkcs1v15Sign::new::<Sha1>(),
        8 => Pkcs1v15Sign::new::<Sha256>(),
        9 => Pkcs1v15Sign::new::<Sha384>(),
        10 => Pkcs1v15Sign::new::<Sha512>(),
        11 => Pkcs1v15Sign::new::<Sha224>(),
        _ => bail!("hash algorithm {algo} is not supported"),
    })
}

fn parse_keys(data: &[u8]) -> Result<Vec<Key>> {
    let mut out = Vec::new();
    for (tag, body) in packets(data)? {
        if tag == TAG_PUBLIC_KEY || tag == TAG_PUBLIC_SUBKEY {
            if let Some(k) = key(body)? {
                out.push(k);
            }
        }
    }
    Ok(out)
}

/// A v4 public key or subkey packet; `None` for other versions and for
/// algorithms that cannot sign here (Elgamal, ECDH, curves but Ed25519).
fn key(body: &[u8]) -> Result<Option<Key>> {
    let mut d = body;
    if take(&mut d, 1)?[0] != 4 {
        return Ok(None);
    }
    take(&mut d, 4)?; // creation time
    let material = match take(&mut d, 1)?[0] {
        1 | 3 => {
            let n = rsa::BigUint::from_bytes_be(mpi(&mut d)?);
            let e = rsa::BigUint::from_bytes_be(mpi(&mut d)?);
            Material::Rsa(RsaPublicKey::new_with_max_size(n, e, 16384).context("RSA key")?)
        }
        17 => {
            let mut next =
                || -> Result<dsa::BigUint> { Ok(dsa::BigUint::from_bytes_be(mpi(&mut d)?)) };
            let (p, q, g, y) = (next()?, next()?, next()?, next()?);
            let c = dsa::Components::from_components(p, q, g)
                .map_err(|_| anyhow::anyhow!("bad DSA key"))?;
            Material::Dsa(
                dsa::VerifyingKey::from_components(c, y)
                    .map_err(|_| anyhow::anyhow!("bad DSA key"))?,
            )
        }
        22 => {
            let oid_len = take(&mut d, 1)?[0] as usize;
            if take(&mut d, oid_len)? != ED25519_OID {
                return Ok(None);
            }
            // The point: 0x40, then the 32-byte public key.
            let point = mpi(&mut d)?;
            let Some((&0x40, pk)) = point.split_first() else { return Ok(None) };
            let pk: [u8; 32] = pk.try_into().context("Ed25519 key length")?;
            Material::Ed25519(ed25519_dalek::VerifyingKey::from_bytes(&pk).context("Ed25519 key")?)
        }
        _ => return Ok(None),
    };
    // v4 fingerprint: SHA-1 of 0x99, the two-octet body length, the body.
    let mut h = Sha1::new();
    Digest::update(&mut h, [0x99]);
    Digest::update(&mut h, (body.len() as u16).to_be_bytes());
    Digest::update(&mut h, body);
    Ok(Some(Key { fingerprint: hex(&h.finalize()), material }))
}

fn parse_signatures(data: &[u8], class: u8) -> Result<Vec<Signature>> {
    let mut out = Vec::new();
    for (tag, body) in packets(data)? {
        if tag == TAG_SIGNATURE {
            if let Some(s) = signature(body, class)? {
                out.push(s);
            }
        }
    }
    Ok(out)
}

/// A v4 signature of type `class` (a binary document or text) by RSA,
/// DSA or EdDSA.
fn signature(body: &[u8], class: u8) -> Result<Option<Signature>> {
    let mut d = body;
    let head = take(&mut d, 4)?;
    let (version, kind, algo, hash) = (head[0], head[1], head[2], head[3]);
    if version != 4 || kind != class {
        return Ok(None);
    }
    let hlen = u16::from_be_bytes(take(&mut d, 2)?.try_into()?) as usize;
    let hashed_sub = take(&mut d, hlen)?;
    let hashed = body[..6 + hlen].to_vec();
    let ulen = u16::from_be_bytes(take(&mut d, 2)?.try_into()?) as usize;
    let unhashed_sub = take(&mut d, ulen)?;
    let left16 = take(&mut d, 2)?.try_into()?;
    let mpis = match algo {
        1 | 3 => vec![mpi(&mut d)?.to_vec()],
        17 | 22 => vec![mpi(&mut d)?.to_vec(), mpi(&mut d)?.to_vec()],
        _ => return Ok(None),
    };
    let mut issuer = issuer_of(hashed_sub)?;
    if issuer.is_empty() {
        issuer = issuer_of(unhashed_sub)?;
    }
    Ok(Some(Signature { hash, hashed, left16, mpis, issuer }))
}

/// The issuer fingerprint (subpacket 33) or, failing that, key ID (16).
fn issuer_of(mut sub: &[u8]) -> Result<String> {
    let mut id = String::new();
    while !sub.is_empty() {
        let first = take(&mut sub, 1)?[0] as usize;
        let len = match first {
            0..=191 => first,
            192..=254 => ((first - 192) << 8) + take(&mut sub, 1)?[0] as usize + 192,
            _ => u32::from_be_bytes(take(&mut sub, 4)?.try_into()?) as usize,
        };
        let packet = take(&mut sub, len)?;
        let Some((&kind, value)) = packet.split_first() else { continue };
        match kind & 0x7f {
            33 if value.len() == 21 => return Ok(hex(&value[1..])),
            16 if value.len() == 8 => id = hex(value),
            _ => {}
        }
    }
    Ok(id)
}

/// `(tag, body)` of every packet in `data`.
fn packets(mut data: &[u8]) -> Result<Vec<(u8, &[u8])>> {
    let mut out = Vec::new();
    while !data.is_empty() {
        let h = take(&mut data, 1)?[0];
        if h & 0x80 == 0 {
            bail!("not OpenPGP data");
        }
        let (tag, len) = if h & 0x40 != 0 {
            let first = take(&mut data, 1)?[0] as usize;
            let len = match first {
                0..=191 => first,
                192..=223 => ((first - 192) << 8) + take(&mut data, 1)?[0] as usize + 192,
                255 => u32::from_be_bytes(take(&mut data, 4)?.try_into()?) as usize,
                _ => bail!("partial body length in a key or signature"),
            };
            (h & 0x3f, len)
        } else {
            let len = match h & 3 {
                0 => take(&mut data, 1)?[0] as usize,
                1 => u16::from_be_bytes(take(&mut data, 2)?.try_into()?) as usize,
                2 => u32::from_be_bytes(take(&mut data, 4)?.try_into()?) as usize,
                _ => bail!("indeterminate packet length"),
            };
            ((h >> 2) & 0x0f, len)
        };
        out.push((tag, take(&mut data, len)?));
    }
    Ok(out)
}

fn take<'a>(d: &mut &'a [u8], n: usize) -> Result<&'a [u8]> {
    if d.len() < n {
        bail!("truncated OpenPGP data");
    }
    let (a, b) = d.split_at(n);
    *d = b;
    Ok(a)
}

fn mpi<'a>(d: &mut &'a [u8]) -> Result<&'a [u8]> {
    let bits = u16::from_be_bytes(take(d, 2)?.try_into()?) as usize;
    take(d, bits.div_ceil(8))
}

/// The binary packets of an armored signature or public key block, or
/// `data` itself when it is not armored. A clearsigned message is not
/// accepted (`verify_cleartext` takes those).
fn dearmor(data: &[u8]) -> Result<Vec<u8>> {
    let Ok(text) = std::str::from_utf8(data) else { return Ok(data.to_vec()) };
    if !text.trim_start().starts_with("-----BEGIN PGP ") {
        return Ok(data.to_vec());
    }
    let mut out = Vec::new();
    let mut lines = text.lines().map(str::trim);
    while let Some(begin) = lines.by_ref().find(|l| l.starts_with("-----BEGIN PGP ")) {
        if begin != "-----BEGIN PGP SIGNATURE-----"
            && begin != "-----BEGIN PGP PUBLIC KEY BLOCK-----"
        {
            bail!("unsupported armor: {begin}");
        }
        // Armor headers (Comment:, Version:) end at the first blank line.
        lines.by_ref().find(|l| l.is_empty());
        let mut b64 = String::new();
        for l in lines.by_ref() {
            if l.starts_with("-----END PGP ") {
                break;
            }
            // `=` starts the optional CRC-24 line, never base64 data.
            if !l.starts_with('=') {
                b64.push_str(l);
            }
        }
        out.extend(base64(&b64)?);
    }
    Ok(out)
}

pub(crate) fn base64(s: &str) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => bail!("bad base64 in armor"),
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Made with gpg: an RSA key that signs with a subkey (SHA-512, binary),
    // a DSA-1024 key (SHA-1, armored), as old upstream keys do, and an
    // Ed25519 key (SHA-512, binary), as newer ones do.
    const DATA: &[u8] = include_bytes!("../testdata/openpgp/data");
    const RSA: &[u8] = include_bytes!("../testdata/openpgp/rsa.asc");
    const RSA_SIG: &[u8] = include_bytes!("../testdata/openpgp/data.rsa.sig");
    const DSA: &[u8] = include_bytes!("../testdata/openpgp/dsa.asc");
    const DSA_SIG: &[u8] = include_bytes!("../testdata/openpgp/data.dsa.asc");
    const ED: &[u8] = include_bytes!("../testdata/openpgp/ed25519.asc");
    const ED_SIG: &[u8] = include_bytes!("../testdata/openpgp/data.ed25519.sig");

    #[test]
    fn good_signatures() {
        assert_eq!(verify(RSA, RSA_SIG, DATA).unwrap(), "4BAF89D1F684C7675AEE643955E6AB4C380FE608");
        assert_eq!(verify(DSA, DSA_SIG, DATA).unwrap(), "6C56805587FE2A86DF6DE65DE0761AFC7AA1EABA");
        assert_eq!(verify(ED, ED_SIG, DATA).unwrap(), "6B0B2FCC06C18BB198DF083821A6B0B41F1C539B");
    }

    #[test]
    fn bad_data_and_unpinned_signer() {
        let mut other = DATA.to_vec();
        other[0] ^= 1;
        for (key, sig) in [(RSA, RSA_SIG), (DSA, DSA_SIG), (ED, ED_SIG)] {
            let e = verify(key, sig, &other[..]).unwrap_err().to_string();
            assert!(e.starts_with("BAD signature"), "{e}");
        }
        let e = verify(DSA, RSA_SIG, DATA).unwrap_err().to_string();
        assert!(
            e.contains("4BAF89D1F684C7675AEE643955E6AB4C380FE608")
                && e.contains("not a pinned key"),
            "{e}"
        );
        assert!(verify(RSA, DATA, DATA).is_err());
    }

    // Valve's .dsc for the Steam launcher, signed by its Steam for Linux key.
    const DSC: &[u8] = include_bytes!("../testdata/openpgp/steam_1.0.0.87.dsc");
    const VALVE: &[u8] = include_bytes!("../../../pkgs/keys/valve.asc");

    #[test]
    fn cleartext_signatures() {
        assert!(is_cleartext(DSC) && !is_cleartext(DATA));
        let (who, text) = verify_cleartext(VALVE, DSC).unwrap();
        assert_eq!(who, "DB23C56BF0F377FD02970A2A84706F31879D59CB");
        assert!(text.starts_with("Format: 3.0 (native)\n"), "{text}");
        assert!(text.contains(
            " 649375d2f9377f8009aaf3e2ff09978041eb9114897d9c5a3886f1d82e27ba1f 20409598 steam_1.0.0.87.tar.gz"
        ));
        let bad = String::from_utf8_lossy(DSC).replace("1.0.0.87.tar.gz", "1.0.0.88.tar.gz");
        let e = verify_cleartext(VALVE, bad.as_bytes()).unwrap_err().to_string();
        assert!(e.starts_with("BAD signature"), "{e}");
        assert!(verify_cleartext(RSA, DSC).unwrap_err().to_string().contains("not a pinned key"));
        // A detached binary signature is not a text signature.
        assert!(verify(VALVE, DSC, DATA).is_err());
    }

    #[test]
    fn repo_keys_parse() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../pkgs/keys");
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.extension().is_none_or(|x| x != "asc") {
                continue;
            }
            let keys = parse_keys(&dearmor(&std::fs::read(&p).unwrap()).unwrap()).unwrap();
            assert!(!keys.is_empty(), "{}", p.display());
        }
    }

    #[test]
    fn base64_and_armor() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("aGVsbG8hIQ==").unwrap(), b"hello!!");
        let armored = "-----BEGIN PGP SIGNATURE-----\nComment: x\n\naGVs\nbG8=\n=AbCd\n-----END PGP SIGNATURE-----\n";
        assert_eq!(dearmor(armored.as_bytes()).unwrap(), b"hello");
        assert_eq!(dearmor(b"\x88\x01").unwrap(), b"\x88\x01");
        assert!(dearmor(b"-----BEGIN PGP SIGNED MESSAGE-----\n").is_err());
    }
}
