// SPDX-FileCopyrightText: © 2026 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/XkbBindingsSeat.zig for Oath, 2026-10-18.

#include "rivulet.h"

static const struct rv_xkb_bindings_seat_requested requested_init = {
	.next_key_change = RV_XKB_BINDINGS_SEAT_REQUESTED_NEXT_KEY_CHANGE_NONE,
	.mods_watched = 0,
};

static const struct river_xkb_bindings_seat_v1_interface bindings_seat_impl;
static const struct river_xkb_bindings_seat_v1_interface bindings_seat_inert_impl;
static void handle_destroy(struct wl_resource *object);

void rv_xkb_bindings_seat_create_object(struct rv_xkb_bindings_seat *bindings_seat, struct wl_client *client,
		uint32_t version, uint32_t id) {
	assert(bindings_seat->object == NULL);
	bindings_seat->object = wl_resource_create(client, &river_xkb_bindings_seat_v1_interface, version, id);
	if (bindings_seat->object == NULL) {
		wl_client_post_no_memory(client);
		return;
	}
	wl_resource_set_implementation(bindings_seat->object, &bindings_seat_impl, bindings_seat, handle_destroy);
}

static void destroy_(struct rv_xkb_bindings_seat *bindings_seat) {
	bindings_seat->object = NULL;
	bindings_seat->requested = requested_init;
}

void rv_xkb_bindings_seat_make_inert(struct rv_xkb_bindings_seat *bindings_seat) {
	if (bindings_seat->object != NULL) {
		wl_resource_set_implementation(bindings_seat->object, &bindings_seat_inert_impl, NULL, NULL);
		destroy_(bindings_seat);
	}
}

static void request_inert_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

static void request_inert_ensure_next_key_eaten(struct wl_client *client, struct wl_resource *object) {
}

static void request_inert_cancel_ensure_next_key_eaten(struct wl_client *client, struct wl_resource *object) {
}

static void request_inert_modifiers_watch(struct wl_client *client, struct wl_resource *object, uint32_t modifiers) {
}

static const struct river_xkb_bindings_seat_v1_interface bindings_seat_inert_impl = {
	.destroy = request_inert_destroy,
	.ensure_next_key_eaten = request_inert_ensure_next_key_eaten,
	.cancel_ensure_next_key_eaten = request_inert_cancel_ensure_next_key_eaten,
	.modifiers_watch = request_inert_modifiers_watch,
};

static void handle_destroy(struct wl_resource *object) {
	struct rv_xkb_bindings_seat *bindings_seat = wl_resource_get_user_data(object);
	destroy_(bindings_seat);
}

static struct rv_xkb_bindings_seat *get_bindings_seat(struct wl_resource *object) {
	struct rv_xkb_bindings_seat *bindings_seat = wl_resource_get_user_data(object);
	assert(bindings_seat->object == object);
	return bindings_seat;
}

static void request_destroy(struct wl_client *client, struct wl_resource *object) {
	(void)get_bindings_seat(object);
	wl_resource_destroy(object);
}

static void request_ensure_next_key_eaten(struct wl_client *client, struct wl_resource *object) {
	struct rv_xkb_bindings_seat *bindings_seat = get_bindings_seat(object);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	bindings_seat->requested.next_key_change = RV_XKB_BINDINGS_SEAT_REQUESTED_NEXT_KEY_CHANGE_ENSURE_EATEN;
}

static void request_cancel_ensure_next_key_eaten(struct wl_client *client, struct wl_resource *object) {
	struct rv_xkb_bindings_seat *bindings_seat = get_bindings_seat(object);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	bindings_seat->requested.next_key_change = RV_XKB_BINDINGS_SEAT_REQUESTED_NEXT_KEY_CHANGE_CANCEL_ENSURE_EATEN;
}

static void request_modifiers_watch(struct wl_client *client, struct wl_resource *object, uint32_t modifiers) {
	struct rv_xkb_bindings_seat *bindings_seat = get_bindings_seat(object);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	bindings_seat->requested.mods_watched = modifiers;
}

static const struct river_xkb_bindings_seat_v1_interface bindings_seat_impl = {
	.destroy = request_destroy,
	.ensure_next_key_eaten = request_ensure_next_key_eaten,
	.cancel_ensure_next_key_eaten = request_cancel_ensure_next_key_eaten,
	.modifiers_watch = request_modifiers_watch,
};

void rv_xkb_bindings_seat_manage_start(struct rv_xkb_bindings_seat *bindings_seat) {
	if (bindings_seat->scheduled.ate_unbound_key) {
		if (bindings_seat->object != NULL) {
			if (wl_resource_get_version(bindings_seat->object) >= 2) {
				river_xkb_bindings_seat_v1_send_ate_unbound_key(bindings_seat->object);
			}
		}
		bindings_seat->scheduled.ate_unbound_key = false;
	}
	if (bindings_seat->scheduled.has_mods_update) {
		struct rv_xkb_bindings_seat_scheduled_mods_update mods = bindings_seat->scheduled.mods_update;
		if (bindings_seat->object != NULL) {
			if (wl_resource_get_version(bindings_seat->object) >= 3) {
				river_xkb_bindings_seat_v1_send_modifiers_update(bindings_seat->object, mods.old, mods.new);
			}
		}
		bindings_seat->scheduled.has_mods_update = false;
	}
}

void rv_xkb_bindings_seat_manage_finish(struct rv_xkb_bindings_seat *bindings_seat) {
	switch (bindings_seat->requested.next_key_change) {
	case RV_XKB_BINDINGS_SEAT_REQUESTED_NEXT_KEY_CHANGE_NONE:
		break;
	case RV_XKB_BINDINGS_SEAT_REQUESTED_NEXT_KEY_CHANGE_ENSURE_EATEN:
		bindings_seat->ensure_next_key_eaten = true;
		break;
	case RV_XKB_BINDINGS_SEAT_REQUESTED_NEXT_KEY_CHANGE_CANCEL_ENSURE_EATEN:
		bindings_seat->ensure_next_key_eaten = false;
		break;
	}
	bindings_seat->requested.next_key_change = RV_XKB_BINDINGS_SEAT_REQUESTED_NEXT_KEY_CHANGE_NONE;
}
