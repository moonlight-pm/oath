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
    }
}
