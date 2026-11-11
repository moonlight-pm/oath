//! Heads (R72): a package published on its own. Whoever makes a package
//! puts its pack on an origin (`pkg/<name>/<hash>.tar`) and its head beside
//! it, `pkg/<name>.json`, signed beside itself (`.sig`, namespace
//! [`NAMESPACE`]) by a key `host:local.publishers` lets sign that name.
//! `oath update` moves each package a machine has to its newest head, and
//! records the heads it took in [`RECORD`] so an origin cannot hand it an
//! older one. A need is a pin: when the moves leave a package's need at
//! another pin, [`settle`] re-points it if the package still finds every
//! library it loads, and otherwise holds the dependency back.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::update::{short, source_label, Move, NEEDED_HEAD};
use crate::{is_realization_id, Error, PkgNeed, Result};

/// The signature namespace of a head (`ssh-keygen -Y sign -n`).
pub const NAMESPACE: &str = "oath-pkg";
/// Under the catalog root: the head this machine took of each package.
pub const RECORD: &str = "heads.json";

/// `pkg/<name>.json`: the head of `pkg:<name>` on an origin.
pub fn key(name: &str) -> String {
    format!("pkg/{name}.json")
}

/// What a publisher signs for one package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Head {
    pub name: String,
    /// The pin: `sha256-<64 hex>` of the pack's tree.
    pub hash: String,
    /// Its runtime needs, each a pin, as on the `pkg` object.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<PkgNeed>,
    /// When it was published (RFC 3339, UTC).
    pub published: String,
    /// When the commit it was built from was made, when the publisher
    /// knew it: the order of heads, as of releases (R45).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed: Option<String>,
    /// Where the tree was built from (R47), as on the `pkg` object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Value>,
    /// The version its components name.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub home: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub category: String,
}

impl Head {
    /// It is a head of `pkg:<name>`, with pins where pins go.
    pub fn check(&self, name: &str) -> Result<()> {
        let bad = |why: String| Error::security(why, "the publisher republishes the head");
        if self.name != name {
            return Err(bad(format!("{} is signed for pkg:{}", key(name), self.name)));
        }
        if !is_realization_id(&self.hash) {
            return Err(bad(format!("{}: hash {:?} is not a pin", key(name), self.hash)));
        }
        for n in &self.needs {
            if !is_realization_id(&n.hash) {
                return Err(bad(format!("{}: need {} has no pin", key(name), n.id)));
            }
        }
        Ok(())
    }

    /// The fields `oath set` writes for it on the `pkg` object.
    pub fn fields(&self) -> Value {
        let needs: Vec<Value> = self
            .needs
            .iter()
            .map(|n| {
                json!({ "id": format!("pkg:{}", n.id.trim_start_matches("pkg:")), "hash": n.hash })
            })
            .collect();
        let mut v = json!({ "hash": self.hash, "needs": needs });
        v["source"] = self.source.clone().unwrap_or(Value::Null);
        let text = [
            ("description", &self.description),
            ("home", &self.home),
            ("category", &self.category),
        ];
        for (k, s) in text {
            if !s.is_empty() {
                v[k] = json!(s);
            }
        }
        v
    }

    /// A new `pkg` object at this head: present, no empty fields.
    pub fn doc(&self) -> Value {
        let mut v = self.fields();
        if let Some(m) = v.as_object_mut() {
            m.retain(|_, x| !x.is_null());
            m.insert("present".into(), json!(true));
        }
        v
    }

    fn order(&self) -> (Option<&str>, &str) {
        (self.committed.as_deref(), &self.published)
    }
}

/// The head a machine took of one package.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Taken {
    pub hash: String,
    pub published: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed: Option<String>,
    /// The comment of the key that signed it.
    pub signer: String,
}

impl Taken {
    pub fn of(h: &Head, signer: &str) -> Self {
        Self {
            hash: h.hash.clone(),
            published: h.published.clone(),
            committed: h.committed.clone(),
            signer: signer.to_string(),
        }
    }

    fn order(&self) -> (Option<&str>, &str) {
        (self.committed.as_deref(), &self.published)
    }
}

/// Whether `b` came after `a`: by the commits' times when both say (the
/// publication breaks a tie: a rebuild of one commit), else by when they
/// were published.
fn after(a: (Option<&str>, &str), b: (Option<&str>, &str)) -> bool {
    match (a.0, b.0) {
        (Some(x), Some(y)) if x != y => y > x,
        _ => b.1 > a.1,
    }
}

/// What ordered `a` and `b` in [`after`], and each one's time by it: when
/// each commit was made, or when each was published.
fn ordered_by<'a>(
    a: (Option<&'a str>, &'a str),
    b: (Option<&'a str>, &'a str),
) -> (&'static str, &'a str, &'a str) {
    match (a.0, b.0) {
        (Some(x), Some(y)) if x != y => ("committed", x, y),
        _ => ("published", a.1, b.1),
    }
}

/// The heads a machine took, by package name (`/oath/heads.json`).
pub type Record = BTreeMap<String, Taken>;

/// `taken` once each offered head a package in `machine` runs (its doc's
/// `hash`) is recorded. One older than the head taken stays out: an origin
/// that serves an old head to a machine an undo took back to it does not
/// lower the floor.
pub fn record(
    machine: &BTreeMap<String, Value>,
    offers: &BTreeMap<String, Offer>,
    taken: &Record,
) -> Record {
    let mut out = taken.clone();
    for (name, o) in offers {
        let runs = machine.get(name).is_some_and(|d| d["hash"].as_str() == Some(&o.head.hash));
        let older = taken.get(name).is_some_and(|t| after(o.head.order(), t.order()));
        if runs && !older {
            out.insert(name.clone(), Taken::of(&o.head, &o.signer));
        }
    }
    out
}

