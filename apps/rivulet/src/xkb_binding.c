// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/XkbBinding.zig for Oath, 2026-10-18.

#include "rivulet.h"

static const struct river_xkb_binding_v1_interface binding_impl;
static const struct river_xkb_binding_v1_interface binding_inert_impl;
static void handle_destroy(struct wl_resource *xkb_binding_v1);
static void destroy_(struct rv_xkb_binding *binding);

bool rv_xkb_binding_create(struct rv_seat *seat, struct wl_client *client, uint32_t version, uint32_t id,
		xkb_keysym_t keysym, uint32_t modifiers) {
	struct rv_xkb_binding *binding = calloc(1, sizeof(*binding));
	if (binding == NULL) {
		return false;
	}

	struct wl_resource *xkb_binding_v1 = wl_resource_create(client, &river_xkb_binding_v1_interface, version, id);
	if (xkb_binding_v1 == NULL) {
		free(binding);
		return false;
	}

	{
		char buffer[64];
		int len = xkb_keysym_get_name(keysym, buffer, sizeof(buffer));
		rv_debug(RV_SCOPE_INPUT, "new river_xkb_binding_v1: keysym: %u(%.*s) modifiers: %u",
			(uint32_t)keysym, len > 0 ? len : 0, buffer, modifiers);
	}

	*binding = (struct rv_xkb_binding){
		.seat = seat,
		.object = xkb_binding_v1,
		.keysym = keysym,
		.modifiers = modifiers,
	};
	wl_resource_set_implementation(xkb_binding_v1, &binding_impl, binding, handle_destroy);

	wl_list_insert(seat->xkb_bindings.prev, &binding->link);
	return true;
}

void rv_xkb_binding_destroy(struct rv_xkb_binding *binding) {
	wl_resource_set_implementation(binding->object, &binding_inert_impl, NULL, NULL);
	destroy_(binding);
}

static void request_inert_destroy(struct wl_client *client, struct wl_resource *xkb_binding_v1) {
	wl_resource_destroy(xkb_binding_v1);
}

static void request_inert_set_layout_override(struct wl_client *client, struct wl_resource *xkb_binding_v1,
		uint32_t layout) {
}

static void request_inert_enable(struct wl_client *client, struct wl_resource *xkb_binding_v1) {
}

static void request_inert_disable(struct wl_client *client, struct wl_resource *xkb_binding_v1) {
}

static const struct river_xkb_binding_v1_interface binding_inert_impl = {
	.destroy = request_inert_destroy,
	.set_layout_override = request_inert_set_layout_override,
	.enable = request_inert_enable,
	.disable = request_inert_disable,
};

static void destroy_(struct rv_xkb_binding *binding) {
	{
		struct rv_keyboard_group *group;
		wl_list_for_each(group, &binding->seat->keyboard_groups, link) {
			for (uint32_t i = 0; i < group->pressed.len; i++) {
				struct rv_keyboard_group_press *press = &group->pressed.values[i];
				if (press->consumer.tag == RV_KEYBOARD_GROUP_KEY_CONSUMER_BINDING &&
						press->consumer.binding == binding) {
					press->consumer.binding = NULL;
				}
			}
		}
	}
	wl_list_remove(&binding->link);
	free(binding);
}

static void handle_destroy(struct wl_resource *xkb_binding_v1) {
	struct rv_xkb_binding *binding = wl_resource_get_user_data(xkb_binding_v1);
	destroy_(binding);
}

static struct rv_xkb_binding *get_binding(struct wl_resource *xkb_binding_v1) {
	struct rv_xkb_binding *binding = wl_resource_get_user_data(xkb_binding_v1);
	assert(binding->object == xkb_binding_v1);
	return binding;
}

static void request_destroy(struct wl_client *client, struct wl_resource *xkb_binding_v1) {
	(void)get_binding(xkb_binding_v1);
	wl_resource_destroy(xkb_binding_v1);
}

