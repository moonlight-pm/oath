//! `oath-bus` (R87, docs/specs/consort.md): this machine's place in its
//! consort. It keeps the membership log, says on the LAN who it is,
//! links to every other member over Noise, publishes facts about this
//! machine and keeps the others', writes them as `peer:*` objects, and
//! answers `oath consort` and a desk's view on `/oath/run/bus.sock`.
//!
//! Every machine starts as a consort of one. Facts flow only between
//! members; a machine that is not one may only ask to join.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use oath_core::consort::{self, join_code, Ask, Beacon, Identity, Log, Op, PeerKey, Record, Signed};
use oath_core::{now_rfc3339, Catalog, PeerActual};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

mod beacon;
mod facts;
mod link;
mod local;

/// What goes over a link, one JSON object a message.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum Msg {
    /// Who this end is, which consort it is in, and its log's heads.
    Hello { name: String, key: PeerKey, consort: String, heads: Vec<String> },
    /// The whole membership log: it is small.
    Log { entries: Vec<Signed> },
    Record { r: Signed },
    /// A machine's ask to join this end's consort.
    Join { ask: Signed },
    /// The ask is waiting for the owner.
    Pending { consort: String },
    Ping,
}

/// How long an ask to join waits for the owner.
const ASK_SECS: u64 = 3600;
/// How long a machine heard but no longer heard stays nearby.
const NEARBY_SECS: u64 = 30;
/// How often facts are gathered, and `oath update`'s preview.
const FACTS: Duration = Duration::from_secs(15);
const UPDATE: Duration = Duration::from_secs(30 * 60);

pub struct Bus {
    root: PathBuf,
    id: Identity,
    name: String,
    port: u16,
    beacon_port: u16,
    st: Mutex<State>,
}

struct Link {
    tx: Sender<Vec<u8>>,
    stream: TcpStream,
    /// The other end's X25519 link key, from the handshake.
    remote: String,
    addr: SocketAddr,
    /// This link is this machine's ask to join the other end's consort.
    join: bool,
    /// The other end's signing key, once the log says the link key is its.
    key: Option<PeerKey>,
    name: String,
    consort: String,
    last: Instant,
}

struct Near {
    name: String,
    consort: String,
    members: usize,
    addr: SocketAddr,
    heard: Instant,
}

struct AskIn {
    signed: Signed,
    addr: SocketAddr,
    heard: Instant,
}

struct Joining {
    to: SocketAddr,
    consort: Option<String>,
    pending: bool,
}

#[derive(Default)]
struct State {
    log: Log,
    links: BTreeMap<u64, Link>,
    serial: u64,
    /// Where each member's links listen, from its beacons.
    addrs: BTreeMap<PeerKey, SocketAddr>,
    dialing: BTreeSet<SocketAddr>,
    records: BTreeMap<(PeerKey, String), (Signed, Record)>,
    seq: u64,
    nearby: BTreeMap<PeerKey, Near>,
    asks: BTreeMap<PeerKey, AskIn>,
    joining: Option<Joining>,
    /// When each member's link last went down.
    seen: BTreeMap<PeerKey, String>,
    watchers: Vec<Sender<String>>,
    /// The `peer:*` objects need writing.
    dirty: bool,
    /// The member set the objects were last written for.
    written: Vec<String>,
}

fn main() {
    let mut root = PathBuf::from(oath_core::DEFAULT_ROOT);
    let mut port = consort::PORT;
    let mut beacon_port = consort::PORT;
    let mut name = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| usage(&format!("{a} needs a value")));
        match a.as_str() {
            "--root" => root = PathBuf::from(val()),
            "--port" => port = val().parse().unwrap_or_else(|_| usage("--port: a number")),
            "--beacon-port" => beacon_port = val().parse().unwrap_or_else(|_| usage("--beacon-port: a number")),
            "--name" => name = Some(val()),
            _ => usage(&format!("unknown argument {a}")),
        }
    }
    let name = name.unwrap_or_else(|| hostname(&root));
    if let Err(e) = serve(root, name, port, beacon_port) {
        eprintln!("oath-bus: {e}");
        std::process::exit(1);
    }
}