/// A head an origin served, checked: who signed it and where it came from.
#[derive(Clone, Debug)]
pub struct Offer {
    pub head: Head,
    pub signer: String,
    pub origin: String,
}

impl Offer {
    /// Of two offers of one name, the later.
    pub fn later(self, other: Self) -> Self {
        if after(self.head.order(), other.head.order()) {
            other
        } else {
            self
        }
    }
}

/// The [`Move::Hold`] of `name` at `at`, its pin here (none when the
/// machine lacks it), when its offered head is older than the one this
/// machine took (`taken`). A package removed since does not come back at
/// an older head either.
pub fn hold_older(name: &str, at: &str, offer: &Offer, taken: &Record) -> Option<Move> {
    let head = &offer.head;
    let t = taken.get(name).filter(|t| after(head.order(), t.order()))?;
    let (when, this, took) = ordered_by(head.order(), t.order());
    Some(Move::Hold {
        name: name.to_string(),
        at: at.to_string(),
        head: head.hash.clone(),
        why: format!(
            "it ({when} {this}) is older than the head this machine took, {} (signed by {}, \
             {when} {took})",
            short(&t.hash),
            t.signer
        ),
    })
}

/// The moves that take `machine` (each package's desired doc, by name) to
/// the heads `offers` holds. A package whose pin is its head does not
/// move. One the owner keeps (`keep`), and one whose offered head is older
/// than the one this machine took (`taken`), is held. A pin that is not
/// the head the machine took is an overlay and is replaced (R47); with no
/// record, `overlay(name, pin)` says (the release's view). A package the
/// machine lacks is made when something present here, or a head taken,
/// needs it and it has a head, and held when that head is older than the
/// one this machine took. Needs are not settled here: see [`settle`].
pub fn plan(
    machine: &BTreeMap<String, Value>,
    offers: &BTreeMap<String, Offer>,
    taken: &Record,
    keep: &BTreeSet<String>,
    overlay: impl Fn(&str, &str) -> bool,
) -> Vec<Move> {
    let mut moves = Vec::new();
    for (name, offer) in offers {
        let Some(cur) = machine.get(name) else { continue };
        let head = &offer.head;
        let mine = cur["hash"].as_str().unwrap_or("").to_string();
        let set = head.fields();
        let none = json!([]);
        let same = |k: &str, v: &Value| {
            let c = cur.get(k).unwrap_or(if k == "needs" { &none } else { &Value::Null });
            c == v || (k == "needs" && needs_eq(c, v))
        };
        if set.as_object().into_iter().flatten().all(|(k, v)| same(k, v)) {
            continue;
        }
        if let Some(hold) = hold_older(name, &mine, offer, taken) {
            moves.push(hold);
            continue;
        }
        if keep.contains(name) && mine != head.hash {
            moves.push(Move::Hold {
                name: name.clone(),
                at: mine.clone(),
                head: head.hash.clone(),
                why: "--keep".to_string(),
            });
            continue;
        }
        let is_overlay = !mine.is_empty()
            && mine != head.hash
            && match taken.get(name) {
                Some(t) => t.hash != mine,
                None => overlay(name, &mine),
            };
        moves.push(Move::Head {
            name: name.clone(),
            from: mine.clone(),
            to: head.hash.clone(),
            overlay: is_overlay
                .then(|| source_label(cur).unwrap_or_else(|| short(&mine).to_string())),
            set,
            signer: offer.signer.clone(),
            repointed: Vec::new(),
        });
    }
    // What the heads and the machine need that the machine lacks.
    let mut todo: Vec<String> = machine
        .iter()
        .filter(|(_, d)| d["present"] != json!(false))
        .flat_map(|(_, d)| need_names(d))
        .chain(moves.iter().filter_map(Move::fields).flat_map(need_names))
        .collect();
    let mut seen = BTreeSet::new();
    while let Some(n) = todo.pop() {
        if machine.contains_key(&n) || !seen.insert(n.clone()) {
            continue;
        }
        let Some(offer) = offers.get(&n) else { continue };
        if let Some(hold) = hold_older(&n, "", offer, taken) {
            moves.push(hold);
            continue;
        }
        let doc = offer.head.doc();
        todo.extend(need_names(&doc));
        moves.push(Move::New { name: n, doc, why: NEEDED_HEAD });
    }
    moves
}

/// What `oath new pkg:<name>` and `oath add` make (R81): each of `names` at
/// its head and, at theirs, each package those need that `machine` lacks.
/// Nothing the machine has moves. The moves are [`Move::Head`]s from no pin,
/// and `machine` comes back with an empty entry for each, so [`settle`] can
/// hold one that would lose a library. A need with no head is an error:
/// nothing says what that package needs in turn.
pub fn additions(
    machine: &BTreeMap<String, Value>,
    offers: &BTreeMap<String, Offer>,
    taken: &Record,
    names: &[String],
) -> Result<(BTreeMap<String, Value>, Vec<Move>)> {
    let mut with = machine.clone();
    let mut moves = Vec::new();
    let mut todo: Vec<(String, Option<String>)> =
        names.iter().rev().map(|n| (n.clone(), None)).collect();
    while let Some((name, by)) = todo.pop() {
        if with.contains_key(&name) {
            continue;
        }
        let Some(offer) = offers.get(&name) else {
            let what = match by {
                Some(p) => format!("pkg:{p} needs pkg:{name}, which has no head on any origin"),
                None => format!("pkg:{name} has no head on any origin"),
            };
            return Err(Error::hint(what, format!("oath ls --origin {name}")));
        };
        if let Some(t) = taken.get(&name).filter(|t| after(offer.head.order(), t.order())) {
            let (when, this, took) = ordered_by(offer.head.order(), t.order());
            let head = format!(
                "the head on the origins, {} ({when} {this}), is older than the one this machine \
                 took, {} (signed by {}, {when} {took})",
                short(&offer.head.hash),
                short(&t.hash),
                t.signer
            );
            let what = match by {
                Some(p) => format!("pkg:{p} needs pkg:{name}, and {head}"),
                None => format!("pkg:{name}: {head}"),
            };
            let pin = format!("sudo oath new pkg:{name} hash={}", t.hash);
            return Err(Error::hint(what, format!("nothing changed; `{pin}` pins the one taken")));
        }
        let set = offer.head.fields();
        todo.extend(need_names(&set).into_iter().rev().map(|n| (n, Some(name.clone()))));
        with.insert(name.clone(), json!({ "present": true, "hash": "" }));
        moves.push(Move::Head {
            name,
            from: String::new(),
            to: offer.head.hash.clone(),
            set,
            overlay: None,
            signer: offer.signer.clone(),
            repointed: Vec::new(),
        });
    }
    Ok((with, moves))
}

