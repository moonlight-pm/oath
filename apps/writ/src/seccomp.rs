//! The box's seccomp filter, assembled by hand. It allows by default, since toolchains
//! need a wide surface, and refuses what a build has no use for and where the kernel's own
//! bugs are likeliest to be reached. A filter cannot read a pointer, so paths are the mount
//! plan's and Landlock's job; `clone3` hides its flags behind one, so it answers as a kernel
//! that lacks it and libc goes back to `clone`, which can be read.
//!
//! An argument the kernel truncates to 32 bits is compared on its low word only: comparing
//! all 64 would let high bits walk past the rule.

use crate::sys;
use std::ffi::c_long;

/// x86_64 numbers from the kernel's table. Each returns EPERM, and `writ check` calls each.
pub const REFUSED: &[(&str, c_long)] = &[
    // Their own attack surface, and a way around a filter.
    ("io_uring_setup", 425),
    ("io_uring_enter", 426),
    ("io_uring_register", 427),
    ("bpf", 321),
    ("userfaultfd", 323),
    ("perf_event_open", 298),
    // Other processes.
    ("ptrace", 101),
    ("process_vm_readv", 310),
    ("process_vm_writev", 311),
    ("process_madvise", 440),
    ("kcmp", 312),
    ("pidfd_getfd", 438),
    // The session keyring is shared with the login that started writ.
    ("add_key", 248),
    ("keyctl", 250),
    ("request_key", 249),
    // Namespaces and mounts: the box's shape is fixed before this filter loads.
    ("unshare", 272),
    ("setns", 308),
    ("mount", 165),
    ("umount2", 166),
    ("pivot_root", 155),
    ("chroot", 161),
    ("open_tree", 428),
    ("open_tree_attr", 467),
    ("move_mount", 429),
    ("fsopen", 430),
    ("fsconfig", 431),
    ("fsmount", 432),
    ("fspick", 433),
    ("mount_setattr", 442),
    ("listmount", 458),
    ("statmount", 457),
    // By-handle opens walk past a mount's root.
    ("name_to_handle_at", 303),
    ("open_by_handle_at", 304),
    // The machine itself.
    ("init_module", 175),
    ("finit_module", 313),
    ("delete_module", 176),
    ("kexec_load", 246),
    ("kexec_file_load", 320),
    ("reboot", 169),
    ("swapon", 167),
    ("swapoff", 168),
    ("acct", 163),
    ("quotactl", 179),
    ("quotactl_fd", 443),
    ("settimeofday", 164),
    ("clock_settime", 227),
    ("clock_adjtime", 305),
    ("adjtimex", 159),
    ("ioperm", 173),
    ("iopl", 172),
    ("modify_ldt", 154),
    ("uselib", 134),
    ("sysfs", 139),
    ("ustat", 136),
    ("vhangup", 153),
];

/// Every `CLONE_NEW*` flag `clone` can carry. `CLONE_NEWTIME` shares a bit with the exit
/// signal there, and only `clone3` and `unshare` can use it.
const CLONE_NEW: u32 = 0x0002_0000 // NEWNS
    | 0x0200_0000 // NEWCGROUP
    | 0x0400_0000 // NEWUTS
    | 0x0800_0000 // NEWIPC
    | 0x1000_0000 // NEWUSER
    | 0x2000_0000 // NEWPID
    | 0x4000_0000; // NEWNET

/// TIOCSTI, TIOCLINUX, TIOCCONS, TIOCSETD: typing into a terminal, and taking it over.
const TERMINAL_IOCTLS: &[u32] = &[0x5412, 0x541c, 0x541d, 0x5423];

/// What `personality` may set: Docker's list, which leaves out turning off address
/// randomization. 0xffffffff only asks.
const PERSONALITIES: &[u32] = &[0, 0x0008, 0x0002_0000, 0x0002_0008, 0xffff_ffff];

const AUDIT_ARCH_X86_64: u32 = 0xc000_003e;
const X32_SYSCALL_BIT: u32 = 0x4000_0000;

const RET_KILL_PROCESS: u32 = 0x8000_0000;
const RET_ERRNO: u32 = 0x0005_0000;
const RET_ALLOW: u32 = 0x7fff_0000;

const LD_ABS: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS
const JEQ: u16 = 0x15; // BPF_JMP | BPF_JEQ | BPF_K
const JGE: u16 = 0x35; // BPF_JMP | BPF_JGE | BPF_K
const JSET: u16 = 0x45; // BPF_JMP | BPF_JSET | BPF_K
const RET: u16 = 0x06; // BPF_RET | BPF_K

