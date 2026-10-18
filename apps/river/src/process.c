// SPDX-FileCopyrightText: © 2022 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/process.zig for Oath, 2026-10-18.

#include <signal.h>
#include <stdint.h>
#include <sys/resource.h>
#include <unistd.h>

#include "river.h"

// ?posix.rlimit, default null
static bool has_original_rlimit = false;
static struct rlimit original_rlimit;

void rv_process_setup(void) {
	// Ignore SIGPIPE so we don't get killed when writing to a socket that
	// has had its read end closed by another process.
	struct sigaction sig_ign = { 0 };
	sig_ign.sa_handler = SIG_IGN;
	sigemptyset(&sig_ign.sa_mask);
	sig_ign.sa_flags = 0;
	sigaction(SIGPIPE, &sig_ign, NULL);

	// Most unix systems have a default limit of 1024 file descriptors and it
	// seems unlikely for this default to be universally raised due to the
	// broken behavior of select() on fds with value >1024. However, it is
	// unreasonable to use such a low limit for a process such as river which
	// uses many fds in its communication with wayland clients and the kernel.
	//
	// There is however an advantage to having a relatively low limit: it helps
	// to catch any fd leaks. Therefore, don't use some crazy high limit that
	// can never be reached before the system runs out of memory. This can be
	// raised further if anyone reaches it in practice.
	struct rlimit original;
	if (getrlimit(RLIMIT_NOFILE, &original) == 0) {
		original_rlimit = original;
		has_original_rlimit = true;
		const struct rlimit new = {
			.rlim_cur = original.rlim_max < 4096 ? original.rlim_max : 4096,
			.rlim_max = original.rlim_max,
		};
		if (setrlimit(RLIMIT_NOFILE, &new) == 0) {
			rv_info(RV_SCOPE_DEFAULT, "raised file descriptor limit of the river process to %ju",
				(uintmax_t)new.rlim_cur);
		} else {
			rv_err(RV_SCOPE_DEFAULT, "setrlimit failed, using system default file descriptor limit of %ju",
				(uintmax_t)original.rlim_cur);
		}
	} else {
		rv_err(RV_SCOPE_DEFAULT, "getrlimit failed, using system default file descriptor limit ");
	}
}

void rv_process_cleanup_child(void) {
	if (setsid() < 0) {
		abort(); // unreachable
	}
	sigset_t empty;
	sigemptyset(&empty);
	if (sigprocmask(SIG_SETMASK, &empty, NULL) < 0) {
		abort(); // unreachable
	}

	struct sigaction sig_dfl = { 0 };
	sig_dfl.sa_handler = SIG_DFL;
	sigemptyset(&sig_dfl.sa_mask);
	sig_dfl.sa_flags = 0;
	sigaction(SIGPIPE, &sig_dfl, NULL);

	if (has_original_rlimit) {
		if (setrlimit(RLIMIT_NOFILE, &original_rlimit) != 0) {
			rv_err(RV_SCOPE_DEFAULT, "failed to restore original file descriptor limit for "
				"child process, setrlimit failed");
		}
	}
}