fn usage(why: &str) -> ! {
    eprintln!("oath-bus: {why}\nusage: oath-bus [--root /oath] [--port 6284] [--beacon-port 6284] [--name NAME]");
    std::process::exit(2);
}

fn hostname(root: &Path) -> String {
    Catalog::open(root)
        .ok()
        .and_then(|c| c.get(&oath_core::ObjectId::new("host", "local")).ok())
        .and_then(|o| o.desired.get("hostname").and_then(Value::as_str).map(String::from))
        .filter(|h| !h.is_empty())
        .or_else(|| std::fs::read_to_string("/proc/sys/kernel/hostname").ok().map(|h| h.trim().to_string()))
        .unwrap_or_else(|| "oath".into())
}

fn serve(root: PathBuf, name: String, port: u16, beacon_port: u16) -> std::io::Result<()> {
    let dir = root.join(consort::DIR);
    let id = Identity::load_or_make(&dir).map_err(std::io::Error::other)?;
    let log = match Log::load(&dir.join("log.json")) {
        Ok(Some(log)) if log.members().contains_key(&id.key()) => log,
        Ok(_) => Log::new(&id, &name).map_err(std::io::Error::other)?,
        Err(e) => {
            eprintln!("oath-bus: {}: {e}; starting a consort of one", dir.join("log.json").display());
            Log::new(&id, &name).map_err(std::io::Error::other)?
        }
    };
    log.save(&dir.join("log.json")).map_err(std::io::Error::other)?;
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    let bus = Arc::new(Bus {
        root,
        id,
        name,
        port,
        beacon_port,
        st: Mutex::new(State { log, dirty: true, ..Default::default() }),
    });
    let (id, members) = {
        let st = bus.lock();
        (st.log.id(), st.log.members().len())
    };
    bus.say(&format!(
        "{} in consort {} ({members} members), key {}, links on {port}",
        bus.name,
        consort::short(&id),
        bus.id.key().fingerprint()
    ));
    local::start(bus.clone())?;
    beacon::start(bus.clone())?;
    let b = bus.clone();
    thread::spawn(move || b.facts());
    let b = bus.clone();
    thread::spawn(move || b.tend());
    for conn in listener.incoming() {
        match conn {
            Ok(s) => {
                let addr = s.peer_addr().unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 0)));
                link::accept(bus.clone(), s, addr);
            }
            Err(e) => bus.say(&format!("accept: {e}")),
        }
    }
    Ok(())
}

fn send(l: &Link, m: &Msg) {
    if let Ok(b) = serde_json::to_vec(m) {
        let _ = l.tx.send(b);
    }
}

impl Bus {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.st.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn say(&self, line: &str) {
        eprintln!("oath-bus: {line}");
    }

    fn save(&self, st: &State) {
        if let Err(e) = st.log.save(&self.root.join(consort::DIR).join("log.json")) {
            self.say(&format!("saving the log: {e}"));
        }
    }

    fn hello(&self, st: &State) -> Msg {
        Msg::Hello { name: self.name.clone(), key: self.id.key(), consort: st.log.id(), heads: st.log.heads() }
    }

    /// This machine's beacon, signed.
    fn beacon(&self) -> Option<Signed> {
        let st = self.lock();
        let b = Beacon {
            v: 1,
            name: self.name.clone(),
            by: self.id.key(),
            consort: st.log.id(),
            members: st.log.members().len(),
            port: self.port,
            at: now_rfc3339(),
        };
        Signed::make(&self.id, "beacon", &b).ok()
    }

    /// A beacon from `from` (its port is the beacon's).
    fn heard(&self, b: Beacon, from: SocketAddr) {
        if b.by == self.id.key() {
            return;
        }
        let addr = SocketAddr::new(from.ip(), b.port);
        let mut st = self.lock();
        if st.log.members().contains_key(&b.by) {
            st.addrs.insert(b.by.clone(), addr);
            st.nearby.remove(&b.by);
        } else {
            let new = !st.nearby.contains_key(&b.by);
            st.nearby.insert(
                b.by.clone(),
                Near { name: b.name, consort: b.consort, members: b.members, addr, heard: Instant::now() },
            );
            if new {
                self.notify(&mut st);
            }
        }
    }

