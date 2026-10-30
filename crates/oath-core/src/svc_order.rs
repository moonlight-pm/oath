//! `svc` `wants`: start dependencies first. No cycles.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::{Error, Result};
use crate::kinds::Svc;

pub fn normalize_want(w: &str) -> String {
    if w.contains(':') {
        w.to_string()
    } else {
        format!("svc:{w}")
    }
}

/// Enabled services in start order (wants first). Disabled ids are omitted.
/// Unknown or disabled wants are ignored (ordering, not Requires).
/// A cycle (or a service wanting itself) is an error: apply refuses it.
pub fn start_order(svcs: &[(String, Svc)]) -> Result<Vec<String>> {
    for (id, spec) in svcs.iter().filter(|(_, s)| s.enabled) {
        if spec.wants.iter().any(|w| normalize_want(w) == *id) {
            return Err(Error::hint(format!("{id} wants itself"), "oath schema svc"));
        }
    }
    let (order, stuck) = order_and_stuck(svcs);
    if !stuck.is_empty() {
        return Err(Error::hint(
            format!("svc wants cycle among {}", stuck.join(", ")),
            "oath schema svc",
        ));
    }
    Ok(order)
}

/// PID 1's order: everything that can start, plus the services it skips
/// because they are in a wants cycle or wait on one. One bad cycle never
/// keeps `svc:sshd` down.
pub fn start_order_lenient(svcs: &[(String, Svc)]) -> (Vec<String>, Vec<String>) {
    order_and_stuck(svcs)
}

/// Why each service [`start_order_lenient`] skips is not started: it is in
/// a wants cycle (with whom), or it waits on one.
pub fn stuck_reasons(svcs: &[(String, Svc)], stuck: &[String]) -> Vec<(String, String)> {
    let wants: HashMap<&str, Vec<String>> = svcs
        .iter()
        .filter(|(id, _)| stuck.contains(id))
        .map(|(id, s)| {
            let w = s.wants.iter().map(|w| normalize_want(w)).filter(|w| stuck.contains(w));
            (id.as_str(), w.collect())
        })
        .collect();
    // What each skipped service reaches through skipped wants.
    let reach = |id: &str| -> HashSet<String> {
        let mut seen = HashSet::new();
        let mut todo = wants.get(id).cloned().unwrap_or_default();
        while let Some(w) = todo.pop() {
            if seen.insert(w.clone()) {
                todo.extend(wants.get(w.as_str()).cloned().unwrap_or_default());
            }
        }
        seen
    };
    let reaches: HashMap<&str, HashSet<String>> =
        stuck.iter().map(|id| (id.as_str(), reach(id))).collect();
    let cyclic = |id: &str| reaches[id].contains(id);
    stuck
        .iter()
        .map(|id| {
            let mut with: Vec<&str> = stuck
                .iter()
                .filter(|w| *w != id && reaches[id.as_str()].contains(*w))
                .filter(|w| reaches[w.as_str()].contains(id) || !cyclic(id) && cyclic(w))
                .map(String::as_str)
                .collect();
            with.sort();
            let why = if !cyclic(id) {
                format!("waits on a wants cycle among {}", with.join(", "))
            } else if with.is_empty() {
                "wants itself".to_string()
            } else {
                format!("wants cycle with {}", with.join(", "))
            };
            (id.clone(), why)
        })
        .collect()
}

