//! The consort (R87, docs/specs/consort.md): Oath machines that know each
//! other. Each member, a peer, has two keys of its own: an Ed25519 key it
//! signs with, and an X25519 key its links agree on (ed25519-dalek advises
//! against using one key for both). Membership is a log of signed entries
//! that every peer keeps and checks back to the first, so no machine holds
//! a consort key and any member can be lost.
//!
//! What `oath-bus` and `oath consort` share: keys, the log, records, the
//! beacon, the join code, and one call to the local socket.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use curve25519_dalek::MontgomeryPoint;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{now_rfc3339, write_json, Error, Result};

/// TCP for links, UDP for beacons: "OATH" on a phone's keypad.
pub const PORT: u16 = 6284;
/// Under the catalog root: the keys and the log, root's alone, outside
/// every generation (an undo never takes membership back).
pub const DIR: &str = "bus";
/// Under the catalog root: the local socket.
pub const SOCKET: &str = "run/bus.sock";

const ED25519: &str = "ssh-ed25519";

/// A peer's signing key: its `ssh-ed25519` blob in base64, the second
/// field of an `.pub` line.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PeerKey(pub String);

impl PeerKey {
    pub fn of(k: &VerifyingKey) -> Self {
        let mut blob = Vec::with_capacity(51);
        put_string(&mut blob, ED25519.as_bytes());
        put_string(&mut blob, k.as_bytes());
        Self(crate::ssh::base64(&blob))
    }

    /// From a whole `ssh-ed25519 AAAA… comment` line or its second field.
    pub fn parse(s: &str) -> Result<Self> {
        let field = s.split_whitespace().find(|f| f.starts_with("AAAA")).unwrap_or(s);
        let k = Self(field.to_string());
        k.verifying()?;
        Ok(k)
    }

    pub fn verifying(&self) -> Result<VerifyingKey> {
        let bad = || Error::Msg(format!("not an ssh-ed25519 key: {}", self.0));
        let blob = crate::sshsig::base64(&self.0).ok_or_else(bad)?;
        let mut r = &blob[..];
        if take_string(&mut r).ok_or_else(bad)? != ED25519.as_bytes() {
            return Err(bad());
        }
        let raw: [u8; 32] = take_string(&mut r).ok_or_else(bad)?.try_into().map_err(|_| bad())?;
        if !r.is_empty() {
            return Err(bad());
        }
        VerifyingKey::from_bytes(&raw).map_err(|_| bad())
    }

    /// `SHA256:…`, as `ssh-keygen -l` prints it.
    pub fn fingerprint(&self) -> String {
        let blob = crate::sshsig::base64(&self.0).unwrap_or_default();
        let b64 = crate::ssh::base64(&Sha256::digest(&blob));
        format!("SHA256:{}", b64.trim_end_matches('='))
    }

    /// An `.pub` line.
    pub fn line(&self, comment: &str) -> String {
        format!("{ED25519} {} {comment}", self.0)
    }
}

/// This machine's keys, made on first start in `<root>/bus/`.
pub struct Identity {
    sign: SigningKey,
    link: [u8; 32],
}

impl Identity {
    /// The keys in `dir` (`key`, `link`: 32 raw bytes each, 0600), made
    /// from the kernel's random numbers when missing.
    pub fn load_or_make(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        let sign = SigningKey::from_bytes(&secret(&dir.join("key"))?);
        let link = secret(&dir.join("link"))?;
        Ok(Self { sign, link })
    }

    pub fn key(&self) -> PeerKey {
        PeerKey::of(&self.sign.verifying_key())
    }

    /// The X25519 secret a link's handshake uses.
    pub fn link_secret(&self) -> &[u8; 32] {
        &self.link
    }

    /// The X25519 public key, in base64.
    pub fn link_key(&self) -> String {
        crate::ssh::base64(&MontgomeryPoint::mul_base_clamped(self.link).to_bytes())
    }

    fn sign(&self, domain: &str, body: &str) -> String {
        crate::ssh::base64(&self.sign.sign(&signed_bytes(domain, body)).to_bytes())
    }
}