static void request_set_layout_override(struct wl_client *client, struct wl_resource *xkb_binding_v1,
		uint32_t layout) {
	struct rv_xkb_binding *binding = get_binding(xkb_binding_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	binding->wm_requested.has_layout = true;
	binding->wm_requested.layout = layout;
}

static void request_enable(struct wl_client *client, struct wl_resource *xkb_binding_v1) {
	struct rv_xkb_binding *binding = get_binding(xkb_binding_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	binding->wm_requested.enabled = true;
}

static void request_disable(struct wl_client *client, struct wl_resource *xkb_binding_v1) {
	struct rv_xkb_binding *binding = get_binding(xkb_binding_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	binding->wm_requested.enabled = false;
}

static const struct river_xkb_binding_v1_interface binding_impl = {
	.destroy = request_destroy,
	.set_layout_override = request_set_layout_override,
	.enable = request_enable,
	.disable = request_disable,
};

void rv_xkb_binding_pressed(struct rv_xkb_binding *binding) {
	assert(!binding->sent_pressed);
	// Input event processing should not continue after a state change
	// until that event is sent to the window manager in an update and acked.
	assert(binding->wm_scheduled.state_change == RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_NONE);
	binding->wm_scheduled.state_change = RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_PRESSED;
	rv_window_manager_dirty_windowing(&server.wm);
}

void rv_xkb_binding_stop_repeat(struct rv_xkb_binding *binding) {
	assert(binding->sent_pressed);
	// Input event processing should not continue after a state change
	// until that event is sent to the window manager in an update and acked.
	// However, stop_repeat is special since it is triggered on any key event.
	// This means that when a keyboard is removed from a group and all keys
	// pressed on that keyboard are released at the same time stopRepeat()
	// may be called more than once.
	assert(binding->wm_scheduled.state_change == RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_NONE ||
		binding->wm_scheduled.state_change == RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_STOP_REPEAT);
	binding->wm_scheduled.state_change = RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_STOP_REPEAT;
	rv_window_manager_dirty_windowing(&server.wm);
}

void rv_xkb_binding_released(struct rv_xkb_binding *binding) {
	assert(binding->sent_pressed);
	// stopRepeat() should always be called before released() by KeyboardGroup
	assert(binding->wm_scheduled.state_change == RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_STOP_REPEAT);
	binding->wm_scheduled.state_change = RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_RELEASED;
	rv_window_manager_dirty_windowing(&server.wm);
}

// Compare binding with given keycode, modifiers and keyboard state
bool rv_xkb_binding_match(const struct rv_xkb_binding *binding, xkb_keycode_t keycode, uint32_t modifiers,
		struct xkb_state *xkb_state, enum rv_xkb_binding_match_method method) {
	if (!binding->wm_requested.enabled) {
		return false;
	}

	struct xkb_keymap *keymap = xkb_state_get_keymap(xkb_state);

	// If the binding has no pinned layout, use the active layout.
	// It doesn't matter if the index is out of range, since xkbcommon
	// will fall back to the active layout if so.
	xkb_layout_index_t layout = binding->wm_requested.has_layout ?
		binding->wm_requested.layout : xkb_state_key_get_layout(xkb_state, keycode);

	switch (method) {
	case RV_XKB_BINDING_MATCH_METHOD_NO_TRANSLATE: {
		// Get keysyms from the base layer, as if modifiers didn't change keysyms.
		// E.g. pressing `Super+Shift 1` does not translate to `Super Exclam`.
		const xkb_keysym_t *keysyms = NULL;
		int len = xkb_keymap_key_get_syms_by_level(keymap, keycode, layout, 0, &keysyms);

		if (modifiers == binding->modifiers) {
			for (int i = 0; i < len; i++) {
				if (keysyms[i] == binding->keysym) {
					return true;
				}
			}
		}
		break;
	}
	case RV_XKB_BINDING_MATCH_METHOD_TRANSLATE: {
		// Keysyms and modifiers as translated by xkb.
		// Modifiers used to translate the key are consumed.
		// E.g. pressing `Super+Shift 1` translates to `Super Exclam`.
		const xkb_keysym_t *keysyms_translated = NULL;
		int len = xkb_keymap_key_get_syms_by_level(keymap, keycode, layout,
			xkb_state_key_get_level(xkb_state, keycode, layout), &keysyms_translated);

		xkb_mod_mask_t consumed = xkb_state_key_get_consumed_mods2(xkb_state, keycode, XKB_CONSUMED_MODE_XKB);
		uint32_t modifiers_translated = modifiers & ~consumed;

		if (modifiers_translated == binding->modifiers) {
			for (int i = 0; i < len; i++) {
				if (keysyms_translated[i] == binding->keysym) {
					return true;
				}
			}
		}
		break;
	}
	}

	return false;
}
