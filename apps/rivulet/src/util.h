// SPDX-FileCopyrightText: © 2022 The River Developers
// SPDX-FileCopyrightText: © 2025 Isaac Freund (slotmap declarations, 0BSD)
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/util.zig, common/slotmap.zig and the logging part of
// river/main.zig for Oath, 2026-10-18.
//
// Owned by the core area. Included only from rivulet.h.

#ifndef RV_UTIL_H
#define RV_UTIL_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

// ---------------------------------------------------------------------------
// Logging (main.zig logFn, std.log.scoped)
// ---------------------------------------------------------------------------

// std.log.Level, same order: a message is printed if level <= the runtime level.
enum rv_log_level {
	RV_LOG_ERR = 0, // printed as "error"
	RV_LOG_WARN = 1, // printed as "warning"
	RV_LOG_INFO = 2, // printed as "info"
	RV_LOG_DEBUG = 3, // printed as "debug"
};

// main.zig LogScope, same order and names (used by -log-scopes).
// Scopes should be added to this list sparingly.
// Only add new scopes if filtering based on them would be meaningful.
// Unscoped std.log calls use RV_SCOPE_DEFAULT.
enum rv_log_scope {
	RV_SCOPE_DEFAULT,
	RV_SCOPE_WLROOTS,
	RV_SCOPE_OUTPUT,
	RV_SCOPE_INPUT,
	RV_SCOPE_LOCK,
	RV_SCOPE_WM,
	RV_SCOPE_XDG,
	RV_SCOPE_XWAYLAND,
};
#define RV_SCOPE_COUNT 8

// main.zig logFn. Defined in main.c. Drops the message if level is above the
// runtime level (-log-level, default info) or if scope != RV_SCOPE_DEFAULT and
// the scope is not in the -log-scopes set (default: all). Otherwise prints
// "<level text><prefix><message>\n" to stderr, where prefix is ": " for the
// default scope and "(<scope name>): " otherwise, e.g. "warning(wm): msg".
void rv_log(enum rv_log_level level, enum rv_log_scope scope, const char *fmt, ...)
	__attribute__((format(printf, 3, 4)));

// rv_err(RV_SCOPE_WM, "out of memory") is std.log.scoped(.wm).err("out of memory", .{}).
#define rv_err(scope, ...) rv_log(RV_LOG_ERR, (scope), __VA_ARGS__)
#define rv_warn(scope, ...) rv_log(RV_LOG_WARN, (scope), __VA_ARGS__)
#define rv_info(scope, ...) rv_log(RV_LOG_INFO, (scope), __VA_ARGS__)
#define rv_debug(scope, ...) rv_log(RV_LOG_DEBUG, (scope), __VA_ARGS__)

// ---------------------------------------------------------------------------
// util.zig
// ---------------------------------------------------------------------------

// util.zig timestamp(): clock_gettime(CLOCK_MONOTONIC); on failure
// @panic("CLOCK_MONOTONIC not supported") (log and abort()).
struct timespec rv_util_timestamp(void);

// util.zig msecTimestamp(): (sec * 1000 + nsec / 1000000) with wrapping
// arithmetic, then % (2^32 - 1). 2^32-1 milliseconds is ~50 days, which is a
// realistic uptime, so we must wrap and hope clients don't get too confused.
uint32_t rv_util_msec_timestamp(void);

// ---------------------------------------------------------------------------
// Generic growable array: the C form of an owned std.ArrayList(T) /
// std.ArrayListUnmanaged(T). Declare a field as RV_ARRAY(T) name;
// zero-initialised is empty.
// ---------------------------------------------------------------------------

#define RV_ARRAY(T) struct { T *items; size_t len, cap; }

// Ensures room for `need` items. Returns false on OOM, leaving the array unchanged.
// items_field is the address of the array's `items` member (any T **).
static inline bool rv_array_reserve_(void *items_field, size_t *cap, size_t need, size_t size) {
	if (need <= *cap) {
		return true;
	}
	size_t new_cap = *cap > 0 ? *cap * 2 : 8;
	while (new_cap < need) {
		new_cap *= 2;
	}
	void *items;
	memcpy(&items, items_field, sizeof(items));
	void *new_items = realloc(items, new_cap * size);
	if (new_items == NULL) {
		return false;
	}
	memcpy(items_field, &new_items, sizeof(new_items));
	*cap = new_cap;
	return true;
}

