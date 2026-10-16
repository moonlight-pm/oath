//! Probation: an apply that can cut the owner off from SSH (sshd, `ssh:local`
//! keys, `net:net0`) runs on a timer. Unless `oath keep` confirms it from a
//! session that started after the apply, PID 1 undoes it when the timer runs
//! out, or at the next boot if the machine restarts first. While one is open,
//! other applies are refused; `oath undo` takes it back at once.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::{read_json, write_json};

/// `<root>/probation.json` while a probation is open.
pub const PROBATION_FILE: &str = "probation.json";
/// Seconds an apply stays on probation unless `--probation` says otherwise.
pub const DEFAULT_SECS: u64 = 180;
pub const MIN_SECS: u64 = 10;
pub const MAX_SECS: u64 = 3600;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Probation {
    /// The apply on probation; undo of it is the rollback.
    pub generation: u64,
    pub ids: Vec<String>,
    /// When the apply finished, RFC 3339.
    pub applied: String,
    pub seconds: u64,
    /// The boot the apply ran in (`/proc/sys/kernel/random/boot_id`). In
    /// any other boot the probation has run out.
    pub boot_id: String,
    /// Seconds since boot when the apply finished: a process that started
    /// later belongs to a new session.
    pub applied_uptime: f64,
}

impl Probation {
    pub fn open(generation: u64, ids: Vec<String>, seconds: u64) -> Self {
        Self {
            generation,
            ids,
            applied: crate::now_rfc3339(),
            seconds,
            boot_id: boot_id(),
            applied_uptime: uptime(),
        }
    }

    /// Seconds left, or 0 once it has run out (or this is another boot).
    pub fn left(&self) -> u64 {
        if self.boot_id != boot_id() {
            return 0;
        }
        let end = self.applied_uptime + self.seconds as f64;
        (end - uptime()).max(0.0).ceil() as u64
    }

    pub fn expired(&self) -> bool {
        self.left() == 0
    }
}

pub fn read(root: &Path) -> Option<Probation> {
    read_json(&root.join(PROBATION_FILE)).ok()
}

pub(crate) fn write(root: &Path, p: &Probation) -> Result<()> {
    write_json(&root.join(PROBATION_FILE), p)
}

pub(crate) fn clear(root: &Path) -> Result<()> {
    match fs::remove_file(root.join(PROBATION_FILE)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// Refuse unless this process belongs to a session that started after the
/// apply: every ancestor below PID 1's own children (sshd's per-connection
/// process, the login shell, sudo) must be younger than the apply. A shell
/// that was already open when the apply ran proves nothing about whether a
/// new login works.
pub fn check_new_session(p: &Probation) -> Result<()> {
    if p.boot_id != boot_id() {
        return Ok(());
    }
    let tck = clock_ticks();
    let mut pid = std::process::id() as i32;
    for _ in 0..64 {
        let Some(st) = proc_stat(pid) else { break };
        if st.ppid <= 1 {
            break;
        }
        if (st.start_ticks as f64) / tck <= p.applied_uptime {
            return Err(Error::hint(
                format!(
                    "this session is older than the apply ({} pid {pid} started before it); \
                     a new login has to confirm it",
                    st.comm
                ),
                "open a new SSH session and run `sudo oath keep` there",
            ));
        }
        pid = st.ppid;
    }
    Ok(())
}

struct Stat {
    comm: String,
    ppid: i32,
    start_ticks: u64,
}

fn proc_stat(pid: i32) -> Option<Stat> {
    let s = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let open = s.find('(')?;
    let close = s.rfind(')')?;
    let comm = s[open + 1..close].to_string();
    // Fields after `comm`: state (3), ppid (4), … starttime (22).
    let rest: Vec<&str> = s[close + 1..].split_whitespace().collect();
    Some(Stat { comm, ppid: rest.get(1)?.parse().ok()?, start_ticks: rest.get(19)?.parse().ok()? })
}

fn clock_ticks() -> f64 {
    let t = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if t > 0 {
        t as f64
    } else {
        100.0
    }
}

pub fn boot_id() -> String {
    fs::read_to_string("/proc/sys/kernel/random/boot_id").unwrap_or_default().trim().to_string()
}

pub fn uptime() -> f64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|x| x.parse().ok()))
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_runs_out_in_another_boot() {
        let mut p = Probation::open(3, vec!["svc:sshd".into()], 60);
        assert!(!p.expired());
        assert!(p.left() > 50);
        p.boot_id = "another-boot".into();
        assert!(p.expired());
    }

    #[test]
    fn runs_out_after_its_seconds() {
        let mut p = Probation::open(3, vec![], 60);
        p.applied_uptime -= 61.0;
        assert!(p.expired());
    }

    #[test]
    fn this_process_is_older_than_a_new_apply() {
        let p = Probation::open(1, vec![], 60);
        let e = check_new_session(&p).unwrap_err();
        assert!(e.to_string().contains("older than the apply"), "{e}");
    }

    #[test]
    fn this_process_is_newer_than_an_old_apply() {
        let mut p = Probation::open(1, vec![], 60);
        p.applied_uptime = 0.0;
        check_new_session(&p).unwrap();
    }
}
