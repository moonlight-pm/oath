//! What writ needs from the kernel, declared by hand. x86_64 only: the numbers are the
//! kernel's `arch/x86/entry/syscalls/syscall_64.tbl`, and seccomp filters by them.

use std::ffi::{c_int, c_long};

#[cfg(not(target_arch = "x86_64"))]
compile_error!("writ's seccomp filter is written for x86_64");

extern "C" {
    pub fn syscall(num: c_long, ...) -> c_long;
    pub fn prctl(option: c_int, ...) -> c_int;
    pub fn setrlimit(resource: c_int, limit: *const Rlimit) -> c_int;
    pub fn isatty(fd: c_int) -> c_int;
    pub fn kill(pid: c_int, sig: c_int) -> c_int;
    pub fn fork() -> c_int;
    pub fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    pub fn pause() -> c_int;
    pub fn getpid() -> c_int;
    pub fn getppid() -> c_int;
    pub fn close(fd: c_int) -> c_int;
    pub fn fcntl(fd: c_int, cmd: c_int, ...) -> c_int;
    pub fn setsid() -> c_int;
    pub fn tcgetattr(fd: c_int, t: *mut Termios) -> c_int;
    pub fn tcsetattr(fd: c_int, action: c_int, t: *const Termios) -> c_int;
    pub fn cfmakeraw(t: *mut Termios);
    pub fn ioctl(fd: c_int, request: u64, ...) -> c_int;
    pub fn poll(fds: *mut Pollfd, count: u64, timeout_ms: c_int) -> c_int;
    pub fn read(fd: c_int, buf: *mut std::ffi::c_void, count: usize) -> isize;
    pub fn write(fd: c_int, buf: *const std::ffi::c_void, count: usize) -> isize;
    pub fn dup(fd: c_int) -> c_int;
    pub fn dup2(from: c_int, to: c_int) -> c_int;
    pub fn signal(signal: c_int, handler: usize) -> usize;
    pub fn getuid() -> u32;
    pub fn _exit(code: c_int) -> !;
}

#[repr(C)]
pub struct Rlimit {
    pub cur: u64,
    pub max: u64,
}

/// glibc's `struct termios` on x86_64.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Termios {
    iflag: u32,
    oflag: u32,
    cflag: u32,
    lflag: u32,
    line: u8,
    cc: [u8; 32],
    ispeed: u32,
    ospeed: u32,
}

impl Termios {
    pub fn zeroed() -> Termios {
        Termios { iflag: 0, oflag: 0, cflag: 0, lflag: 0, line: 0, cc: [0; 32], ispeed: 0, ospeed: 0 }
    }

    /// The same, but what is typed does not show.
    pub fn without_echo(mut self) -> Termios {
        self.lflag &= !ECHO;
        self
    }
}

#[repr(C)]
#[derive(Default)]
pub struct Winsize {
    pub rows: u16,
    pub cols: u16,
    pub xpixel: u16,
    pub ypixel: u16,
}

#[repr(C)]
pub struct Pollfd {
    pub fd: c_int,
    pub events: i16,
    pub revents: i16,
}

pub const TCSANOW: c_int = 0;
const ECHO: u32 = 0o10;
pub const TIOCGWINSZ: u64 = 0x5413;
pub const POLLIN: i16 = 1;
pub const SIGHUP: c_int = 1;
pub const SIGINT: c_int = 2;
pub const SIGTERM: c_int = 15;

pub const SYS_IOCTL: c_long = 16;
pub const SYS_SOCKET: c_long = 41;
pub const SYS_SOCKETPAIR: c_long = 53;
pub const SYS_CLONE: c_long = 56;
pub const SYS_PERSONALITY: c_long = 135;
pub const SYS_CLONE3: c_long = 435;
pub const SYS_CLOSE_RANGE: c_long = 436;
pub const SYS_PIDFD_SEND_SIGNAL: c_long = 424;
pub const SYS_PIDFD_OPEN: c_long = 434;
pub const SYS_LANDLOCK_CREATE_RULESET: c_long = 444;
pub const SYS_LANDLOCK_ADD_RULE: c_long = 445;
pub const SYS_LANDLOCK_RESTRICT_SELF: c_long = 446;

pub const PR_SET_PDEATHSIG: c_int = 1;
pub const PR_SET_DUMPABLE: c_int = 4;
pub const PR_GET_SECCOMP: c_int = 21;
pub const PR_SET_SECCOMP: c_int = 22;
pub const PR_SET_NO_NEW_PRIVS: c_int = 38;
pub const PR_GET_NO_NEW_PRIVS: c_int = 39;
pub const PR_CAP_AMBIENT: c_int = 47;
pub const PR_CAP_AMBIENT_CLEAR_ALL: c_long = 4;
pub const SECCOMP_MODE_FILTER: c_long = 2;

pub const RLIMIT_NPROC: c_int = 6;
pub const RLIMIT_CORE: c_int = 4;

pub const F_SETFD: c_int = 2;
pub const FD_CLOEXEC: c_int = 1;

pub const SIGKILL: c_int = 9;
pub const SIGCONT: c_int = 18;
pub const SIGSTOP: c_int = 19;
pub const CLOSE_RANGE_CLOEXEC: c_long = 4;

pub const EPERM: c_int = 1;
pub const EINTR: c_int = 4;
pub const ESRCH: c_int = 3;
pub const EAGAIN: c_int = 11;
pub const ENOSYS: c_int = 38;
pub const EAFNOSUPPORT: c_int = 97;

pub const AF_UNIX: u32 = 1;
pub const AF_INET: u32 = 2;
pub const AF_INET6: u32 = 10;
pub const AF_NETLINK: u32 = 16;
pub const NETLINK_ROUTE: u32 = 0;

/// The errno of the call that just failed.
pub fn errno() -> c_int {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// Close every descriptor above stderr when a program is executed, so nothing a parent
/// left open reaches a child.
pub fn cloexec_above_stderr() {
    unsafe {
        syscall(SYS_CLOSE_RANGE, 3usize, u32::MAX as usize, CLOSE_RANGE_CLOEXEC);
    }
}

/// Whether `fd` has something to read, or has ended, within this long. A negative time waits for it.
pub fn wait_readable(fd: c_int, timeout_ms: c_int) -> bool {
    let mut p = Pollfd { fd, events: POLLIN, revents: 0 };
    unsafe { poll(&mut p, 1, timeout_ms) > 0 }
}