    pub fn dial_ended(&self, addr: SocketAddr) {
        self.lock().dialing.remove(&addr);
    }

    pub fn link_up(
        &self,
        tx: Sender<Vec<u8>>,
        stream: TcpStream,
        remote: String,
        addr: SocketAddr,
        join: bool,
    ) -> u64 {
        let mut st = self.lock();
        st.serial += 1;
        let serial = st.serial;
        let l = Link {
            tx,
            stream,
            remote,
            addr,
            join,
            key: None,
            name: String::new(),
            consort: String::new(),
            last: Instant::now(),
        };
        send(&l, &self.hello(&st));
        st.links.insert(serial, l);
        serial
    }

    pub fn link_down(&self, serial: u64) {
        let mut st = self.lock();
        if let Some(l) = st.links.remove(&serial) {
            if let Some(k) = l.key {
                if !st.links.values().any(|o| o.key.as_ref() == Some(&k)) {
                    st.seen.insert(k, now_rfc3339());
                    st.dirty = true;
                    self.notify(&mut st);
                }
            }
        }
    }

    /// One message from link `serial`.
    pub fn handle(&self, serial: u64, raw: &[u8]) {
        let Ok(msg) = serde_json::from_slice::<Msg>(raw) else {
            self.say("a message that does not parse");
            return;
        };
        let mut st = self.lock();
        let Some(l) = st.links.get_mut(&serial) else { return };
        l.last = Instant::now();
        match msg {
            Msg::Ping => {}
            Msg::Hello { name, key, consort, heads } => self.on_hello(&mut st, serial, name, key, consort, heads),
            Msg::Log { entries } => self.on_log(&mut st, serial, &entries),
            Msg::Record { r } => self.on_record(&mut st, serial, r),
            Msg::Join { ask } => self.on_join(&mut st, serial, ask),
            Msg::Pending { consort } => {
                let join = st.links.get(&serial).is_some_and(|l| l.join);
                if let Some(j) = st.joining.as_mut().filter(|_| join) {
                    if j.consort.as_deref() == Some(&consort) {
                        j.pending = true;
                    }
                }
            }
        }
    }

    fn on_hello(&self, st: &mut State, serial: u64, name: String, key: PeerKey, consort: String, heads: Vec<String>) {
        let members = st.log.members();
        let entries = st.log.signed();
        let behind = consort != st.log.id() || heads != st.log.heads();
        let mine: Vec<Signed> =
            st.records.iter().filter(|((by, _), _)| *by == self.id.key()).map(|(_, (s, _))| s.clone()).collect();
        let joining = st.joining.is_some();
        let Some(l) = st.links.get_mut(&serial) else { return };
        l.name = name;
        l.consort = consort.clone();
        match members.get(&key) {
            Some(m) if m.link == l.remote => {
                let first = l.key.is_none();
                l.key = Some(key);
                if behind {
                    send(l, &Msg::Log { entries });
                }
                if first {
                    for r in mine {
                        send(l, &Msg::Record { r });
                    }
                    st.dirty = true;
                    self.notify(st);
                }
            }
            Some(_) => {
                self.say(&format!("{} claims {}'s key with another link key; closing", l.addr, key.fingerprint()));
                let _ = l.stream.shutdown(std::net::Shutdown::Both);
            }
            None => {
                l.key = None;
                if !(l.join && joining) {
                    return;
                }
                let ask = Ask {
                    consort: consort.clone(),
                    name: self.name.clone(),
                    by: self.id.key(),
                    link: self.id.link_key(),
                    at: now_rfc3339(),
                };
                if let Ok(s) = Signed::make(&self.id, "ask", &ask) {
                    send(l, &Msg::Join { ask: s });
                }
                if let Some(j) = st.joining.as_mut() {
                    j.consort = Some(consort);
                }
            }
        }
    }

