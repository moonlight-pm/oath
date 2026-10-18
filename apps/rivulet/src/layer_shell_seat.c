// SPDX-FileCopyrightText: © 2025 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/LayerShellSeat.zig for Oath, 2026-10-18.

#include "rivulet.h"

static void request_destroy(struct wl_client *client, struct wl_resource *object);

static const struct river_layer_shell_seat_v1_interface layer_shell_seat_impl = {
	.destroy = request_destroy,
};

static void request_destroy_inert(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

// handleRequestInert
static const struct river_layer_shell_seat_v1_interface layer_shell_seat_inert_impl = {
	.destroy = request_destroy_inert,
};

static void handle_destroy(struct wl_resource *object) {
	struct rv_layer_shell_seat *shell_seat = wl_resource_get_user_data(object);
	shell_seat->object = NULL;
}

void rv_layer_shell_seat_create_object(struct rv_layer_shell_seat *shell_seat, struct wl_client *client,
		uint32_t version, uint32_t id) {
	assert(shell_seat->object == NULL);
	shell_seat->object = wl_resource_create(client, &river_layer_shell_seat_v1_interface, (int)version, id);
	if (shell_seat->object == NULL) {
		wl_client_post_no_memory(client);
		return;
	}
	wl_resource_set_implementation(shell_seat->object, &layer_shell_seat_impl, shell_seat, handle_destroy);
	rv_window_manager_dirty_windowing(&server.wm);
}

void rv_layer_shell_seat_make_inert(struct rv_layer_shell_seat *shell_seat) {
	struct wl_resource *object = shell_seat->object;
	if (object != NULL) {
		wl_resource_set_implementation(object, &layer_shell_seat_inert_impl, NULL, NULL);
		shell_seat->object = NULL;
	}
}

static void request_destroy(struct wl_client *client, struct wl_resource *object) {
	struct rv_layer_shell_seat *shell_seat = wl_resource_get_user_data(object);
	assert(shell_seat->object == object);
	wl_resource_destroy(object);
}

void rv_layer_shell_seat_manage_start(struct rv_layer_shell_seat *shell_seat) {
	// Only the tags are compared.
	if (shell_seat->scheduled.focus.tag != shell_seat->sent.focus.tag) {
		struct wl_resource *shell_seat_v1 = shell_seat->object;
		if (shell_seat_v1 != NULL) {
			switch (shell_seat->scheduled.focus.tag) {
			case RV_LAYER_SHELL_SEAT_FOCUS_EXCLUSIVE:
				river_layer_shell_seat_v1_send_focus_exclusive(shell_seat_v1);
				break;
			case RV_LAYER_SHELL_SEAT_FOCUS_NON_EXCLUSIVE:
				river_layer_shell_seat_v1_send_focus_non_exclusive(shell_seat_v1);
				break;
			case RV_LAYER_SHELL_SEAT_FOCUS_NONE:
				river_layer_shell_seat_v1_send_focus_none(shell_seat_v1);
				break;
			}
		}
	}
	shell_seat->sent.focus = shell_seat->scheduled.focus;
}