/// One package as the moves leave it.
struct Final {
    hash: String,
    present: bool,
    needs: Vec<(String, String)>,
}

/// Settle the needs of `moves` (from [`plan`]) against `machine`. Where the
/// moves leave a need of a present package at another pin, the need is
/// re-pointed when the package loses no library: every `DT_NEEDED` name its
/// ELF files load (`loads`) that some present package's `lib/` provides
/// (`provides`) now is still provided after the moves. Otherwise the
/// dependency is held at the machine's pin (or, when that does not help,
/// the package itself), whether a head or a release moved it, and the rest
/// is settled again. A package that needs one the machine lacks and the
/// moves hold (its head is older than the one this machine took) is held
/// too, whether the moves would move it or make it. `loads` and
/// `provides` return None when the tree is not in the store yet: a preview
/// re-points such a need unchecked, and says so.
pub fn settle(
    machine: &BTreeMap<String, Value>,
    mut moves: Vec<Move>,
    mut loads: impl FnMut(&str, &str) -> Option<BTreeSet<String>>,
    mut provides: impl FnMut(&str, &str) -> Option<BTreeSet<String>>,
) -> Vec<Move> {
    let mut load_cache: HashMap<(String, String), Option<BTreeSet<String>>> = HashMap::new();
    let mut prov_cache: HashMap<(String, String), Option<BTreeSet<String>>> = HashMap::new();
    let mut load = |n: &str, h: &str| {
        load_cache.entry((n.to_string(), h.to_string())).or_insert_with(|| loads(n, h)).clone()
    };
    let mut prov = |n: &str, h: &str| {
        prov_cache.entry((n.to_string(), h.to_string())).or_insert_with(|| provides(n, h)).clone()
    };
    let before = finals(machine, &[]);
    loop {
        let after = finals(machine, &moves);
        type Prov<'a> = dyn FnMut(&str, &str) -> Option<BTreeSet<String>> + 'a;
        let union = |set: &BTreeMap<String, Final>, prov: &mut Prov| {
            let mut all = BTreeSet::new();
            let mut known = true;
            for (n, f) in set.iter().filter(|(_, f)| f.present && !f.hash.is_empty()) {
                match prov(n, &f.hash) {
                    Some(p) => all.extend(p),
                    None => known = false,
                }
            }
            (all, known)
        };
        let (had, had_known) = union(&before, &mut prov);
        let (has, has_known) = union(&after, &mut prov);
        let mut repoint: BTreeMap<String, (Vec<(String, String)>, bool)> = BTreeMap::new();
        let mut hold: Option<(String, String)> = None;
        'pkgs: for (p, f) in after.iter().filter(|(_, f)| f.present) {
            for (q, h) in &f.needs {
                let Some(qf) = after.get(q) else {
                    // One the machine lacks and the moves hold: what needs
                    // it is not moved or made either.
                    let waits =
                        moves.iter().any(|m| m.name() == q && matches!(m, Move::Hold { .. }));
                    let takes = moves.iter().any(|m| {
                        m.name() == p
                            && matches!(
                                m,
                                Move::Head { .. }
                                    | Move::Pin { .. }
                                    | Move::Rebuild { .. }
                                    | Move::New { .. }
                            )
                    });
                    if waits && takes {
                        hold = Some((p.clone(), format!("pkg:{p} needs pkg:{q}, which waits")));
                        break 'pkgs;
                    }
                    continue;
                };
                if qf.hash == *h || qf.hash.is_empty() {
                    continue;
                }
                // What this package loses: a library some package provided
                // before and none does after, that it loads now and did not
                // already lack.
                let now = load(p, &f.hash);
                let was = before.get(p).filter(|b| !b.hash.is_empty()).map(|b| load(p, &b.hash));
                let checked = now.is_some() && had_known && has_known;
                let lost: Vec<String> = match &now {
                    Some(libs) if checked => libs
                        .iter()
                        .filter(|l| !has.contains(*l))
                        .filter(|l| {
                            had.contains(*l)
                                || !matches!(&was, Some(Some(w)) if w.contains(*l))
                        })
                        .cloned()
                        .collect(),
                    _ => Vec::new(),
                };
                if lost.is_empty() {
                    let e = repoint.entry(p.clone()).or_insert_with(|| (Vec::new(), true));
                    e.0.push((q.clone(), qf.hash.clone()));
                    e.1 &= checked;
                    continue;
                }
                let why = format!(
                    "pkg:{p} needs pkg:{q} at {}, and would lose {}",
                    short(h),
                    lost.join(", ")
                );
                let moving = |n: &str| {
                    moves.iter().any(|m| {
                        m.name() == n && matches!(m, Move::Head { .. } | Move::Pin { .. })
                    })
                };
                let machine_has = |n: &str, h: &str| before.get(n).is_some_and(|b| b.hash == h);
                hold = if moving(q) && machine_has(q, h) {
                    Some((q.clone(), why))
                } else if moving(p) {
                    Some((p.clone(), why))
                } else if moving(q) {
                    Some((q.clone(), why))
                } else {
                    // Nothing an update can hold: the apply's needs check
                    // says what is wrong.
                    continue;
                };
                break 'pkgs;
            }
        }
        if let Some((name, why)) = hold {
            for m in moves.iter_mut().filter(|m| m.name() == name) {
                let (at, head) = match m {
                    Move::Head { from, to, .. }
                    | Move::Pin { from, to, .. }
                    | Move::Rebuild { from, to, .. } => (from.clone(), to.clone()),
                    Move::New { doc, .. } => {
                        (String::new(), doc["hash"].as_str().unwrap_or("").to_string())
                    }
                    _ => continue,
                };
                *m = Move::Hold { name: name.clone(), at, head, why: why.clone() };
            }
            continue;
        }
        for (p, (to, checked)) in repoint {
            let i = moves.iter().position(|m| m.name() == p && m.fields().is_some());
            let cur_needs = match i {
                Some(i) => moves[i].fields().unwrap()["needs"].clone(),
                None => machine.get(&p).map(|d| d["needs"].clone()).unwrap_or(Value::Null),
            };
            let mut needs = cur_needs.as_array().cloned().unwrap_or_default();
            let mut moved = Vec::new();
            for n in needs.iter_mut() {
                let id = n["id"].as_str().unwrap_or("").to_string();
                let name = id.trim_start_matches("pkg:").to_string();
                if let Some((_, h)) = to.iter().find(|(q, _)| *q == name) {
                    let was = short(n["hash"].as_str().unwrap_or("")).to_string();
                    moved.push(format!("{id} {was} -> {}", short(h)));
                    n["hash"] = json!(h);
                }
            }
            match i.map(|i| &mut moves[i]) {
                Some(Move::Head { set, repointed, .. }) => {
                    set["needs"] = json!(needs);
                    repointed.extend(moved);
                }
                Some(Move::New { doc, .. }) => doc["needs"] = json!(needs),
                Some(other) => {
                    if let Some(set) = match other {
                        Move::Repoint { set, .. } => Some(set),
                        _ => None,
                    } {
                        set["needs"] = json!(needs);
                    }
                }
                None => moves.push(Move::Repoint {
                    name: p.clone(),
                    set: json!({ "needs": needs }),
                    moved,
                    checked,
                }),
            }
        }
        // A package made only for what is now held is not made. One the
        // release makes for itself (new to the profile, or fixed by the
        // output) is, though nothing here needs it.
        let wanted: BTreeSet<String> = finals(machine, &moves)
            .iter()
            .filter(|(_, f)| f.present)
            .flat_map(|(_, f)| f.needs.iter().map(|(q, _)| q.clone()).collect::<Vec<_>>())
            .collect();
        moves.retain(|m| !(m.made_for_a_need() && !wanted.contains(m.name())));
        return moves;
    }
}