    fn on_log(&self, st: &mut State, serial: u64, entries: &[Signed]) {
        let Some(l) = st.links.get(&serial) else { return };
        let member = l.key.is_some();
        // A machine joining takes the consort it asked for, once that log
        // admits it with the keys it asked with.
        if !member && l.join {
            let mut theirs = Log::default();
            let Some(want) = st.joining.as_ref().and_then(|j| j.consort.clone()) else { return };
            if theirs.merge(entries).is_err() || theirs.id() != want {
                return;
            }
            let ok = theirs.members().get(&self.id.key()).is_some_and(|m| m.link == self.id.link_key());
            if !ok {
                return;
            }
            self.say(&format!("joined consort {} ({} members)", consort::short(&want), theirs.members().len()));
            st.log = theirs;
            st.joining = None;
            st.records.retain(|(by, _), _| *by == self.id.key());
            self.save(st);
            self.after_log(st);
            return;
        }
        if !member || entries.is_empty() {
            return;
        }
        let mut copy = st.log.clone();
        match copy.merge(entries) {
            Ok(0) => {}
            Ok(n) => {
                self.say(&format!("{n} new membership entries from {}", l.name));
                st.log = copy;
                self.save(st);
                self.after_log(st);
            }
            Err(e) => {
                // A log of another consort: the other end left or was
                // removed, and its next hello says so.
                if Log::default().merge(entries).map(|_| ()).is_ok() {
                    return;
                }
                self.say(&format!("entries from {}: {e}", l.name));
            }
        }
    }

    /// The log changed: who is a member now, and tell every member.
    fn after_log(&self, st: &mut State) {
        let members = st.log.members();
        if !members.contains_key(&self.id.key()) {
            self.say("this machine was removed; it is a consort of one again");
            self.reset(st);
            return;
        }
        // A link is whoever's link key the log gives the key the handshake
        // proved: a machine just admitted becomes a member's link here.
        let entries = st.log.signed();
        let hello = self.hello(st);
        let me = self.id.key();
        let mine: Vec<Signed> =
            st.records.iter().filter(|((by, _), _)| *by == me).map(|(_, (s, _))| s.clone()).collect();
        for l in st.links.values_mut() {
            match members.values().find(|m| m.link == l.remote && m.key != me) {
                Some(m) => {
                    let first = l.key.as_ref() != Some(&m.key);
                    l.key = Some(m.key.clone());
                    send(l, &hello);
                    send(l, &Msg::Log { entries: entries.clone() });
                    if first {
                        for r in &mine {
                            send(l, &Msg::Record { r: r.clone() });
                        }
                    }
                }
                None => l.key = None,
            }
        }
        st.asks.retain(|k, _| !members.contains_key(k));
        st.records.retain(|(by, _), _| members.contains_key(by));
        self.publish_asks(st);
        st.dirty = true;
        self.notify(st);
    }

    /// Back to a consort of one, with a new first entry.
    fn reset(&self, st: &mut State) {
        match Log::new(&self.id, &self.name) {
            Ok(log) => st.log = log,
            Err(e) => self.say(&format!("a new consort: {e}")),
        }
        st.joining = None;
        st.asks.clear();
        st.records.retain(|(by, _), _| *by == self.id.key());
        self.save(st);
        let hello = self.hello(st);
        for l in st.links.values_mut() {
            l.key = None;
            send(l, &hello);
        }
        st.dirty = true;
        self.notify(st);
    }

    fn on_record(&self, st: &mut State, serial: u64, s: Signed) {
        if st.links.get(&serial).and_then(|l| l.key.as_ref()).is_none() {
            return;
        }
        let Ok(r) = s.open::<Record>("record") else {
            self.say("a record with a bad signature");
            return;
        };
        if !st.log.members().contains_key(&r.by) || r.by == self.id.key() {
            return;
        }
        let k = (r.by.clone(), r.topic.clone());
        if st.records.get(&k).is_some_and(|(_, old)| old.seq >= r.seq) {
            return;
        }
        if r.topic == "status" {
            st.dirty = true;
        }
        let ev = self.record_json(st, &r);
        st.records.insert(k, (s, r));
        self.event(st, json!({ "ev": "record", "record": ev }));
    }