// Offsets into struct seccomp_data.
const NR: u32 = 0;
const ARCH: u32 = 4;
/// The low word of a syscall's argument, on a little-endian machine.
const fn arg(i: u32) -> u32 {
    16 + 8 * i
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Insn {
    pub code: u16,
    pub jt: u8,
    pub jf: u8,
    pub k: u32,
}

#[repr(C)]
struct Fprog {
    len: u16,
    filter: *const Insn,
}

fn load(offset: u32) -> Insn {
    Insn { code: LD_ABS, jt: 0, jf: 0, k: offset }
}

fn ret(k: u32) -> Insn {
    Insn { code: RET, jt: 0, jf: 0, k }
}

fn jump(code: u16, k: u32, jt: u8, jf: u8) -> Insn {
    Insn { code, jt, jf, k }
}

fn errno(e: i32) -> u32 {
    RET_ERRNO | e as u32
}

/// Instructions that run for one syscall only, and must end in a return.
fn only(p: &mut Vec<Insn>, nr: c_long, body: Vec<Insn>) {
    assert!(body.len() <= u8::MAX as usize);
    assert_eq!(body.last().map(|i| i.code), Some(RET));
    p.push(jump(JEQ, nr as u32, 0, body.len() as u8));
    p.extend(body);
}

/// Allow when the loaded value is one of `values`, or else return `otherwise`.
fn one_of(values: &[u32], otherwise: u32) -> Vec<Insn> {
    let mut b = Vec::new();
    for (i, v) in values.iter().enumerate() {
        // Taken, jump past the remaining checks and the refusal to the allow.
        b.push(jump(JEQ, *v, (values.len() - i) as u8, 0));
    }
    b.push(ret(otherwise));
    b.push(ret(RET_ALLOW));
    b
}

pub fn program() -> Vec<Insn> {
    let mut p = Vec::new();

    // Another architecture has other numbers, and `int 0x80` reaches them even on x86_64.
    p.push(load(ARCH));
    p.push(jump(JEQ, AUDIT_ARCH_X86_64, 1, 0));
    p.push(ret(RET_KILL_PROCESS));
    p.push(load(NR));
    p.push(jump(JGE, X32_SYSCALL_BIT, 0, 1));
    p.push(ret(RET_KILL_PROCESS));

    for (_, nr) in REFUSED {
        only(&mut p, *nr, vec![ret(errno(sys::EPERM))]);
    }

    only(&mut p, sys::SYS_CLONE3, vec![ret(errno(sys::ENOSYS))]);

    only(
        &mut p,
        sys::SYS_CLONE,
        vec![
            load(arg(0)),
            jump(JSET, CLONE_NEW, 0, 1),
            ret(errno(sys::EPERM)),
            ret(RET_ALLOW),
        ],
    );

    let mut ioctl = vec![load(arg(1))];
    for cmd in TERMINAL_IOCTLS {
        ioctl.push(jump(JEQ, *cmd, 0, 1));
        ioctl.push(ret(errno(sys::EPERM)));
    }
    ioctl.push(ret(RET_ALLOW));
    only(&mut p, sys::SYS_IOCTL, ioctl);

    let mut personality = vec![load(arg(0))];
    personality.extend(one_of(PERSONALITIES, errno(sys::EPERM)));
    only(&mut p, sys::SYS_PERSONALITY, personality);

    // The box's network namespace is empty, so AF_INET reaches loopback and nothing else,
    // and tests that listen there still work. Netlink is for glibc's getaddrinfo.
    let mut socket = vec![
        load(arg(0)),
        jump(JEQ, sys::AF_UNIX, 7, 0),
        jump(JEQ, sys::AF_INET, 6, 0),
        jump(JEQ, sys::AF_INET6, 5, 0),
        jump(JEQ, sys::AF_NETLINK, 1, 0),
        ret(errno(sys::EAFNOSUPPORT)),
        load(arg(2)),
        jump(JEQ, sys::NETLINK_ROUTE, 1, 0),
    ];
    socket.push(ret(errno(sys::EAFNOSUPPORT)));
    socket.push(ret(RET_ALLOW));
    only(&mut p, sys::SYS_SOCKET, socket);

    let mut socketpair = vec![load(arg(0))];
    socketpair.extend(one_of(&[sys::AF_UNIX], errno(sys::EAFNOSUPPORT)));
    only(&mut p, sys::SYS_SOCKETPAIR, socketpair);

    p.push(ret(RET_ALLOW));
    p
}

/// Load the filter on this thread, for good. `no_new_privs` must already be set.
pub fn load_filter() -> Result<(), String> {
    let prog = program();
    let fprog = Fprog { len: prog.len() as u16, filter: prog.as_ptr() };
    let r = unsafe {
        sys::prctl(sys::PR_SET_SECCOMP, sys::SECCOMP_MODE_FILTER, &fprog as *const Fprog)
    };
    if r != 0 {
        return Err(format!("seccomp: errno {}", sys::errno()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The classic BPF the kernel would run, enough to test the filter's logic in-process.
    fn run(p: &[Insn], nr: u32, arch: u32, args: [u64; 6]) -> u32 {
        let word = |offset: u32| -> u32 {
            match offset {
                NR => nr,
                ARCH => arch,
                o => {
                    let i = (o - 16) / 8;
                    let v = args[i as usize];
                    if (o - 16) % 8 == 0 {
                        v as u32
                    } else {
                        (v >> 32) as u32
                    }
                }
            }
        };
        let (mut pc, mut a) = (0usize, 0u32);
        loop {
            let i = p[pc];
            pc += 1;
            match i.code {
                LD_ABS => a = word(i.k),
                RET => return i.k,
                JEQ => pc += if a == i.k { i.jt } else { i.jf } as usize,
                JGE => pc += if a >= i.k { i.jt } else { i.jf } as usize,
                JSET => pc += if a & i.k != 0 { i.jt } else { i.jf } as usize,
                c => panic!("unknown code {c:#x}"),
            }
        }
    }

    fn call(nr: c_long, args: [u64; 6]) -> u32 {
        run(&program(), nr as u32, AUDIT_ARCH_X86_64, args)
    }

    #[test]
    fn ends_in_allow_and_fits() {
        let p = program();
        assert_eq!(p.last().unwrap().k, RET_ALLOW);
        assert!(p.len() < 4096);
    }

    #[test]
    fn ordinary_calls_pass() {
        for nr in [0, 1, 2, 3, 9, 39, 59, 60, 231, 257] {
            assert_eq!(call(nr, [0; 6]), RET_ALLOW, "syscall {nr}");
        }
    }

    #[test]
    fn every_refused_call_is_refused() {
        for (name, nr) in REFUSED {
            assert_eq!(call(*nr, [0; 6]), RET_ERRNO | 1, "{name}");
        }
    }

    #[test]
    fn other_architectures_and_x32_die() {
        let p = program();
        assert_eq!(run(&p, 20, 0x4000_0003, [0; 6]), RET_KILL_PROCESS);
        assert_eq!(run(&p, 39 | X32_SYSCALL_BIT, AUDIT_ARCH_X86_64, [0; 6]), RET_KILL_PROCESS);
    }

    #[test]
    fn clone3_looks_absent_and_clone_cannot_make_a_namespace() {
        assert_eq!(call(sys::SYS_CLONE3, [0; 6]), RET_ERRNO | sys::ENOSYS as u32);
        // What glibc's pthread_create and fork pass.
        assert_eq!(call(sys::SYS_CLONE, [0x003d_0f00, 0, 0, 0, 0, 0]), RET_ALLOW);
        assert_eq!(call(sys::SYS_CLONE, [0x0120_0011, 0, 0, 0, 0, 0]), RET_ALLOW);
        for flag in [0x0002_0000u64, 0x0200_0000, 0x0400_0000, 0x0800_0000, 0x1000_0000, 0x2000_0000, 0x4000_0000] {
            assert_eq!(call(sys::SYS_CLONE, [flag | 17, 0, 0, 0, 0, 0]), RET_ERRNO | 1, "{flag:#x}");
        }
    }

    #[test]
    fn terminal_ioctls_are_refused_even_with_high_bits() {
        assert_eq!(call(sys::SYS_IOCTL, [0, 0x5401, 0, 0, 0, 0]), RET_ALLOW);
        assert_eq!(call(sys::SYS_IOCTL, [0, 0x5412, 0, 0, 0, 0]), RET_ERRNO | 1);
        assert_eq!(call(sys::SYS_IOCTL, [0, 0xffff_ffff_0000_5412, 0, 0, 0, 0]), RET_ERRNO | 1);
    }

    #[test]
    fn socket_families_are_an_allowlist() {
        let e = RET_ERRNO | sys::EAFNOSUPPORT as u32;
        for f in [1u64, 2, 10] {
            assert_eq!(call(sys::SYS_SOCKET, [f, 1, 0, 0, 0, 0]), RET_ALLOW, "family {f}");
        }
        assert_eq!(call(sys::SYS_SOCKET, [16, 3, 0, 0, 0, 0]), RET_ALLOW);
        assert_eq!(call(sys::SYS_SOCKET, [16, 3, 12, 0, 0, 0]), e);
        for f in [38u64, 40, 17, 15, 44, 21, 30, 31, 0x1_0000_0026] {
            assert_eq!(call(sys::SYS_SOCKET, [f, 2, 0, 0, 0, 0]), e, "family {f:#x}");
        }
        assert_eq!(call(sys::SYS_SOCKETPAIR, [1, 1, 0, 0, 0, 0]), RET_ALLOW);
        assert_eq!(call(sys::SYS_SOCKETPAIR, [2, 1, 0, 0, 0, 0]), e);
    }

    #[test]
    fn personality_cannot_turn_off_randomization() {
        assert_eq!(call(sys::SYS_PERSONALITY, [0xffff_ffff, 0, 0, 0, 0, 0]), RET_ALLOW);
        assert_eq!(call(sys::SYS_PERSONALITY, [0x0004_0000, 0, 0, 0, 0, 0]), RET_ERRNO | 1);
    }
}