// bool rv_array_reserve(RV_ARRAY(T) *arr, size_t need): false on OOM.
#define rv_array_reserve(arr, need) \
	rv_array_reserve_(&(arr)->items, &(arr)->cap, (need), sizeof(*(arr)->items))
// bool rv_array_append(RV_ARRAY(T) *arr, T value): false on OOM (array unchanged).
#define rv_array_append(arr, value) \
	(rv_array_reserve((arr), (arr)->len + 1) && ((arr)->items[(arr)->len++] = (value), true))
// Removes item i by moving the last item into its place (std swapRemove order).
#define rv_array_swap_remove(arr, i) ((arr)->items[(i)] = (arr)->items[--(arr)->len])
// Frees the buffer and leaves the array empty (deinit).
#define rv_array_finish(arr) \
	(free((arr)->items), (arr)->items = NULL, (arr)->len = 0, (arr)->cap = 0)

// ---------------------------------------------------------------------------
// common/slotmap.zig: SlotMap(T) with T a non-null pointer, stored as void *.
// Defined in slotmap.c (0BSD). Iteration visits slots in index order.
// ---------------------------------------------------------------------------

// SlotMap(T).Key. Packed in Zig "just to make == work": compare with rv_slotmap_key_eql().
struct rv_slotmap_key {
	uint32_t generation;
	uint32_t index;
};

static inline bool rv_slotmap_key_eql(struct rv_slotmap_key a, struct rv_slotmap_key b) {
	return a.generation == b.generation && a.index == b.index;
}

enum rv_slotmap_slot_data_tag {
	RV_SLOTMAP_SLOT_DATA_VALUE,
	RV_SLOTMAP_SLOT_DATA_NEXT_FREE,
};

// SlotMap(T).Slot.data: union(enum) { value: T, next_free: u32 }
struct rv_slotmap_slot_data {
	enum rv_slotmap_slot_data_tag tag;
	union {
		void *value;
		// Index of the next free slot or slots.items.len + 1 if
		// this is the last free slot.
		uint32_t next_free;
	};
};

// SlotMap(T).Slot (file-private in Zig)
struct rv_slotmap_slot {
	uint32_t generation;
	struct rv_slotmap_slot_data data;
};

struct rv_slotmap {
	RV_ARRAY(struct rv_slotmap_slot) slots;
	// Number of values stored in the map.
	uint32_t count;
	// Index of the first free slot or slots.items.len + 1 if there
	// is no free slot.
	uint32_t first_free; // default: 1 (via rv_slotmap_empty)
};

// SlotMap(T).empty: { .slots = empty, .count = 0, .first_free = 1 }.
// NOT all-zero: assign this to every SlotMap field defaulted to .empty.
extern const struct rv_slotmap rv_slotmap_empty;

void rv_slotmap_deinit(struct rv_slotmap *map);
// Zig put() returns error{OutOfMemory}!Key: returns false on OOM, else stores the key in *key.
bool rv_slotmap_put(struct rv_slotmap *map, void *value, struct rv_slotmap_key *key);
// Returns NULL (Zig null) if the key is stale or out of range.
void *rv_slotmap_get(struct rv_slotmap *map, struct rv_slotmap_key key);
void rv_slotmap_remove(struct rv_slotmap *map, struct rv_slotmap_key key);

// SlotMap(T).Iterator
struct rv_slotmap_iterator {
	struct rv_slotmap *map;
	uint32_t index;
};

// Removing values from the map during iteration is safe.
// Adding values to the map during iteration is safe but there is
// no guarantee whether or not values added during iteration will
// be seen by the iterator.
struct rv_slotmap_iterator rv_slotmap_iterator(struct rv_slotmap *map);
// Returns NULL when done.
void *rv_slotmap_iterator_next(struct rv_slotmap_iterator *it);

#endif