    fn on_join(&self, st: &mut State, serial: u64, s: Signed) {
        let Some(l) = st.links.get(&serial) else { return };
        let Ok(ask) = s.open::<Ask>("ask") else {
            self.say(&format!("an ask to join from {} with a bad signature", l.addr));
            return;
        };
        // The ask names the link key it came over and this consort.
        if ask.link != l.remote || ask.consort != st.log.id() || !oath_core::seat::is_host_name(&ask.name, 63) {
            self.say(&format!("an ask to join from {} that does not fit", l.addr));
            return;
        }
        if st.log.members().contains_key(&ask.by) {
            send(l, &Msg::Log { entries: st.log.signed() });
            return;
        }
        self.say(&format!(
            "{} at {} asks to join: code {}",
            ask.name,
            l.addr.ip(),
            join_code(&ask.consort, &ask.by, &ask.link)
        ));
        send(l, &Msg::Pending { consort: st.log.id() });
        let addr = l.addr;
        st.asks.insert(ask.by.clone(), AskIn { signed: s, addr, heard: Instant::now() });
        self.publish_asks(st);
        self.notify(st);
    }

    /// The asks this machine holds, as a record every member sees, so the
    /// owner may admit from any of them.
    fn publish_asks(&self, st: &mut State) {
        let data: Vec<Value> = st
            .asks
            .values()
            .map(|a| json!({ "ask": a.signed, "address": a.addr.ip().to_string() }))
            .collect();
        self.publish(st, "consort.asks", Value::Array(data));
    }

    /// Sign a fact about this machine and send it to every member, when
    /// it says something new.
    fn publish(&self, st: &mut State, topic: &str, data: Value) {
        let me = self.id.key();
        let k = (me.clone(), topic.to_string());
        if st.records.get(&k).is_some_and(|(_, r)| r.data == data) {
            return;
        }
        st.seq += 1;
        let r = Record { by: me, topic: topic.into(), seq: st.seq, at: now_rfc3339(), audience: "consort".into(), data };
        let Ok(s) = Signed::make(&self.id, "record", &r) else { return };
        for l in st.links.values().filter(|l| l.key.is_some()) {
            send(l, &Msg::Record { r: s.clone() });
        }
        if topic == "status" {
            st.dirty = true;
        }
        let ev = self.record_json(st, &r);
        st.records.insert(k, (s, r));
        self.event(st, json!({ "ev": "record", "record": ev }));
    }

    fn facts(self: Arc<Self>) {
        // The first records go out before the first `oath update`.
        let mut f = facts::Facts::new();
        let mut last_update: Option<Instant> = None;
        let started = Instant::now();
        loop {
            for (topic, data) in f.gather(&self.root) {
                let mut st = self.lock();
                self.publish(&mut st, topic, data);
            }
            let due = match last_update {
                None => started.elapsed() > Duration::from_secs(60),
                Some(t) => t.elapsed() > UPDATE,
            };
            if due {
                last_update = Some(Instant::now());
                if let Some(u) = facts::update(&self.root) {
                    let mut st = self.lock();
                    self.publish(&mut st, "update", u);
                }
            }
            thread::sleep(FACTS);
        }
    }