/// Every package as `moves` leave `machine`.
fn finals(machine: &BTreeMap<String, Value>, moves: &[Move]) -> BTreeMap<String, Final> {
    let mut out: BTreeMap<String, Final> = machine
        .iter()
        .map(|(n, d)| {
            let f = Final {
                hash: d["hash"].as_str().unwrap_or("").to_string(),
                present: d["present"] != json!(false),
                needs: need_pins(d),
            };
            (n.clone(), f)
        })
        .collect();
    for m in moves {
        let Some(set) = m.fields() else { continue };
        let f = out.entry(m.name().to_string()).or_insert(Final {
            hash: String::new(),
            present: true,
            needs: Vec::new(),
        });
        if let Some(h) = set["hash"].as_str() {
            f.hash = h.to_string();
        }
        if set.get("needs").is_some_and(|n| !n.is_null()) {
            f.needs = need_pins(set);
        }
        if let Some(p) = set["present"].as_bool() {
            f.present = p;
        }
    }
    out
}

fn need_pins(doc: &Value) -> Vec<(String, String)> {
    doc["needs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| {
            let id = n["id"].as_str()?.trim_start_matches("pkg:").to_string();
            Some((id, n["hash"].as_str().unwrap_or("").to_string()))
        })
        .collect()
}

fn need_names(doc: &Value) -> Vec<String> {
    need_pins(doc).into_iter().map(|(n, _)| n).collect()
}

