// SPDX-FileCopyrightText: © 2022 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/util.zig for Oath, 2026-10-18.
//
// util.gpa (std.heap.c_allocator) is plain malloc/calloc/free in the C port.

#include "rivulet.h"

struct timespec rv_util_timestamp(void) {
	struct timespec timespec;
	if (clock_gettime(CLOCK_MONOTONIC, &timespec) != 0) {
		// Zig: @panic("CLOCK_MONOTONIC not supported")
		rv_err(RV_SCOPE_DEFAULT, "CLOCK_MONOTONIC not supported");
		abort();
	}
	return timespec;
}

uint32_t rv_util_msec_timestamp(void) {
	const struct timespec now = rv_util_timestamp();
	// 2^32-1 milliseconds is ~50 days, which is a realistic uptime.
	// This means that we must wrap if the monotonic time is greater than
	// 2^32-1 milliseconds and hope that clients don't get too confused.
	// Wrapping i64 arithmetic (*% and +%), then @rem by maxInt(u32).
	const int64_t ms = (int64_t)((uint64_t)now.tv_sec * 1000u + (uint64_t)(now.tv_nsec / 1000000));
	const int64_t rem = ms % (int64_t)UINT32_MAX;
	// Zig's @intCast to u32 checks this.
	assert(rem >= 0);
	return (uint32_t)rem;
}