    /// Every two seconds: link to members heard and not linked, keep a
    /// join's link up, ping, drop dead links, forget the stale, and write
    /// the `peer:*` objects.
    fn tend(self: Arc<Self>) {
        let mut ticks: u64 = 0;
        loop {
            thread::sleep(Duration::from_secs(2));
            ticks += 1;
            let mut dial = Vec::new();
            {
                let mut st = self.lock();
                let me = self.id.key();
                let members = st.log.members();
                for (k, m) in members.iter().filter(|(k, _)| **k != me) {
                    let Some(addr) = st.addrs.get(k).copied() else { continue };
                    let linked = st.links.values().any(|l| l.remote == m.link);
                    // The lower key dials, so two links do not cross; the
                    // higher one dials too when nothing came for a while.
                    if !linked && !st.dialing.contains(&addr) && (me < *k || ticks % 8 == 0) {
                        dial.push((addr, false));
                    }
                }
                if let Some(j) = st.joining.as_ref() {
                    let to = j.to;
                    if !st.links.values().any(|l| l.join) && !st.dialing.contains(&to) {
                        dial.push((to, true));
                    }
                }
                for (addr, _) in &dial {
                    st.dialing.insert(*addr);
                }
                if ticks % 10 == 0 {
                    for l in st.links.values() {
                        send(l, &Msg::Ping);
                    }
                }
                for l in st.links.values() {
                    if l.last.elapsed() > link::IDLE {
                        let _ = l.stream.shutdown(std::net::Shutdown::Both);
                    }
                }
                let before = st.nearby.len() + st.asks.len();
                st.nearby.retain(|_, n| n.heard.elapsed() < Duration::from_secs(NEARBY_SECS));
                st.asks.retain(|_, a| a.heard.elapsed() < Duration::from_secs(ASK_SECS));
                if st.nearby.len() + st.asks.len() != before {
                    self.publish_asks(&mut st);
                    self.notify(&mut st);
                }
            }
            for (addr, join) in dial {
                link::dial(self.clone(), addr, join);
            }
            self.write_peers();
        }
    }

    /// The members as `peer:*` objects.
    fn write_peers(&self) {
        if !self.root.join("objects").is_dir() {
            return;
        }
        let (found, names) = {
            let mut st = self.lock();
            if !st.dirty {
                return;
            }
            st.dirty = false;
            let found = self.peer_actuals(&st);
            let names: Vec<String> = found.iter().map(|(n, _)| n.clone()).collect();
            (found, names)
        };
        let Ok(cat) = Catalog::open(&self.root) else { return };
        let same = self.lock().written == names;
        let result = if same {
            found.iter().try_for_each(|(n, a)| cat.write_peer_actual(n, a))
        } else {
            match cat.try_lock() {
                Ok(Some(lock)) => cat.write_peers(&lock, &found).map(|changes| {
                    if !changes.is_empty() {
                        self.say(&format!("peers: {}", changes.join(" ")));
                    }
                    self.lock().written = names;
                }),
                // An apply holds the catalog: try again on the next tick.
                Ok(None) => {
                    self.lock().dirty = true;
                    Ok(())
                }
                Err(e) => Err(e),
            }
        };
        if let Err(e) = result {
            self.say(&format!("writing peer objects: {e}"));
            self.lock().dirty = true;
        }
    }

    fn peer_actuals(&self, st: &State) -> Vec<(String, PeerActual)> {
        let me = self.id.key();
        let members = st.log.members();
        members
            .values()
            .map(|m| {
                let link = st.links.values().find(|l| l.key.as_ref() == Some(&m.key));
                let status = st.records.get(&(m.key.clone(), "status".into())).map(|(_, r)| r.data.clone());
                let a = PeerActual {
                    present: true,
                    me: m.key == me,
                    key: m.key.line(&format!("oath-bus@{}", m.name)),
                    fingerprint: m.key.fingerprint(),
                    address: link.map(|l| l.addr.ip().to_string()).unwrap_or_default(),
                    online: m.key == me || link.is_some(),
                    seen: st.seen.get(&m.key).cloned().unwrap_or_default(),
                    admitted_by: members.get(&m.by).map(|b| b.name.clone()).unwrap_or_else(|| m.by.fingerprint()),
                    admitted: m.at.clone(),
                    status: status.unwrap_or(Value::Null),
                };
                (m.name.clone(), a)
            })
            .collect()
    }

