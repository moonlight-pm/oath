// SPDX-FileCopyrightText: © 2025 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/XkbBindings.zig for Oath, 2026-10-18.

#include "river.h"

static void handle_server_destroy(struct wl_listener *listener, void *data);
static void bind(struct wl_client *client, void *data, uint32_t version, uint32_t id);

bool rv_xkb_bindings_init(struct rv_xkb_bindings *bindings) {
	struct wl_global *global = wl_global_create(server.wl_server, &river_xkb_bindings_v1_interface, 3, NULL, bind);
	if (global == NULL) {
		return false;
	}
	*bindings = (struct rv_xkb_bindings){
		.global = global,
	};
	bindings->server_destroy.notify = handle_server_destroy;
	wl_display_add_destroy_listener(server.wl_server, &bindings->server_destroy);
	return true;
}

static void handle_server_destroy(struct wl_listener *listener, void *data) {
	struct rv_xkb_bindings *bindings = wl_container_of(listener, bindings, server_destroy);

	wl_global_destroy(bindings->global);
}

static const struct river_xkb_bindings_v1_interface bindings_impl;

static void bind(struct wl_client *client, void *data, uint32_t version, uint32_t id) {
	struct wl_resource *object = wl_resource_create(client, &river_xkb_bindings_v1_interface, version, id);
	if (object == NULL) {
		wl_client_post_no_memory(client);
		rv_err(RV_SCOPE_WM, "out of memory");
		return;
	}

	wl_resource_set_implementation(object, &bindings_impl, NULL, NULL);
}

static void request_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

static void request_get_xkb_binding(struct wl_client *client, struct wl_resource *object,
		struct wl_resource *seat_resource, uint32_t id, uint32_t keysym, uint32_t modifiers) {
	// Since we make all river_seat_v1 objects inert when the active
	// window manager is destroyed, this check means that only the
	// active window manager can create bindings.
	struct rv_seat *seat = wl_resource_get_user_data(seat_resource);
	if (seat == NULL) {
		return;
	}
	if (!rv_xkb_binding_create(seat, wl_resource_get_client(object), wl_resource_get_version(object), id,
			(xkb_keysym_t)keysym, modifiers)) {
		wl_client_post_no_memory(wl_resource_get_client(object));
		rv_err(RV_SCOPE_WM, "out of memory");
		return;
	}
}

static void request_get_seat(struct wl_client *client, struct wl_resource *object, uint32_t id,
		struct wl_resource *seat_resource) {
	// Since we make all river_seat_v1 objects inert when the active
	// window manager is destroyed, this check means that only the
	// active window manager can create a bindings seat.
	struct rv_seat *seat = wl_resource_get_user_data(seat_resource);
	if (seat == NULL) {
		return;
	}
	if (seat->xkb_bindings_seat.object != NULL) {
		wl_resource_post_error(object, RIVER_XKB_BINDINGS_V1_ERROR_OBJECT_ALREADY_CREATED, "%s",
			"river_xkb_bindings_seat_v1 already created");
		return;
	}
	rv_xkb_bindings_seat_create_object(&seat->xkb_bindings_seat, wl_resource_get_client(object),
		wl_resource_get_version(object), id);
}

static const struct river_xkb_bindings_v1_interface bindings_impl = {
	.destroy = request_destroy,
	.get_xkb_binding = request_get_xkb_binding,
	.get_seat = request_get_seat,
};