fn secret(path: &Path) -> Result<[u8; 32]> {
    if let Ok(b) = fs::read(path) {
        return b
            .try_into()
            .map_err(|_| Error::Msg(format!("{}: not 32 bytes; remove it to make another", path.display())));
    }
    let mut b = [0u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    let tmp = path.with_extension("new");
    let mut f = fs::OpenOptions::new().create(true).truncate(true).write(true).mode(0o600).open(&tmp)?;
    f.write_all(&b)?;
    f.sync_all()?;
    fs::rename(&tmp, path)?;
    Ok(b)
}

fn signed_bytes(domain: &str, body: &str) -> Vec<u8> {
    format!("oath-consort/{domain}\n{body}").into_bytes()
}

/// What a signed body names as its signer.
pub trait By {
    fn by(&self) -> &PeerKey;
}

/// A body and the signature of its signer, over the exact bytes sent, so
/// no two programs need agree on how JSON is written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signed {
    pub body: String,
    pub sig: String,
}

impl Signed {
    pub fn make<T: Serialize>(id: &Identity, domain: &str, v: &T) -> Result<Self> {
        let body = serde_json::to_string(v)?;
        Ok(Self { sig: id.sign(domain, &body), body })
    }

    /// The body, when its signer signed it for `domain`.
    pub fn open<T: DeserializeOwned + By>(&self, domain: &str) -> Result<T> {
        let v: T = serde_json::from_str(&self.body)?;
        let bad = || Error::security(format!("a bad {domain} signature"), "nothing to do: it is dropped");
        let sig = crate::sshsig::base64(&self.sig).ok_or_else(bad)?;
        let sig = Signature::from_slice(&sig).map_err(|_| bad())?;
        v.by().verifying()?.verify(&signed_bytes(domain, &self.body), &sig).map_err(|_| bad())?;
        Ok(v)
    }