    fn record_json(&self, st: &State, r: &Record) -> Value {
        let name = st.log.members().get(&r.by).map(|m| m.name.clone()).unwrap_or_default();
        json!({ "peer": name, "topic": r.topic, "seq": r.seq, "at": r.at, "data": r.data })
    }

    /// Tell every watcher what is.
    fn notify(&self, st: &mut State) {
        let v = json!({ "ev": "state", "state": self.state_json(st) });
        self.event(st, v);
    }

    fn event(&self, st: &mut State, v: Value) {
        if st.watchers.is_empty() {
            return;
        }
        let line = v.to_string();
        st.watchers.retain(|w| w.send(line.clone()).is_ok());
    }

    /// The asks any member holds, from its `consort.asks` record or here.
    fn all_asks(&self, st: &State) -> Vec<(Ask, String, String)> {
        let members = st.log.members();
        let mut out: BTreeMap<PeerKey, (Ask, String, String)> = BTreeMap::new();
        for ((by, topic), (_, r)) in st.records.iter() {
            if topic != "consort.asks" {
                continue;
            }
            let via = members.get(by).map(|m| m.name.clone()).unwrap_or_default();
            for a in r.data.as_array().into_iter().flatten() {
                let Ok(s) = serde_json::from_value::<Signed>(a["ask"].clone()) else { continue };
                let Ok(ask) = s.open::<Ask>("ask") else { continue };
                if ask.consort != st.log.id() || members.contains_key(&ask.by) {
                    continue;
                }
                let addr = a["address"].as_str().unwrap_or("").to_string();
                out.insert(ask.by.clone(), (ask, via.clone(), addr));
            }
        }
        out.into_values().collect()
    }

    fn state_json(&self, st: &State) -> Value {
        let me = self.id.key();
        let members: Vec<Value> = self
            .peer_actuals(st)
            .into_iter()
            .map(|(name, a)| {
                let mut v = serde_json::to_value(&a).unwrap_or_default();
                v["name"] = json!(name);
                v
            })
            .collect();
        let nearby: Vec<Value> = st
            .nearby
            .iter()
            .map(|(k, n)| {
                json!({
                    "name": n.name,
                    "fingerprint": k.fingerprint(),
                    "consort": consort::short(&n.consort),
                    "members": n.members,
                    "address": n.addr.ip().to_string(),
                })
            })
            .collect();
        let id = st.log.id();
        let asks: Vec<Value> = self
            .all_asks(st)
            .into_iter()
            .map(|(a, via, addr)| {
                json!({
                    "name": a.name,
                    "fingerprint": a.by.fingerprint(),
                    "address": addr,
                    "via": via,
                    "at": a.at,
                    "code": join_code(&id, &a.by, &a.link),
                })
            })
            .collect();
        let joining = st.joining.as_ref().map(|j| {
            json!({
                "to": j.to.to_string(),
                "consort": j.consort.as_deref().map(consort::short),
                "code": j.consort.as_deref().map(|c| join_code(c, &me, &self.id.link_key())),
                "pending": j.pending,
            })
        });
        json!({
            "me": { "name": self.name, "key": me.line(&format!("oath-bus@{}", self.name)), "fingerprint": me.fingerprint() },
            "consort": { "id": id, "short": consort::short(&id), "members": members },
            "nearby": nearby,
            "asks": asks,
            "joining": joining,
        })
    }

    // The local socket's verbs. Each returns its answer, `ok` and all.

