//! The probation timer. An apply that can cut off SSH is undone unless a
//! new session runs `oath keep` in time (oath-core's `probation`). PID 1
//! checks once a second and, when the time has run out (or the machine
//! booted again first), runs `oath undo --expired`, which takes the catalog
//! lock and undoes only that apply.

use std::time::{Duration, Instant};

use oath_core::{probation, tel, DEFAULT_ROOT};
use serde_json::json;

use crate::log;
use crate::supervise::Supervisor;

const TASK: &str = "probation";
const CHECK: Duration = Duration::from_secs(1);
/// After a failed rollback, wait this long before trying again.
const RETRY: Duration = Duration::from_secs(30);

#[derive(Default)]
pub struct Watch {
    next: Option<Instant>,
}

impl Watch {
    pub fn tick(&mut self, sup: &mut Supervisor) {
        let now = Instant::now();
        if let Some(ok) = sup.task_exit(TASK) {
            self.next = Some(now + if ok { CHECK } else { RETRY });
        }
        if self.next.is_some_and(|t| now < t) || sup.task_running(TASK) {
            return;
        }
        self.next = Some(now + CHECK);
        let Some(p) = probation::read(std::path::Path::new(DEFAULT_ROOT)) else { return };
        if !p.expired() {
            return;
        }
        let why = if p.boot_id == probation::boot_id() {
            format!("{} s passed without `oath keep`", p.seconds)
        } else {
            "the machine booted again before `oath keep`".to_string()
        };
        log(&format!(
            "probation: generation {} ({}): {why}; undoing",
            p.generation,
            p.ids.join(", ")
        ));
        tel("init", "probation", json!({ "generation": p.generation, "ids": p.ids }));
        sup.run_task(TASK, &["/bin/oath", "undo", "--expired"]);
    }
}