    pub fn hash(&self) -> String {
        hex(&Sha256::digest(self.body.as_bytes()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Op {
    /// The first entry: a machine makes its consort of one.
    Genesis,
    Admit,
    Remove,
}

/// One change to who is in the consort.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The consort's id: its first entry's hash. Empty in that entry.
    #[serde(default)]
    pub consort: String,
    pub op: Op,
    /// The peer it admits or removes.
    pub key: PeerKey,
    /// That peer's X25519 link key (genesis and admit).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub link: String,
    pub name: String,
    pub by: PeerKey,
    /// The entries it follows: the heads its signer had.
    #[serde(default)]
    pub parents: Vec<String>,
    pub at: String,
    /// The first entry's salt: no two consorts of one share an id.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub nonce: String,
}

impl By for Entry {
    fn by(&self) -> &PeerKey {
        &self.by
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Member {
    pub key: PeerKey,
    pub link: String,
    pub name: String,
    /// Who admitted it (itself for the first member) and when.
    pub by: PeerKey,
    pub at: String,
}

/// The membership log. Entries form a graph by their parents; every peer
/// replays them in one order (depth from the first entry, then hash), and
/// an entry counts only when its signer is a member at that point.
#[derive(Clone, Debug, Default)]
pub struct Log {
    entries: BTreeMap<String, (Signed, Entry)>,
}

impl Log {
    /// A consort of one: this machine.
    pub fn new(id: &Identity, name: &str) -> Result<Self> {
        let mut nonce = [0u8; 16];
        fs::File::open("/dev/urandom")?.read_exact(&mut nonce)?;
        let e = Entry {
            consort: String::new(),
            op: Op::Genesis,
            key: id.key(),
            link: id.link_key(),
            name: name.to_string(),
            by: id.key(),
            parents: Vec::new(),
            at: now_rfc3339(),
            nonce: hex(&nonce),
        };
        let mut log = Self::default();
        log.add(Signed::make(id, "entry", &e)?)?;
        Ok(log)
    }

    /// The consort's id: the first entry's hash.
    pub fn id(&self) -> String {
        self.entries.values().find(|(_, e)| e.op == Op::Genesis).map(|(s, _)| s.hash()).unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries no other entry follows.
    pub fn heads(&self) -> Vec<String> {
        let named: BTreeSet<&String> = self.entries.values().flat_map(|(_, e)| &e.parents).collect();
        self.entries.keys().filter(|h| !named.contains(h)).cloned().collect()
    }

    /// Every entry, in replay order.
    pub fn signed(&self) -> Vec<Signed> {
        self.order().into_iter().map(|h| self.entries[&h].0.clone()).collect()
    }

    /// Take one entry. `Ok(false)`: already here. An entry for another
    /// consort, a second first entry, or one whose parents are not here is
    /// refused.
    pub fn add(&mut self, s: Signed) -> Result<bool> {
        let h = s.hash();
        if self.entries.contains_key(&h) {
            return Ok(false);
        }
        let e: Entry = s.open("entry")?;
        if e.op == Op::Genesis {
            if !self.entries.is_empty() || !e.consort.is_empty() || !e.parents.is_empty() || e.key != e.by {
                return Err(Error::Msg("a first entry where one is not taken".into()));
            }
        } else if e.consort != self.id() {
            return Err(Error::Msg(format!("an entry for consort {}", short(&e.consort))));
        } else if e.parents.is_empty() || e.parents.iter().any(|p| !self.entries.contains_key(p)) {
            return Err(Error::Msg(format!("entry {} follows entries not here", short(&h))));
        }
        if !crate::seat::is_host_name(&e.name, 63) {
            return Err(Error::Msg(format!("entry {}: `{}` is not a host name", short(&h), e.name)));
        }
        self.entries.insert(h, (s, e));
        Ok(true)
    }

    /// Take what of `entries` belongs here, parents first whatever order
    /// they came in. Returns how many were new.
    pub fn merge(&mut self, entries: &[Signed]) -> Result<usize> {
        let mut left: Vec<&Signed> = entries.iter().filter(|s| !self.entries.contains_key(&s.hash())).collect();
        let mut added = 0;
        loop {
            let before = left.len();
            let mut err = None;
            left.retain(|s| match self.add((*s).clone()) {
                Ok(_) => {
                    added += 1;
                    false
                }
                Err(e) => {
                    err = Some(e);
                    true
                }
            });
            if left.is_empty() {
                return Ok(added);
            }
            if left.len() == before {
                return Err(err.unwrap_or_else(|| Error::Msg("entries that do not fit".into())));
            }
        }
    }

    /// Who is in it now.
    pub fn members(&self) -> BTreeMap<PeerKey, Member> {
        let mut m: BTreeMap<PeerKey, Member> = BTreeMap::new();
        for h in self.order() {
            let e = &self.entries[&h].1;
            let named = |m: &BTreeMap<PeerKey, Member>| m.values().any(|x| x.name == e.name);
            match e.op {
                Op::Genesis => {
                    m.insert(e.key.clone(), member(e));
                }
                Op::Admit if m.contains_key(&e.by) && !m.contains_key(&e.key) && !named(&m) => {
                    m.insert(e.key.clone(), member(e));
                }
                Op::Remove if m.contains_key(&e.by) => {
                    m.remove(&e.key);
                }
                _ => {}
            }
        }
        m
    }

    /// Sign and take a change, after the heads this log has.
    pub fn append(&mut self, id: &Identity, op: Op, key: &PeerKey, link: &str, name: &str) -> Result<Signed> {
        let members = self.members();
        if !members.contains_key(&id.key()) {
            return Err(Error::Msg("this machine is not a member".into()));
        }
        match op {
            Op::Genesis => return Err(Error::Msg("a consort has one first entry".into())),
            Op::Admit if members.contains_key(key) => {
                return Err(Error::Msg(format!("{name} is a member already")));
            }
            Op::Admit if members.values().any(|m| m.name == name) => {
                return Err(Error::hint(
                    format!("a member is named {name} already"),
                    "two machines need two host names: `sudo oath set host:local hostname=…` on the new one",
                ));
            }
            Op::Remove if !members.contains_key(key) => {
                return Err(Error::Msg(format!("{name} is not a member")));
            }
            _ => {}
        }
        let e = Entry {
            consort: self.id(),
            op,
            key: key.clone(),
            link: if op == Op::Admit { link.to_string() } else { String::new() },
            name: name.to_string(),
            by: id.key(),
            parents: self.heads(),
            at: now_rfc3339(),
            nonce: String::new(),
        };
        let s = Signed::make(id, "entry", &e)?;
        self.add(s.clone())?;
        Ok(s)
    }

    pub fn load(path: &Path) -> Result<Option<Self>> {
        let Ok(text) = fs::read_to_string(path) else {
            return Ok(None);
        };
        let signed: Vec<Signed> = serde_json::from_str(&text)?;
        let mut log = Self::default();
        log.merge(&signed)?;
        Ok(Some(log))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        write_json(path, &self.signed())
    }

    fn order(&self) -> Vec<String> {
        let mut depth: BTreeMap<&String, usize> = BTreeMap::new();
        fn d<'a>(log: &'a Log, h: &'a String, depth: &mut BTreeMap<&'a String, usize>) -> usize {
            if let Some(n) = depth.get(h) {
                return *n;
            }
            let n = log.entries[h].1.parents.iter().map(|p| d(log, p, depth) + 1).max().unwrap_or(0);
            depth.insert(h, n);
            n
        }
        let mut hs: Vec<(usize, &String)> = self.entries.keys().map(|h| (d(self, h, &mut depth), h)).collect();
        hs.sort();
        hs.into_iter().map(|(_, h)| h.clone()).collect()
    }
}

fn member(e: &Entry) -> Member {
    Member { key: e.key.clone(), link: e.link.clone(), name: e.name.clone(), by: e.by.clone(), at: e.at.clone() }
}

/// A fact a peer publishes about itself.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub by: PeerKey,
    pub topic: String,
    /// Higher replaces lower, per signer and topic.
    pub seq: u64,
    pub at: String,
    /// Who may read it. `consort` is every member.
    #[serde(default = "consort")]
    pub audience: String,
    pub data: Value,
}

fn consort() -> String {
    "consort".into()
}

impl By for Record {
    fn by(&self) -> &PeerKey {
        &self.by
    }
}

/// What a peer says on its LAN every few seconds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Beacon {
    pub v: u32,
    pub name: String,
    pub by: PeerKey,
    pub consort: String,
    pub members: usize,
    /// Its links' TCP port.
    pub port: u16,
    pub at: String,
}

impl By for Beacon {
    fn by(&self) -> &PeerKey {
        &self.by
    }
}

/// A machine's ask to join, which its keys sign: the consort and the link
/// key are bound to the signing key, so neither can be swapped on the way.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ask {
    pub consort: String,
    pub name: String,
    pub by: PeerKey,
    pub link: String,
    pub at: String,
}

impl By for Ask {
    fn by(&self) -> &PeerKey {
        &self.by
    }
}

/// Six digits both sides show for a join, from the consort, the joining
/// machine's keys: a swapped key on either side shows another code.
pub fn join_code(consort: &str, key: &PeerKey, link: &str) -> String {
    let d = Sha256::digest(format!("oath-consort/join\n{consort}\n{}\n{link}", key.0).as_bytes());
    let n = u32::from_be_bytes([d[0], d[1], d[2], d[3]]) % 1_000_000;
    format!("{n:06}")
}

/// Seconds since `at` (RFC 3339); negative when it is ahead of this clock.
pub fn age_secs(at: &str) -> Option<i64> {
    let t = chrono::DateTime::parse_from_rfc3339(at).ok()?;
    Some(chrono::Utc::now().timestamp() - t.timestamp())
}

/// A Unix time in RFC 3339, UTC, as [`now_rfc3339`] writes it.
pub fn rfc3339_of_secs(t: i64) -> Option<String> {
    let d = chrono::DateTime::<chrono::Utc>::from_timestamp(t, 0)?;
    Some(d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

/// Standard base64, as keys and signatures are written.
pub fn b64(b: &[u8]) -> String {
    crate::ssh::base64(b)
}

pub fn short(id: &str) -> &str {
    &id[..id.len().min(12)]
}

pub fn socket(root: &Path) -> PathBuf {
    root.join(SOCKET)
}

/// One request to the local `oath-bus`, one answer. An answer with
/// `ok: false` is an `Err` with its `error` (and `hint`).
pub fn call(root: &Path, req: &Value) -> Result<Value> {
    let path = socket(root);
    let mut s = UnixStream::connect(&path).map_err(|e| {
        Error::hint(format!("{}: {e}", path.display()), "oath get svc:oath-bus (is it running?)")
    })?;
    s.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut line = serde_json::to_vec(req)?;
    line.push(b'\n');
    crate::write_to_peer(&mut s, &line)?;
    let mut answer = String::new();
    BufReader::new(&s).read_line(&mut answer)?;
    let v: Value = serde_json::from_str(answer.trim())
        .map_err(|e| Error::Msg(format!("oath-bus answered {answer:?}: {e}")))?;
    if v.get("ok") == Some(&Value::Bool(true)) {
        return Ok(v);
    }
    let why = v.get("error").and_then(Value::as_str).unwrap_or("refused").to_string();
    match v.get("hint").and_then(Value::as_str) {
        Some(h) => Err(Error::hint(why, h)),
        None => Err(Error::Msg(why)),
    }
}

fn put_string(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(&(s.len() as u32).to_be_bytes());
    out.extend_from_slice(s);
}

fn take_string<'a>(r: &mut &'a [u8]) -> Option<&'a [u8]> {
    let n = u32::from_be_bytes(r.get(..4)?.try_into().ok()?) as usize;
    let s = r.get(4..4 + n)?;
    *r = &r[4 + n..];
    Some(s)
}

fn hex(bytes: &[u8]) -> String {
    const T: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(T[(b >> 4) as usize] as char);
        s.push(T[(b & 0xf) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(dir: &tempfile::TempDir, name: &str) -> Identity {
        Identity::load_or_make(&dir.path().join(name)).unwrap()
    }

    #[test]
    fn keys_are_made_once_and_kept() {
        let d = tempfile::tempdir().unwrap();
        let a = ident(&d, "a");
        let again = ident(&d, "a");
        assert_eq!(a.key(), again.key());
        assert_eq!(a.link_key(), again.link_key());
        assert_ne!(a.key(), ident(&d, "b").key());
        let mode = fs::metadata(d.path().join("a/key")).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_key_reads_back_from_its_pub_line() {
        let d = tempfile::tempdir().unwrap();
        let k = ident(&d, "a").key();
        assert_eq!(PeerKey::parse(&k.line("oath-bus@aulos")).unwrap(), k);
        assert!(k.fingerprint().starts_with("SHA256:"));
        assert!(PeerKey::parse("ssh-ed25519 AAAAnope").is_err());
    }

    #[test]
    fn a_signature_covers_its_domain_and_its_bytes() {
        let d = tempfile::tempdir().unwrap();
        let a = ident(&d, "a");
        let r = Record {
            by: a.key(),
            topic: "status".into(),
            seq: 1,
            at: now_rfc3339(),
            audience: consort(),
            data: serde_json::json!({ "x": 1 }),
        };
        let s = Signed::make(&a, "record", &r).unwrap();
        assert_eq!(s.open::<Record>("record").unwrap(), r);
        assert!(s.open::<Record>("beacon").is_err());
        let tampered = Signed { body: s.body.replace("\"x\":1", "\"x\":2"), sig: s.sig.clone() };
        assert!(tampered.open::<Record>("record").is_err());
        // Another key cannot sign for a.
        let b = ident(&d, "b");
        let forged = Signed { body: s.body.clone(), sig: b.sign("record", &s.body) };
        assert!(forged.open::<Record>("record").is_err());
    }

    #[test]
    fn a_machine_starts_as_a_consort_of_one() {
        let d = tempfile::tempdir().unwrap();
        let a = ident(&d, "a");
        let log = Log::new(&a, "aulos").unwrap();
        let m = log.members();
        assert_eq!(m.len(), 1);
        assert_eq!(m[&a.key()].name, "aulos");
        assert_eq!(log.id().len(), 64);
        assert_ne!(log.id(), Log::new(&a, "aulos").unwrap().id(), "the salt");
    }

    #[test]
    fn any_member_admits_and_every_copy_agrees() {
        let d = tempfile::tempdir().unwrap();
        let (a, b, c) = (ident(&d, "a"), ident(&d, "b"), ident(&d, "c"));
        let mut log = Log::new(&a, "aulos").unwrap();
        log.append(&a, Op::Admit, &b.key(), &b.link_key(), "buccina").unwrap();
        // b, a member now, admits c.
        log.append(&b, Op::Admit, &c.key(), &c.link_key(), "canto").unwrap();
        assert_eq!(log.members().len(), 3);
        // A copy that took the entries in any order replays the same.
        let mut entries = log.signed();
        entries.reverse();
        let mut copy = Log::default();
        assert_eq!(copy.merge(&entries).unwrap(), 3);
        assert_eq!(copy.members(), log.members());
        assert_eq!(copy.id(), log.id());
    }

    #[test]
    fn only_a_member_may_admit() {
        let d = tempfile::tempdir().unwrap();
        let (a, b, c) = (ident(&d, "a"), ident(&d, "b"), ident(&d, "c"));
        let mut log = Log::new(&a, "aulos").unwrap();
        assert!(log.append(&b, Op::Admit, &c.key(), &c.link_key(), "canto").is_err());
        // An entry b signs by hand is kept but counts for nothing.
        let e = Entry {
            consort: log.id(),
            op: Op::Admit,
            key: c.key(),
            link: c.link_key(),
            name: "canto".into(),
            by: b.key(),
            parents: log.heads(),
            at: now_rfc3339(),
            nonce: String::new(),
        };
        log.add(Signed::make(&b, "entry", &e).unwrap()).unwrap();
        assert_eq!(log.members().len(), 1);
    }

    #[test]
    fn a_removed_member_signs_nothing_that_counts() {
        let d = tempfile::tempdir().unwrap();
        let (a, b, c) = (ident(&d, "a"), ident(&d, "b"), ident(&d, "c"));
        let mut log = Log::new(&a, "aulos").unwrap();
        log.append(&a, Op::Admit, &b.key(), &b.link_key(), "buccina").unwrap();
        log.append(&a, Op::Remove, &b.key(), "", "buccina").unwrap();
        assert!(log.append(&b, Op::Admit, &c.key(), &c.link_key(), "canto").is_err());
        assert_eq!(log.members().len(), 1);
        // A later admission, at the owner's word, brings it back.
        log.append(&a, Op::Admit, &b.key(), &b.link_key(), "buccina").unwrap();
        assert_eq!(log.members().len(), 2);
    }

    #[test]
    fn two_names_never_share_a_member() {
        let d = tempfile::tempdir().unwrap();
        let (a, b) = (ident(&d, "a"), ident(&d, "b"));
        let mut log = Log::new(&a, "aulos").unwrap();
        assert!(log.append(&a, Op::Admit, &b.key(), &b.link_key(), "aulos").is_err());
    }

    #[test]
    fn an_entry_for_another_consort_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let (a, b, c) = (ident(&d, "a"), ident(&d, "b"), ident(&d, "c"));
        let mut mine = Log::new(&a, "aulos").unwrap();
        let mut theirs = Log::new(&b, "buccina").unwrap();
        let s = theirs.append(&b, Op::Admit, &c.key(), &c.link_key(), "canto").unwrap();
        assert!(mine.add(s).is_err());
        assert!(mine.merge(&theirs.signed()).is_err());
        assert_eq!(mine.members().len(), 1);
    }

    #[test]
    fn a_log_saves_and_loads() {
        let d = tempfile::tempdir().unwrap();
        let (a, b) = (ident(&d, "a"), ident(&d, "b"));
        let mut log = Log::new(&a, "aulos").unwrap();
        log.append(&a, Op::Admit, &b.key(), &b.link_key(), "buccina").unwrap();
        let path = d.path().join("log.json");
        log.save(&path).unwrap();
        let back = Log::load(&path).unwrap().unwrap();
        assert_eq!(back.members(), log.members());
        assert_eq!(back.heads(), log.heads());
    }

    #[test]
    fn the_join_code_turns_on_every_key() {
        let d = tempfile::tempdir().unwrap();
        let (a, b) = (ident(&d, "a"), ident(&d, "b"));
        let code = join_code("c0ffee", &a.key(), &a.link_key());
        assert_eq!(code.len(), 6);
        assert_eq!(code, join_code("c0ffee", &a.key(), &a.link_key()));
        assert_ne!(code, join_code("c0ffef", &a.key(), &a.link_key()));
        assert_ne!(code, join_code("c0ffee", &b.key(), &a.link_key()));
        assert_ne!(code, join_code("c0ffee", &a.key(), &b.link_key()));
    }
}