    fn op_join(self: &Arc<Self>, to: &str) -> Value {
        let addr = {
            let st = self.lock();
            if st.log.members().len() > 1 {
                return refuse(
                    "this machine is in a consort with others",
                    "sudo oath consort leave first, to join another",
                );
            }
            match resolve(to, self.port, &st) {
                Some(a) => a,
                None => {
                    return refuse(
                        &format!("no machine nearby is named {to}"),
                        "oath consort lists the machines heard on this LAN; an address works too",
                    )
                }
            }
        };
        {
            let mut st = self.lock();
            st.joining = Some(Joining { to: addr, consort: None, pending: false });
            for l in st.links.values_mut().filter(|l| l.join) {
                let _ = l.stream.shutdown(std::net::Shutdown::Both);
            }
        }
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(10) {
            thread::sleep(Duration::from_millis(200));
            let st = self.lock();
            let Some(j) = st.joining.as_ref() else {
                // Already admitted: the log came back at once.
                return json!({ "ok": true, "joined": true, "consort": consort::short(&st.log.id()) });
            };
            if let (Some(c), true) = (j.consort.as_ref(), j.pending) {
                let to = st.links.values().find(|l| l.join).map(|l| l.name.clone()).unwrap_or_default();
                return json!({
                    "ok": true,
                    "consort": consort::short(c),
                    "via": to,
                    "name": self.name,
                    "code": join_code(c, &self.id.key(), &self.id.link_key()),
                });
            }
        }
        refuse(&format!("{addr} did not answer the ask"), "oath get svc:oath-bus on that machine")
    }

    fn op_admit(&self, name: &str, code: &str) -> Value {
        let mut st = self.lock();
        let Some((ask, _, _)) = self.all_asks(&st).into_iter().find(|(a, _, _)| a.name == name) else {
            return refuse(&format!("no machine named {name} asks to join"), "oath consort lists the asks");
        };
        let want = join_code(&st.log.id(), &ask.by, &ask.link);
        if code.split_whitespace().collect::<String>() != want {
            return refuse(
                "that code is not this ask's",
                "compare it with the code `oath consort join` printed on the machine joining",
            );
        }
        let id = &self.id;
        match st.log.append(id, Op::Admit, &ask.by, &ask.link, &ask.name) {
            Ok(_) => {
                self.say(&format!("admitted {name}"));
                self.save(&st);
                self.after_log(&mut st);
                json!({ "ok": true, "admitted": name })
            }
            Err(e) => refuse(&e.to_string(), e.hint_str().unwrap_or("oath consort")),
        }
    }

    fn op_remove(&self, name: &str) -> Value {
        let mut st = self.lock();
        let members = st.log.members();
        let Some(m) = members.values().find(|m| m.name == name).cloned() else {
            return refuse(&format!("no member is named {name}"), "oath ls peer");
        };
        if m.key == self.id.key() {
            drop(st);
            return self.op_leave();
        }
        match st.log.append(&self.id, Op::Remove, &m.key, "", &m.name) {
            Ok(_) => {
                self.say(&format!("removed {name}"));
                self.save(&st);
                // The removed peer hears it once more, and leaves.
                let entries = st.log.signed();
                for l in st.links.values().filter(|l| l.key.as_ref() == Some(&m.key)) {
                    send(l, &Msg::Log { entries: entries.clone() });
                }
                self.after_log(&mut st);
                json!({ "ok": true, "removed": name })
            }
            Err(e) => refuse(&e.to_string(), "oath consort"),
        }
    }

    fn op_leave(&self) -> Value {
        let mut st = self.lock();
        if st.log.members().len() < 2 {
            return refuse("this machine is a consort of one", "oath consort join <peer> joins another");
        }
        let me = self.id.key();
        if let Err(e) = st.log.append(&self.id, Op::Remove, &me, "", &self.name) {
            return refuse(&e.to_string(), "oath consort");
        }
        let entries = st.log.signed();
        for l in st.links.values().filter(|l| l.key.is_some()) {
            send(l, &Msg::Log { entries: entries.clone() });
        }
        self.say("left the consort");
        self.reset(&mut st);
        json!({ "ok": true, "left": true })
    }
}

fn resolve(to: &str, port: u16, st: &State) -> Option<SocketAddr> {
    if let Ok(a) = to.parse::<SocketAddr>() {
        return Some(a);
    }
    if let Ok(ip) = to.parse::<IpAddr>() {
        return Some(SocketAddr::new(ip, port));
    }
    st.nearby.values().find(|n| n.name == to).map(|n| n.addr)
}

fn refuse(why: &str, hint: &str) -> Value {
    json!({ "ok": false, "error": why, "hint": hint })
}
