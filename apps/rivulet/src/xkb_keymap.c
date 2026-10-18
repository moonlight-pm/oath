// SPDX-FileCopyrightText: © 2026 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/XkbKeymap.zig for Oath, 2026-10-18.

#include "rivulet.h"

static const struct river_xkb_keymap_v1_interface keymap_impl;
static const struct river_xkb_keymap_v1_interface keymap_inert_impl;
static void handle_destroy(struct wl_resource *object);

bool rv_xkb_keymap_create(struct wl_client *client, uint32_t version, uint32_t id, struct xkb_keymap *xkb_keymap) {
	struct rv_xkb_keymap *keymap = calloc(1, sizeof(*keymap));
	if (keymap == NULL) {
		return false;
	}

	struct wl_resource *object = wl_resource_create(client, &river_xkb_keymap_v1_interface, (int)version, id);
	if (object == NULL) {
		free(keymap);
		return false;
	}

	*keymap = (struct rv_xkb_keymap){
		.object = object,
		.xkb_keymap = xkb_keymap_ref(xkb_keymap),
	};
	wl_list_insert(server.xkb_config.keymaps.prev, &keymap->link);

	wl_resource_set_implementation(object, &keymap_impl, keymap, handle_destroy);
	river_xkb_keymap_v1_send_success(object);
	return true;
}

bool rv_xkb_keymap_create_failed(struct wl_client *client, uint32_t version, uint32_t id, const char *error_msg) {
	struct wl_resource *object = wl_resource_create(client, &river_xkb_keymap_v1_interface, (int)version, id);
	if (object == NULL) {
		return false;
	}
	wl_resource_set_implementation(object, &keymap_inert_impl, NULL, NULL);
	river_xkb_keymap_v1_send_failure(object, error_msg);
	return true;
}

static void request_inert_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

static const struct river_xkb_keymap_v1_interface keymap_inert_impl = {
	.destroy = request_inert_destroy,
};

static void handle_destroy(struct wl_resource *object) {
	struct rv_xkb_keymap *keymap = wl_resource_get_user_data(object);
	xkb_keymap_unref(keymap->xkb_keymap);
	wl_list_remove(&keymap->link);
	free(keymap);
}

static void request_destroy(struct wl_client *client, struct wl_resource *object) {
	struct rv_xkb_keymap *keymap = wl_resource_get_user_data(object);
	assert(keymap->object == object);
	wl_resource_destroy(object);
}

static const struct river_xkb_keymap_v1_interface keymap_impl = {
	.destroy = request_destroy,
};