/// Two needs lists with the same pins, `pkg:` or not.
fn needs_eq(a: &Value, b: &Value) -> bool {
    let mut x = need_pins(a);
    let mut y = need_pins(b);
    x.sort();
    y.sort();
    x == y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_times_in_utc_order_as_text() {
        let utc = |s| crate::rfc3339_utc(s).unwrap();
        assert_eq!(utc("2026-11-07T11:00:00-08:00"), "2026-11-07T19:00:00Z");
        assert_eq!(utc("2026-11-07T19:00:00.5Z"), "2026-11-07T19:00:00Z");
        let (pacific, later) = (utc("2026-11-07T11:00:00-08:00"), "2026-11-07T19:30:00Z");
        assert!(after((Some(&pacific), ""), (Some(later), "")));
        assert_eq!(crate::rfc3339_utc("yesterday"), None);
    }

    fn h(c: char) -> String {
        format!("sha256-{}", c.to_string().repeat(64))
    }

    fn head(name: &str, hash: char, needs: &[(&str, char)], published: &str) -> Offer {
        Offer {
            head: Head {
                name: name.into(),
                hash: h(hash),
                needs: needs
                    .iter()
                    .map(|(n, c)| PkgNeed { id: format!("pkg:{n}"), hash: h(*c) })
                    .collect(),
                published: published.into(),
                committed: None,
                source: None,
                version: String::new(),
                description: String::new(),
                home: String::new(),
                category: String::new(),
            },
            signer: "oath@novus".into(),
            origin: "https://o".into(),
        }
    }

    fn doc(hash: char, needs: &[(&str, char)]) -> Value {
        let needs: Vec<Value> =
            needs.iter().map(|(n, c)| json!({ "id": format!("pkg:{n}"), "hash": h(*c) })).collect();
        json!({ "present": true, "hash": h(hash), "needs": needs })
    }

    fn offers(list: Vec<Offer>) -> BTreeMap<String, Offer> {
        list.into_iter().map(|o| (o.head.name.clone(), o)).collect()
    }

    /// Trees as maps: (name, hash char) → what it loads / provides.
    type Trees = BTreeMap<(String, String), BTreeSet<String>>;

    fn trees(list: &[(&str, char, &[&str])]) -> Trees {
        list.iter()
            .map(|(n, c, libs)| {
                ((n.to_string(), h(*c)), libs.iter().map(|l| l.to_string()).collect())
            })
            .collect()
    }

    fn run(
        machine: &BTreeMap<String, Value>,
        moves: Vec<Move>,
        loads: &Trees,
        provides: &Trees,
    ) -> Vec<Move> {
        settle(
            machine,
            moves,
            |n, x| Some(loads.get(&(n.to_string(), x.to_string())).cloned().unwrap_or_default()),
            |n, x| Some(provides.get(&(n.to_string(), x.to_string())).cloned().unwrap_or_default()),
        )
    }

    fn texts(moves: &[Move]) -> Vec<String> {
        moves.iter().map(Move::describe).collect()
    }

    #[test]
    fn a_package_moves_to_its_head() {
        let machine = BTreeMap::from([("sola".to_string(), doc('a', &[]))]);
        let moves = plan(
            &machine,
            &offers(vec![head("sola", 'b', &[], "2026-11-06T00:00:00Z")]),
            &Record::new(),
            &BTreeSet::new(),
            |_, _| false,
        );
        assert_eq!(moves.len(), 1);
        assert!(matches!(&moves[0], Move::Head { overlay: None, .. }), "{:?}", texts(&moves));
        assert!(texts(&moves)[0].contains("signed by oath@novus"));
        // At its head already: nothing moves.
        let machine = BTreeMap::from([("sola".to_string(), doc('b', &[]))]);
        let o = offers(vec![head("sola", 'b', &[], "2026-11-06T00:00:00Z")]);
        assert!(plan(&machine, &o, &Record::new(), &BTreeSet::new(), |_, _| false).is_empty());
    }

    #[test]
    fn an_older_head_a_keep_and_an_overlay() {
        let machine = BTreeMap::from([("sola".to_string(), doc('c', &[]))]);
        let o = offers(vec![head("sola", 'b', &[], "2026-11-05T00:00:00Z")]);
        let taken = Record::from([(
            "sola".to_string(),
            Taken { hash: h('c'), published: "2026-11-06T00:00:00Z".into(), ..Taken::default() },
        )]);
        let m = plan(&machine, &o, &taken, &BTreeSet::new(), |_, _| false);
        assert!(texts(&m)[0].contains("older than the head this machine took"), "{:?}", texts(&m));
        let keep = BTreeSet::from(["sola".to_string()]);
        let m = plan(&machine, &o, &Record::new(), &keep, |_, _| false);
        assert!(texts(&m)[0].contains("waits: --keep"));
        // The machine runs a pin that is not the head it took: an overlay.
        let taken = Record::from([(
            "sola".to_string(),
            Taken { hash: h('d'), published: "2026-11-01T00:00:00Z".into(), ..Taken::default() },
        )]);
        let m = plan(&machine, &o, &taken, &BTreeSet::new(), |_, _| false);
        assert!(m[0].replaces_overlay(), "{:?}", texts(&m));
        assert!(texts(&m)[0].contains("not its head"), "{:?}", texts(&m));
    }

    #[test]
    fn the_record_takes_what_runs_and_never_goes_back() {
        // novus published sola 'b' and runs it; the record says 'a'.
        let machine = BTreeMap::from([
            ("sola".to_string(), doc('b', &[])),
            ("mesa".to_string(), doc('m', &[])),
        ]);
        let taken = Record::from([(
            "sola".to_string(),
            Taken { hash: h('a'), published: "2026-11-10T01:44:00Z".into(), ..Taken::default() },
        )]);
        let o = offers(vec![
            head("sola", 'b', &[], "2026-11-10T04:03:54Z"),
            head("mesa", 'M', &[], "2026-11-10T00:00:00Z"),
        ]);
        let r = record(&machine, &o, &taken);
        assert_eq!(r["sola"].hash, h('b'));
        assert_eq!(r["sola"].signer, "oath@novus");
        assert!(!r.contains_key("mesa"), "it runs another pin: {r:?}");
        // An undo took sola back to 'a', and an origin serves 'a' again.
        let machine = BTreeMap::from([("sola".to_string(), doc('a', &[]))]);
        let o = offers(vec![head("sola", 'a', &[], "2026-11-10T01:44:00Z")]);
        assert_eq!(record(&machine, &o, &r)["sola"].hash, h('b'));
    }

    #[test]
    fn commits_order_heads_before_publication() {
        let mut a = head("sola", 'a', &[], "2026-11-06T00:00:00Z");
        let mut b = head("sola", 'b', &[], "2026-11-01T00:00:00Z");
        a.head.committed = Some("2026-11-01T00:00:00Z".into());
        b.head.committed = Some("2026-11-02T00:00:00Z".into());
        assert_eq!(a.clone().later(b.clone()).head.hash, h('b'));
        b.head.committed = None;
        assert_eq!(a.later(b).head.hash, h('a'));
    }

    #[test]
    fn an_older_head_names_the_times_that_ordered_it() {
        // Published after the head taken, from an older commit.
        let mut b = head("sola", 'b', &[], "2026-11-08T00:00:00Z");
        b.head.committed = Some("2026-11-01T00:00:00Z".into());
        let mut t = Taken {
            hash: h('c'),
            published: "2026-11-06T00:00:00Z".into(),
            committed: Some("2026-11-02T00:00:00Z".into()),
            signer: "oath@buccina".into(),
        };
        let machine = BTreeMap::from([("sola".to_string(), doc('c', &[]))]);
        let names = ["sola".to_string()];
        let said = |o: &BTreeMap<String, Offer>, t: &Taken| {
            let taken = Record::from([("sola".to_string(), t.clone())]);
            let m = plan(&machine, o, &taken, &BTreeSet::new(), |_, _| false);
            let e = additions(&BTreeMap::new(), o, &taken, &names).unwrap_err().to_string();
            (texts(&m)[0].clone(), e)
        };
        let (held, refused) = said(&offers(vec![b.clone()]), &t);
        assert!(held.contains("it (committed 2026-11-01T00:00:00Z) is older"), "{held}");
        assert!(held.contains("oath@buccina, committed 2026-11-02T00:00:00Z)"), "{held}");
        assert!(refused.contains("bbbbbbbbbbbb (committed 2026-11-01T00:00:00Z)"), "{refused}");
        assert!(refused.contains("committed 2026-11-02T00:00:00Z)"), "{refused}");
        // A rebuild of the commit taken, or a head with no commit: the
        // publications order them.
        b.head.published = "2026-11-05T00:00:00Z".into();
        for committed in [Some("2026-11-01T00:00:00Z"), None] {
            t.committed = committed.map(str::to_string);
            let (held, refused) = said(&offers(vec![b.clone()]), &t);
            assert!(held.contains("it (published 2026-11-05T00:00:00Z) is older"), "{held}");
            assert!(held.contains("oath@buccina, published 2026-11-06T00:00:00Z)"), "{held}");
            assert!(refused.contains("(published 2026-11-05T00:00:00Z)"), "{refused}");
            assert!(!refused.contains("committed"), "{refused}");
        }
    }

    #[test]
    fn a_need_the_machine_lacks_is_made_from_its_head() {
        let machine = BTreeMap::from([("sola".to_string(), doc('a', &[]))]);
        let o = offers(vec![
            head("sola", 'b', &[("libnew", 'n')], "t"),
            head("libnew", 'n', &[], "t"),
            head("unrelated", 'u', &[], "t"),
        ]);
        let m = plan(&machine, &o, &Record::new(), &BTreeSet::new(), |_, _| false);
        assert_eq!(m.iter().map(Move::kind).collect::<Vec<_>>(), ["head", "new"]);
        assert_eq!(m[1].name(), "libnew");
    }

    #[test]
    fn a_package_removed_never_comes_back_at_an_older_head() {
        // This machine took libnew at 'N' and removed it; Sola's head needs
        // it again, and the origins serve an older one.
        let machine = BTreeMap::from([("sola".to_string(), doc('a', &[]))]);
        let taken = Record::from([(
            "libnew".to_string(),
            Taken { hash: h('N'), published: "2026-11-06T00:00:00Z".into(), ..Taken::default() },
        )]);
        let o = offers(vec![
            head("sola", 'b', &[("libnew", 'n')], "2026-11-07T00:00:00Z"),
            head("libnew", 'n', &[], "2026-11-05T00:00:00Z"),
        ]);
        let moves = plan(&machine, &o, &taken, &BTreeSet::new(), |_, _| false);
        let kinds = |m: &[Move]| m.iter().map(Move::kind).collect::<Vec<_>>();
        assert_eq!(kinds(&moves), ["head", "hold"], "{:?}", texts(&moves));
        let said = texts(&moves)[1].clone();
        assert!(said.starts_with("pkg:libnew sha256-nnnnnnnnnnnn waits: it (published"), "{said}");
        assert!(said.contains("older than the head this machine took"), "{said}");
        assert_eq!(moves[1].to(), None, "it ends on no pin");
        // Sola waits for it: nothing is moved or made half-way.
        let none = trees(&[]);
        let m = run(&machine, moves, &none, &none);
        assert_eq!(kinds(&m), ["hold", "hold"], "{:?}", texts(&m));
        assert!(texts(&m)[0].contains("pkg:sola needs pkg:libnew, which waits"), "{:?}", texts(&m));
        // Needed through a package the moves make: that one waits too.
        let o = offers(vec![
            head("sola", 'b', &[("gtk", 'g')], "2026-11-07T00:00:00Z"),
            head("gtk", 'g', &[("libnew", 'n')], "2026-11-07T00:00:00Z"),
            head("libnew", 'n', &[], "2026-11-05T00:00:00Z"),
        ]);
        let moves = plan(&machine, &o, &taken, &BTreeSet::new(), |_, _| false);
        assert_eq!(kinds(&moves), ["head", "new", "hold"], "{:?}", texts(&moves));
        let m = run(&machine, moves, &none, &none);
        assert_eq!(kinds(&m), ["hold", "hold", "hold"], "{:?}", texts(&m));
        assert_eq!(addition_names(&m), ["sola", "gtk", "libnew"]);
        assert!(texts(&m)[1].contains("pkg:gtk needs pkg:libnew, which waits"), "{:?}", texts(&m));
        assert!(texts(&m)[0].contains("pkg:sola needs pkg:gtk, which waits"), "{:?}", texts(&m));
    }

    #[test]
    fn a_need_is_repointed_when_every_library_stays() {
        // buccina's mesa moved; Sola's head did not.
        let machine = BTreeMap::from([
            ("mesa".to_string(), doc('m', &[])),
            ("sola".to_string(), doc('s', &[("mesa", 'm')])),
        ]);
        let o = offers(vec![head("mesa", 'M', &[], "t")]);
        let moves = plan(&machine, &o, &Record::new(), &BTreeSet::new(), |_, _| false);
        let loads = trees(&[("sola", 's', &["libEGL.so.1", "libc.so.6"])]);
        let provides = trees(&[("mesa", 'm', &["libEGL.so.1"]), ("mesa", 'M', &["libEGL.so.1"])]);
        let m = run(&machine, moves.clone(), &loads, &provides);
        let kinds: Vec<&str> = m.iter().map(Move::kind).collect();
        assert_eq!(kinds, ["head", "repoint"], "{:?}", texts(&m));
        assert_eq!(m[1].fields().unwrap()["needs"][0]["hash"], json!(h('M')));
        assert!(texts(&m)[1].contains("every library it loads is still in /lib"));
        // The new mesa drops libEGL: mesa waits, and says for whom.
        let provides = trees(&[("mesa", 'm', &["libEGL.so.1"]), ("mesa", 'M', &["libEGL.so.2"])]);
        let m = run(&machine, moves, &loads, &provides);
        assert_eq!(m.iter().map(Move::kind).collect::<Vec<_>>(), ["hold"], "{:?}", texts(&m));
        assert!(texts(&m)[0].contains("pkg:sola needs pkg:mesa at sha256-mmmm"), "{:?}", texts(&m));
        assert!(texts(&m)[0].contains("would lose libEGL.so.1"));
    }

    #[test]
    fn a_library_already_missing_does_not_hold_anything() {
        // libc.so.6 is in no tree here (as a dlopen'd or system library
        // may be): it was missing before the moves too.
        let machine = BTreeMap::from([
            ("mesa".to_string(), doc('m', &[])),
            ("sola".to_string(), doc('s', &[("mesa", 'm')])),
        ]);
        let o = offers(vec![head("mesa", 'M', &[], "t")]);
        let moves = plan(&machine, &o, &Record::new(), &BTreeSet::new(), |_, _| false);
        let loads = trees(&[("sola", 's', &["libEGL.so.1", "libgone.so"])]);
        let provides = trees(&[("mesa", 'm', &["libEGL.so.1"]), ("mesa", 'M', &["libEGL.so.1"])]);
        let m = run(&machine, moves, &loads, &provides);
        assert_eq!(m.iter().map(Move::kind).collect::<Vec<_>>(), ["head", "repoint"]);
    }

    #[test]
    fn a_head_built_against_a_held_pin_is_repointed_back() {
        // Sola's head needs a mesa no head offers now; the machine's mesa
        // still has every library it loads.
        let machine = BTreeMap::from([
            ("mesa".to_string(), doc('m', &[])),
            ("sola".to_string(), doc('s', &[("mesa", 'm')])),
        ]);
        let o = offers(vec![head("sola", 'S', &[("mesa", 'x')], "t")]);
        let moves = plan(&machine, &o, &Record::new(), &BTreeSet::new(), |_, _| false);
        let loads = trees(&[("sola", 'S', &["libEGL.so.1"]), ("sola", 's', &["libEGL.so.1"])]);
        let provides = trees(&[("mesa", 'm', &["libEGL.so.1"])]);
        let m = run(&machine, moves.clone(), &loads, &provides);
        assert_eq!(m.iter().map(Move::kind).collect::<Vec<_>>(), ["head"], "{:?}", texts(&m));
        assert_eq!(m[0].fields().unwrap()["needs"][0]["hash"], json!(h('m')));
        assert!(texts(&m)[0].contains("its needs move: pkg:mesa"));
        // Its new tree loads what no mesa here has: Sola waits.
        let loads = trees(&[
            ("sola", 'S', &["libEGL.so.1", "libgbm.so.1"]),
            ("sola", 's', &["libEGL.so.1"]),
        ]);
        let provides = trees(&[("mesa", 'm', &["libEGL.so.1"]), ("mesa", 'x', &["libgbm.so.1"])]);
        let m = run(&machine, moves, &loads, &provides);
        assert_eq!(m.iter().map(Move::kind).collect::<Vec<_>>(), ["hold"], "{:?}", texts(&m));
    }

    #[test]
    fn a_package_the_release_makes_for_itself_is_made_though_nothing_needs_it() {
        let machine = BTreeMap::from([("bash".to_string(), doc('b', &[]))]);
        let new = |name: &str, c, why| Move::New { name: name.into(), doc: doc(c, &[]), why };
        let moves = vec![
            new("foot", 'f', "new to the profile"),
            new("zsh", 'z', "the output fixes it"),
            new("libx", 'x', crate::update::NEEDED),
            new("liby", 'y', NEEDED_HEAD),
        ];
        let m = settle(&machine, moves, |_, _| None, |_, _| None);
        assert_eq!(m.iter().map(Move::name).collect::<Vec<_>>(), ["foot", "zsh"]);
    }

    #[test]
    fn a_preview_without_the_packs_says_it_checks_later() {
        let machine = BTreeMap::from([
            ("mesa".to_string(), doc('m', &[])),
            ("sola".to_string(), doc('s', &[("mesa", 'm')])),
        ]);
        let o = offers(vec![head("mesa", 'M', &[], "t")]);
        let moves = plan(&machine, &o, &Record::new(), &BTreeSet::new(), |_, _| false);
        let m = settle(&machine, moves, |_, _| None, |_, _| None);
        assert_eq!(m.iter().map(Move::kind).collect::<Vec<_>>(), ["head", "repoint"]);
        assert!(texts(&m)[1].contains("checks its libraries once the packs are here"));
    }

    fn addition_names(moves: &[Move]) -> Vec<String> {
        moves.iter().map(|m| m.name().to_string()).collect()
    }

    #[test]
    fn additions_take_what_the_machine_lacks_and_move_nothing_it_has() {
        let machine = BTreeMap::from([("glibc".to_string(), doc('g', &[]))]);
        let o = offers(vec![
            head("cell", 'c', &[("crun", 'r'), ("glibc", 'g')], "t"),
            head("crun", 'r', &[("seccomp", 's')], "t"),
            head("seccomp", 's', &[], "t"),
            head("glibc", 'G', &[], "t"),
            head("other", 'o', &[], "t"),
        ]);
        let (with, moves) = additions(&machine, &o, &Record::new(), &["cell".to_string()]).unwrap();
        assert_eq!(addition_names(&moves), ["cell", "crun", "seccomp"]);
        assert_eq!(with["cell"]["hash"], "");
        assert_eq!(with["glibc"], machine["glibc"]);
    }

    #[test]
    fn a_need_with_no_head_is_named() {
        let o = offers(vec![head("cell", 'c', &[("crun", 'r')], "t")]);
        let names = ["cell".to_string()];
        let e = additions(&BTreeMap::new(), &o, &Record::new(), &names).unwrap_err().to_string();
        assert!(e.contains("pkg:cell needs pkg:crun, which has no head"), "{e}");
        let names = ["nothing".to_string()];
        let e = additions(&BTreeMap::new(), &o, &Record::new(), &names).unwrap_err().to_string();
        assert!(e.contains("pkg:nothing has no head"), "{e}");
    }

    #[test]
    fn an_addition_never_takes_a_head_older_than_one_taken() {
        let o = offers(vec![
            head("cell", 'c', &[("crun", 'r')], "2026-11-05T00:00:00Z"),
            head("crun", 'r', &[], "2026-11-05T00:00:00Z"),
        ]);
        let later =
            |c| Taken { hash: h(c), published: "2026-11-06T00:00:00Z".into(), ..Taken::default() };
        let names = ["cell".to_string()];
        let taken = Record::from([("cell".to_string(), later('d'))]);
        let e = additions(&BTreeMap::new(), &o, &taken, &names).unwrap_err().to_string();
        assert!(e.contains("pkg:cell: the head on the origins"), "{e}");
        assert!(e.contains("is older than the one this machine took"), "{e}");
        let taken = Record::from([("crun".to_string(), later('e'))]);
        let e = additions(&BTreeMap::new(), &o, &taken, &names).unwrap_err().to_string();
        assert!(e.contains("pkg:cell needs pkg:crun, and the head on the origins"), "{e}");
        let taken = Record::from([(
            "cell".to_string(),
            Taken { hash: h('b'), published: "2026-11-04T00:00:00Z".into(), ..Taken::default() },
        )]);
        let (_, moves) = additions(&BTreeMap::new(), &o, &taken, &names).unwrap();
        assert_eq!(addition_names(&moves), ["cell", "crun"], "a newer head is taken");
    }

    #[test]
    fn a_new_package_runs_with_the_machines_pin_if_it_loses_no_library() {
        let machine = BTreeMap::from([("certs".to_string(), doc('n', &[]))]);
        let o = offers(vec![head("cell", 'c', &[("certs", 'o')], "t")]);
        let (with, moves) = additions(&machine, &o, &Record::new(), &["cell".to_string()]).unwrap();
        let none = trees(&[]);
        let m = run(&with, moves, &none, &none);
        assert_eq!(addition_names(&m), ["cell"]);
        let Move::Head { set, repointed, .. } = &m[0] else { panic!("{m:?}") };
        assert_eq!(set["needs"][0]["hash"], json!(h('n')));
        assert_eq!(repointed.len(), 1, "{repointed:?}");
    }

    #[test]
    fn a_new_package_that_would_lose_a_library_is_held() {
        let machine = BTreeMap::from([("foo".to_string(), doc('n', &[]))]);
        let o = offers(vec![head("app", 'a', &[("foo", 'o')], "t")]);
        let (with, moves) = additions(&machine, &o, &Record::new(), &["app".to_string()]).unwrap();
        let loads = trees(&[("app", 'a', &["libfoo.so.1"])]);
        let provides = trees(&[("foo", 'n', &["libfoo.so.2"])]);
        let m = run(&with, moves, &loads, &provides);
        let Move::Hold { name, why, .. } = &m[0] else { panic!("{m:?}") };
        assert_eq!(name, "app");
        assert!(why.contains("would lose libfoo.so.1"), "{why}");
    }

    #[test]
    fn a_head_names_its_package_and_pins() {
        let o = head("sola", 'a', &[("mesa", 'b')], "t");
        assert!(o.head.check("sola").is_ok());
        let e = o.head.check("sola-browser").unwrap_err();
        assert!(e.to_string().contains("signed for pkg:sola"), "{e}");
        let mut bad = o.head.clone();
        bad.needs[0].hash = "x".into();
        assert!(bad.check("sola").is_err());
    }
}