/// Running services grouped for shutdown: the first group wants nothing
/// that anyone in a later group wants, so each group stops before what it
/// wants. Within a group the ids are sorted; a cycle lands in one group.
pub fn stop_layers(svcs: &[(String, Svc)]) -> Vec<Vec<String>> {
    let ids: HashSet<&str> = svcs.iter().map(|(id, _)| id.as_str()).collect();
    let mut depth: HashMap<&str, usize> = svcs.iter().map(|(id, _)| (id.as_str(), 0)).collect();
    // Longest wants chain below each service; n rounds settle any DAG and
    // bound a cycle.
    for _ in 0..svcs.len() {
        let mut changed = false;
        for (id, spec) in svcs {
            for w in &spec.wants {
                let w = normalize_want(w);
                let Some(w) = ids.get(w.as_str()) else { continue };
                let d = (depth[w] + 1).min(svcs.len());
                if d > depth[id.as_str()] {
                    depth.insert(id.as_str(), d);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let top = depth.values().copied().max().unwrap_or(0);
    (0..=top)
        .rev()
        .map(|d| {
            let mut layer: Vec<String> =
                depth.iter().filter(|(_, v)| **v == d).map(|(id, _)| id.to_string()).collect();
            layer.sort();
            layer
        })
        .filter(|l| !l.is_empty())
        .collect()
}

fn order_and_stuck(svcs: &[(String, Svc)]) -> (Vec<String>, Vec<String>) {
    let enabled: HashMap<String, &Svc> =
        svcs.iter().filter(|(_, s)| s.enabled).map(|(id, s)| (id.clone(), s)).collect();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut indeg: HashMap<String, usize> = HashMap::new();
    for id in enabled.keys() {
        adj.insert(id.clone(), Vec::new());
        indeg.insert(id.clone(), 0);
    }
    for (id, spec) in &enabled {
        let mut seen = HashSet::new();
        for w in &spec.wants {
            let w = normalize_want(w);
            if !enabled.contains_key(&w) || !seen.insert(w.clone()) {
                continue;
            }
            adj.get_mut(&w).expect("adj").push(id.clone());
            *indeg.get_mut(id).expect("indeg") += 1;
        }
    }
    let mut q: VecDeque<String> =
        indeg.iter().filter(|(_, d)| **d == 0).map(|(id, _)| id.clone()).collect();
    q.make_contiguous().sort();
    let mut out = Vec::new();
    while let Some(id) = q.pop_front() {
        out.push(id.clone());
        let mut nxt = adj.remove(&id).unwrap_or_default();
        nxt.sort();
        for n in nxt {
            if let Some(d) = indeg.get_mut(&n) {
                *d -= 1;
                if *d == 0 {
                    q.push_back(n);
                }
            }
        }
    }
    let mut stuck: Vec<String> = enabled.keys().filter(|id| !out.contains(id)).cloned().collect();
    stuck.sort();
    (out, stuck)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::kinds::SvcRestart;

    fn s(wants: &[&str]) -> Svc {
        Svc {
            exec: vec!["/bin/true".into()],
            wants: wants.iter().map(|w| w.to_string()).collect(),
            restart: SvcRestart::Never,
            enabled: true,
            user: None,
            env: BTreeMap::new(),
            bind_low_ports: false,
            keep_children: false,
            rtprio: None,
            seat: None,
        }
    }

    fn off() -> Svc {
        let mut x = s(&[]);
        x.enabled = false;
        x
    }

    #[test]
    fn serial_before_hold() {
        let svcs = vec![("svc:hold".into(), s(&["svc:serial"])), ("svc:serial".into(), s(&[]))];
        assert_eq!(start_order(&svcs).unwrap(), vec!["svc:serial", "svc:hold"]);
    }

    #[test]
    fn cycle_errors() {
        let svcs = vec![("svc:a".into(), s(&["svc:b"])), ("svc:b".into(), s(&["svc:a"]))];
        assert!(start_order(&svcs).unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn disabled_want_ignored() {
        let svcs = vec![("svc:a".into(), s(&["svc:b"])), ("svc:b".into(), off())];
        assert_eq!(start_order(&svcs).unwrap(), vec!["svc:a"]);
    }

    #[test]
    fn lenient_order_skips_only_the_cycle() {
        let svcs = vec![
            ("svc:a".into(), s(&["svc:b"])),
            ("svc:b".into(), s(&["svc:a"])),
            ("svc:c".into(), s(&["svc:a"])),
            ("svc:sshd".into(), s(&[])),
            ("svc:self".into(), s(&["svc:self"])),
        ];
        assert!(start_order(&svcs).is_err());
        let (order, stuck) = start_order_lenient(&svcs);
        assert_eq!(order, vec!["svc:sshd"]);
        assert_eq!(stuck, vec!["svc:a", "svc:b", "svc:c", "svc:self"]);
        let why: HashMap<String, String> = stuck_reasons(&svcs, &stuck).into_iter().collect();
        assert!(why["svc:a"].starts_with("wants cycle with"), "{}", why["svc:a"]);
        assert!(why["svc:c"].starts_with("waits on a wants cycle"), "{}", why["svc:c"]);
        assert_eq!(why["svc:a"], "wants cycle with svc:b");
        assert_eq!(why["svc:c"], "waits on a wants cycle among svc:a, svc:b");
        assert_eq!(why["svc:self"], "wants itself");
    }

    #[test]
    fn stop_layers_take_wanters_first() {
        let svcs = vec![
            ("svc:app".into(), s(&["svc:db", "svc:log"])),
            ("svc:db".into(), s(&["svc:log"])),
            ("svc:log".into(), s(&[])),
            ("svc:sshd".into(), s(&[])),
            ("svc:x".into(), s(&["svc:gone"])),
        ];
        assert_eq!(
            stop_layers(&svcs),
            vec![vec!["svc:app"], vec!["svc:db"], vec!["svc:log", "svc:sshd", "svc:x"]]
        );
        let cyc = vec![("svc:a".into(), s(&["svc:b"])), ("svc:b".into(), s(&["svc:a"]))];
        assert_eq!(stop_layers(&cyc).concat().len(), 2);
    }
}
