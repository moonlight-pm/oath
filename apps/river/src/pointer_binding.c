// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/PointerBinding.zig for Oath, 2026-10-18.

#include "river.h"

static const struct river_pointer_binding_v1_interface binding_impl;
static const struct river_pointer_binding_v1_interface binding_inert_impl;
static void handle_destroy(struct wl_resource *pointer_binding_v1);
static void destroy_(struct rv_pointer_binding *binding);

bool rv_pointer_binding_create(struct rv_seat *seat, struct wl_client *client, uint32_t version, uint32_t id,
		uint32_t button, uint32_t modifiers) {
	struct rv_pointer_binding *binding = calloc(1, sizeof(*binding));
	if (binding == NULL) {
		return false;
	}

	struct wl_resource *pointer_binding_v1 =
		wl_resource_create(client, &river_pointer_binding_v1_interface, version, id);
	if (pointer_binding_v1 == NULL) {
		free(binding);
		return false;
	}

	const char *name = libevdev_event_code_get_name(EV_KEY, button);
	rv_debug(RV_SCOPE_INPUT, "new river_pointer_binding_v1: button: %u(%s) modifiers: %u",
		button, name != NULL ? name : "null", modifiers);

	*binding = (struct rv_pointer_binding){
		.seat = seat,
		.object = pointer_binding_v1,
		.button = button,
		.modifiers = modifiers,
	};
	wl_resource_set_implementation(pointer_binding_v1, &binding_impl, binding, handle_destroy);

	wl_list_insert(seat->pointer_bindings.prev, &binding->link);
	return true;
}

void rv_pointer_binding_destroy(struct rv_pointer_binding *binding) {
	wl_resource_set_implementation(binding->object, &binding_inert_impl, NULL, NULL);
	destroy_(binding);
}

static void request_inert_destroy(struct wl_client *client, struct wl_resource *pointer_binding_v1) {
	wl_resource_destroy(pointer_binding_v1);
}

static void request_inert_enable(struct wl_client *client, struct wl_resource *pointer_binding_v1) {
}

static void request_inert_disable(struct wl_client *client, struct wl_resource *pointer_binding_v1) {
}

static const struct river_pointer_binding_v1_interface binding_inert_impl = {
	.destroy = request_inert_destroy,
	.enable = request_inert_enable,
	.disable = request_inert_disable,
};

static void destroy_(struct rv_pointer_binding *binding) {
	struct rv_cursor *cursor = &binding->seat->cursor;
	for (size_t i = 0; i < cursor->pressed.len; i++) {
		struct rv_cursor_pressed_entry *entry = &cursor->pressed.items[i];
		if (entry->button == binding->button) {
			// It is possible for the window manager to create duplicate pointer bindings.
			if (entry->binding == binding) {
				entry->binding = NULL;
			}
			break;
		}
	}

	wl_list_remove(&binding->link);
	free(binding);
}

static void handle_destroy(struct wl_resource *pointer_binding_v1) {
	struct rv_pointer_binding *binding = wl_resource_get_user_data(pointer_binding_v1);
	destroy_(binding);
}

static struct rv_pointer_binding *get_binding(struct wl_resource *pointer_binding_v1) {
	struct rv_pointer_binding *binding = wl_resource_get_user_data(pointer_binding_v1);
	assert(binding->object == pointer_binding_v1);
	return binding;
}

static void request_destroy(struct wl_client *client, struct wl_resource *pointer_binding_v1) {
	(void)get_binding(pointer_binding_v1);
	wl_resource_destroy(pointer_binding_v1);
}

static void request_enable(struct wl_client *client, struct wl_resource *pointer_binding_v1) {
	struct rv_pointer_binding *binding = get_binding(pointer_binding_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	binding->wm_requested.enabled = true;
}

static void request_disable(struct wl_client *client, struct wl_resource *pointer_binding_v1) {
	struct rv_pointer_binding *binding = get_binding(pointer_binding_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	binding->wm_requested.enabled = false;
}

static const struct river_pointer_binding_v1_interface binding_impl = {
	.destroy = request_destroy,
	.enable = request_enable,
	.disable = request_disable,
};

void rv_pointer_binding_pressed(struct rv_pointer_binding *binding) {
	assert(!binding->sent_pressed);
	// Input event processing should not continue after a press/release event
	// until that event is sent to the window manager in an update and acked.
	assert(binding->wm_scheduled.state_change == RV_POINTER_BINDING_WM_SCHEDULED_STATE_CHANGE_NONE);
	binding->wm_scheduled.state_change = RV_POINTER_BINDING_WM_SCHEDULED_STATE_CHANGE_PRESSED;
	rv_window_manager_dirty_windowing(&server.wm);
}

void rv_pointer_binding_released(struct rv_pointer_binding *binding) {
	assert(binding->sent_pressed);
	// Input event processing should not continue after a press/release event
	// until that event is sent to the window manager in an update and acked.
	assert(binding->wm_scheduled.state_change == RV_POINTER_BINDING_WM_SCHEDULED_STATE_CHANGE_NONE);
	binding->wm_scheduled.state_change = RV_POINTER_BINDING_WM_SCHEDULED_STATE_CHANGE_RELEASED;
	rv_window_manager_dirty_windowing(&server.wm);
}

// modifiers is a wlr.Keyboard.ModifierMask.
bool rv_pointer_binding_match(const struct rv_pointer_binding *binding, uint32_t button, uint32_t modifiers) {
	if (!binding->wm_requested.enabled) {
		return false;
	}

	return button == binding->button && modifiers == binding->modifiers;
}
