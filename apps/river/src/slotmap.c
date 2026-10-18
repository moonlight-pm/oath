// SPDX-FileCopyrightText: © 2025 Isaac Freund
// SPDX-License-Identifier: 0BSD
// Ported to C from common/slotmap.zig for Oath, 2026-10-18.
//
// SlotMap(T) with T a non-null pointer, stored as void *. The types are in util.h.

#include "river.h"

const struct rv_slotmap rv_slotmap_empty = {
	.slots = { .items = NULL, .len = 0, .cap = 0 },
	.count = 0,
	.first_free = 1,
};

void rv_slotmap_deinit(struct rv_slotmap *map) {
	rv_array_finish(&map->slots);
}

bool rv_slotmap_put(struct rv_slotmap *map, void *value, struct rv_slotmap_key *key) {
	if (map->first_free < map->slots.len) {
		uint32_t index = map->first_free;
		struct rv_slotmap_slot *slot = &map->slots.items[index];
		// Zig checks the active union field on this access.
		assert(slot->data.tag == RV_SLOTMAP_SLOT_DATA_NEXT_FREE);
		map->first_free = slot->data.next_free;
		slot->data = (struct rv_slotmap_slot_data){
			.tag = RV_SLOTMAP_SLOT_DATA_VALUE,
			.value = value,
		};
		map->count += 1;
		*key = (struct rv_slotmap_key){
			.generation = slot->generation,
			.index = index,
		};
		return true;
	}
	struct rv_slotmap_slot slot = {
		.generation = 0,
		.data = { .tag = RV_SLOTMAP_SLOT_DATA_VALUE, .value = value },
	};
	if (!rv_array_append(&map->slots, slot)) {
		return false;
	}
	map->count += 1;
	map->first_free += 1;
	*key = (struct rv_slotmap_key){
		.generation = 0,
		.index = (uint32_t)(map->slots.len - 1),
	};
	return true;
}

static struct rv_slotmap_slot *get_slot(struct rv_slotmap *map, struct rv_slotmap_key key) {
	if (key.index < map->slots.len) {
		if (key.generation == map->slots.items[key.index].generation) {
			return &map->slots.items[key.index];
		}
	}
	return NULL;
}

void *rv_slotmap_get(struct rv_slotmap *map, struct rv_slotmap_key key) {
	struct rv_slotmap_slot *slot = get_slot(map, key);
	if (slot != NULL) {
		// Zig checks the active union field on this access.
		assert(slot->data.tag == RV_SLOTMAP_SLOT_DATA_VALUE);
		return slot->data.value;
	}
	return NULL;
}

void rv_slotmap_remove(struct rv_slotmap *map, struct rv_slotmap_key key) {
	struct rv_slotmap_slot *slot = get_slot(map, key);
	if (slot != NULL) {
		assert(slot->data.tag == RV_SLOTMAP_SLOT_DATA_VALUE);
		*slot = (struct rv_slotmap_slot){
			.generation = slot->generation + 1, // wrapping
			.data = { .tag = RV_SLOTMAP_SLOT_DATA_NEXT_FREE, .next_free = map->first_free },
		};
		map->count -= 1;
		map->first_free = key.index;
	}
}

// Removing values from the map during iteration is safe.
// Adding values to the map during iteration is safe but there is
// no guarantee whether or not values added during iteration will
// be seen by the iterator.
struct rv_slotmap_iterator rv_slotmap_iterator(struct rv_slotmap *map) {
	return (struct rv_slotmap_iterator){ .map = map, .index = 0 };
}

void *rv_slotmap_iterator_next(struct rv_slotmap_iterator *it) {
	while (it->index < it->map->slots.len) {
		struct rv_slotmap_slot_data data = it->map->slots.items[it->index].data;
		it->index += 1;
		switch (data.tag) {
		case RV_SLOTMAP_SLOT_DATA_VALUE:
			return data.value;
		case RV_SLOTMAP_SLOT_DATA_NEXT_FREE:
			break;
		}
	}
	return NULL;
}
