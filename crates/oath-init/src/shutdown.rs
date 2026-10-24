//! Shutdown on request. `reboot` sends SIGTERM, `halt` SIGUSR1, `poweroff`
//! SIGUSR2 (those commands are this binary, see [`command_main`]);
//! Ctrl-Alt-Del sends SIGINT. `oath` power changes send the same
//! signals. PID 1 stops every service, kills what is left, syncs,
//! unmounts `host:local.mounts`, remounts `/` read-only, then reboots.

use std::sync::atomic::{AtomicI32, Ordering};
use std::time::{Duration, Instant};

use nix::mount::{mount, umount2, MntFlags, MsFlags};
use nix::sys::reboot::{reboot, RebootMode};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use oath_core::tel;
use serde_json::json;

use crate::log;
use crate::supervise::Supervisor;

static REQUEST: AtomicI32 = AtomicI32::new(0);

#[derive(Clone, Copy, Debug)]
pub enum Mode {
    Reboot,
    Halt,
    PowerOff,
}

/// `reboot`, `halt` and `poweroff` (R18): ask PID 1, which stops every
/// service first. `-f` does not ask: it syncs and calls the kernel.
pub fn command_main(name: &str, args: &[String]) -> i32 {
    let force = match args {
        [] => false,
        [f] if f == "-f" || f == "--force" => true,
        _ => {
            eprintln!("{name}: usage: {name} [-f]");
            return 1;
        }
    };
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("{name}: needs root: sudo {name}");
        return 1;
    }
    let (sig, cmd) = match name {
        "reboot" => (libc::SIGTERM, libc::RB_AUTOBOOT),
        "halt" => (libc::SIGUSR1, libc::RB_HALT_SYSTEM),
        _ => (libc::SIGUSR2, libc::RB_POWER_OFF),
    };
    if force {
        unsafe {
            libc::sync();
            libc::reboot(cmd);
        }
    } else if unsafe { libc::kill(1, sig) } == 0 {
        return 0;
    }
    eprintln!("{name}: {}", std::io::Error::last_os_error());
    1
}

extern "C" fn on_signal(sig: libc::c_int) {
    REQUEST.store(sig, Ordering::SeqCst);
}

/// Catch the shutdown signals. Ctrl-Alt-Del becomes SIGINT, not an
/// instant reboot.
pub fn install() {
    let handler = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
    unsafe {
        for sig in [libc::SIGTERM, libc::SIGINT, libc::SIGUSR1, libc::SIGUSR2] {
            libc::signal(sig, handler);
        }
        libc::reboot(libc::LINUX_REBOOT_CMD_CAD_OFF);
    }
}

pub fn requested() -> Option<Mode> {
    match REQUEST.load(Ordering::SeqCst) {
        0 => None,
        libc::SIGUSR1 => Some(Mode::Halt),
        libc::SIGUSR2 => Some(Mode::PowerOff),
        _ => Some(Mode::Reboot),
    }
}

pub fn finish(sup: &mut Supervisor, mode: Mode) -> ! {
    log(&format!("shutdown: {mode:?}"));
    tel("init", "shutdown", json!({ "mode": format!("{mode:?}") }));
    sup.stop_all();
    // Logins, orphans and anything outside a service cgroup.
    unsafe {
        libc::kill(-1, libc::SIGTERM);
    }
    reap_for(Duration::from_secs(2));
    unsafe {
        libc::kill(-1, libc::SIGKILL);
    }
    reap_for(Duration::from_millis(200));
    unsafe {
        libc::sync();
    }
    // host:local.mounts, deepest first; nothing uses them now.
    for path in crate::host_mounts().iter().rev() {
        if umount2(path.as_str(), MntFlags::empty()).is_err() {
            let _ = umount2(path.as_str(), MntFlags::MNT_DETACH);
        }
    }
    let _ = umount2(oath_core::BTRFS_TOP, MntFlags::MNT_DETACH);
    if let Err(e) = mount(
        None::<&str>,
        "/",
        None::<&str>,
        MsFlags::MS_REMOUNT | MsFlags::MS_RDONLY,
        None::<&str>,
    ) {
        log(&format!("remount / read-only: {e}"));
    }
    unsafe {
        libc::sync();
    }
    let how = match mode {
        Mode::Reboot => RebootMode::RB_AUTOBOOT,
        Mode::Halt => RebootMode::RB_HALT_SYSTEM,
        Mode::PowerOff => RebootMode::RB_POWER_OFF,
    };
    let _ = reboot(how);
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn reap_for(wait: Duration) {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        match waitpid(Pid::from_raw(-1), Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => std::thread::sleep(Duration::from_millis(20)),
            Ok(_) => {}
            Err(_) => return,
        }
    }
}
