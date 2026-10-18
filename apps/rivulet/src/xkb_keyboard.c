// SPDX-FileCopyrightText: © 2026 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/XkbKeyboard.zig for Oath, 2026-10-18.

#include "rivulet.h"

static const struct river_xkb_keyboard_v1_interface keyboard_impl;
static const struct river_xkb_keyboard_v1_interface keyboard_inert_impl;
static void handle_destroy(struct wl_resource *object);

void rv_xkb_keyboard_init(struct rv_xkb_keyboard *xkb_keyboard) {
	*xkb_keyboard = (struct rv_xkb_keyboard){ 0 };
	wl_list_init(&xkb_keyboard->objects);
	wl_list_insert(server.xkb_config.keyboards.prev, &xkb_keyboard->link);
	{
		struct wl_resource *config_v1, *tmp;
		wl_resource_for_each_safe(config_v1, tmp, &server.xkb_config.objects) {
			rv_xkb_keyboard_create_object(xkb_keyboard, config_v1);
		}
	}
}

void rv_xkb_keyboard_create_object(struct rv_xkb_keyboard *xkb_keyboard, struct wl_resource *config_v1) {
	struct wl_resource *object = wl_resource_create(wl_resource_get_client(config_v1),
		&river_xkb_keyboard_v1_interface, wl_resource_get_version(config_v1), 0);
	if (object == NULL) {
		rv_err(RV_SCOPE_INPUT, "out of memory");
		wl_resource_post_no_memory(config_v1);
		return;
	}
	wl_list_insert(xkb_keyboard->objects.prev, wl_resource_get_link(object));
	wl_resource_set_implementation(object, &keyboard_impl, xkb_keyboard, handle_destroy);
	river_xkb_config_v1_send_xkb_keyboard(config_v1, object);
	{
		struct rv_input_device *device = wl_container_of(xkb_keyboard, device, xkb_keyboard);
		assert(!device->virtual);
		struct wl_resource *input_device_v1, *tmp;
		wl_resource_for_each_safe(input_device_v1, tmp, &device->objects) {
			if (wl_resource_get_client(object) == wl_resource_get_client(input_device_v1)) {
				river_xkb_keyboard_v1_send_input_device(object, input_device_v1);
			}
		}
	}
	const struct rv_xkb_keyboard_sent *sent = &xkb_keyboard->sent;
	if (sent->has_layout_index) {
		river_xkb_keyboard_v1_send_layout(object, sent->layout_index, sent->layout_name);
	}
	if (sent->has_capslock) {
		if (sent->capslock) {
			river_xkb_keyboard_v1_send_capslock_enabled(object);
		} else {
			river_xkb_keyboard_v1_send_capslock_disabled(object);
		}
	}
	if (sent->has_numlock) {
		if (sent->numlock) {
			river_xkb_keyboard_v1_send_numlock_enabled(object);
		} else {
			river_xkb_keyboard_v1_send_numlock_disabled(object);
		}
	}
}

void rv_xkb_keyboard_deinit(struct rv_xkb_keyboard *xkb_keyboard) {
	{
		struct wl_resource *object, *tmp;
		wl_resource_for_each_safe(object, tmp, &xkb_keyboard->objects) {
			wl_list_remove(wl_resource_get_link(object));
			river_xkb_keyboard_v1_send_removed(object);
			wl_resource_set_implementation(object, &keyboard_inert_impl, NULL, NULL);
		}
	}
	assert(wl_list_empty(&xkb_keyboard->objects));
	wl_list_remove(&xkb_keyboard->link);
}

static void request_inert_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

static void request_inert_set_keymap(struct wl_client *client, struct wl_resource *object,
		struct wl_resource *keymap) {
}

static void request_inert_set_layout_by_index(struct wl_client *client, struct wl_resource *object,
		int32_t index) {
}

static void request_inert_set_layout_by_name(struct wl_client *client, struct wl_resource *object,
		const char *name) {
}

static void request_inert_noop(struct wl_client *client, struct wl_resource *object) {
}

static const struct river_xkb_keyboard_v1_interface keyboard_inert_impl = {
	.destroy = request_inert_destroy,
	.set_keymap = request_inert_set_keymap,
	.set_layout_by_index = request_inert_set_layout_by_index,
	.set_layout_by_name = request_inert_set_layout_by_name,
	.capslock_enable = request_inert_noop,
	.capslock_disable = request_inert_noop,
	.numlock_enable = request_inert_noop,
	.numlock_disable = request_inert_noop,
};

static void handle_destroy(struct wl_resource *object) {
	wl_list_remove(wl_resource_get_link(object));
}

// The common head of XkbKeyboard.handleRequest, run before every request:
// the keyboard and its group (keyboard.group.? asserts the group is set).
static struct rv_keyboard *get_keyboard(struct wl_resource *object, struct rv_keyboard_group **group) {
	struct rv_xkb_keyboard *xkb_keyboard = wl_resource_get_user_data(object);
	struct rv_input_device *device = wl_container_of(xkb_keyboard, device, xkb_keyboard);
	struct rv_keyboard *keyboard = wl_container_of(device, keyboard, device);
	assert(keyboard->group != NULL);
	*group = keyboard->group;
	return keyboard;
}

// group.config.keymap.?
static struct xkb_keymap *group_keymap(struct rv_keyboard_group *group) {
	assert(group->config.keymap != NULL);
	return group->config.keymap;
}

static void notify_modifiers(struct rv_keyboard_group *group, struct wlr_keyboard_modifiers modifiers) {
	wlr_keyboard_notify_modifiers(&group->state, modifiers.depressed, modifiers.latched, modifiers.locked,
		modifiers.group);
}

static void request_destroy(struct wl_client *client, struct wl_resource *object) {
	struct rv_keyboard_group *group;
	(void)get_keyboard(object, &group);
	wl_resource_destroy(object);
}

static void request_set_keymap(struct wl_client *client, struct wl_resource *object,
		struct wl_resource *keymap_arg) {
	struct rv_keyboard_group *group;
	struct rv_keyboard *keyboard = get_keyboard(object, &group);
	struct rv_xkb_keymap *keymap = wl_resource_get_user_data(keymap_arg);
	if (keymap != NULL) {
		rv_keyboard_set_keymap(keyboard, keymap->xkb_keymap);
	} else {
		wl_resource_post_error(object, RIVER_XKB_KEYBOARD_V1_ERROR_INVALID_KEYMAP, "%s",
			"client set invalid keymap");
		return;
	}
}

static void request_set_layout_by_index(struct wl_client *client, struct wl_resource *object, int32_t index) {
	struct rv_keyboard_group *group;
	(void)get_keyboard(object, &group);
	if (index < 0 || (uint32_t)index >= xkb_keymap_num_layouts(group_keymap(group))) {
		return;
	}
	struct wlr_keyboard_modifiers modifiers = group->state.modifiers;
	modifiers.group = (xkb_layout_index_t)index;
	notify_modifiers(group, modifiers);
}

static void request_set_layout_by_name(struct wl_client *client, struct wl_resource *object, const char *name) {
	struct rv_keyboard_group *group;
	(void)get_keyboard(object, &group);
	const xkb_layout_index_t index = xkb_keymap_layout_get_index(group_keymap(group), name);
	if (index == XKB_LAYOUT_INVALID) {
		return;
	}
	struct wlr_keyboard_modifiers modifiers = group->state.modifiers;
	modifiers.group = index;
	notify_modifiers(group, modifiers);
}

static void request_numlock_enable(struct wl_client *client, struct wl_resource *object) {
	struct rv_keyboard_group *group;
	(void)get_keyboard(object, &group);
	const xkb_mod_mask_t mask = xkb_keymap_mod_get_mask(group_keymap(group), XKB_VMOD_NAME_NUM);
	struct wlr_keyboard_modifiers modifiers = group->state.modifiers;
	modifiers.locked |= mask;
	notify_modifiers(group, modifiers);
}

static void request_numlock_disable(struct wl_client *client, struct wl_resource *object) {
	struct rv_keyboard_group *group;
	(void)get_keyboard(object, &group);
	const xkb_mod_mask_t mask = xkb_keymap_mod_get_mask(group_keymap(group), XKB_VMOD_NAME_NUM);
	struct wlr_keyboard_modifiers modifiers = group->state.modifiers;
	modifiers.locked &= ~mask;
	notify_modifiers(group, modifiers);
}

static void request_capslock_enable(struct wl_client *client, struct wl_resource *object) {
	struct rv_keyboard_group *group;
	(void)get_keyboard(object, &group);
	const xkb_mod_mask_t mask = xkb_keymap_mod_get_mask(group_keymap(group), XKB_MOD_NAME_CAPS);
	struct wlr_keyboard_modifiers modifiers = group->state.modifiers;
	modifiers.locked |= mask;
	notify_modifiers(group, modifiers);
}

static void request_capslock_disable(struct wl_client *client, struct wl_resource *object) {
	struct rv_keyboard_group *group;
	(void)get_keyboard(object, &group);
	const xkb_mod_mask_t mask = xkb_keymap_mod_get_mask(group_keymap(group), XKB_MOD_NAME_CAPS);
	struct wlr_keyboard_modifiers modifiers = group->state.modifiers;
	modifiers.locked &= ~mask;
	notify_modifiers(group, modifiers);
}

static const struct river_xkb_keyboard_v1_interface keyboard_impl = {
	.destroy = request_destroy,
	.set_keymap = request_set_keymap,
	.set_layout_by_index = request_set_layout_by_index,
	.set_layout_by_name = request_set_layout_by_name,
	.capslock_enable = request_capslock_enable,
	.capslock_disable = request_capslock_disable,
	.numlock_enable = request_numlock_enable,
	.numlock_disable = request_numlock_disable,
};

void rv_xkb_keyboard_send_state(struct rv_xkb_keyboard *xkb_keyboard, xkb_layout_index_t layout_index,
		const char *layout_name, bool capslock, bool numlock) {
	struct rv_xkb_keyboard_sent *sent = &xkb_keyboard->sent;
	struct wl_resource *object, *tmp;
	wl_resource_for_each_safe(object, tmp, &xkb_keyboard->objects) {
		if (!sent->has_layout_index || sent->layout_index != layout_index ||
				(sent->layout_name == NULL) != (layout_name == NULL) ||
				(layout_name != NULL && strcmp(layout_name, sent->layout_name) != 0)) {
			river_xkb_keyboard_v1_send_layout(object, layout_index, layout_name);
		}
		if (!sent->has_capslock || sent->capslock != capslock) {
			if (capslock) {
				river_xkb_keyboard_v1_send_capslock_enabled(object);
			} else {
				river_xkb_keyboard_v1_send_capslock_disabled(object);
			}
		}
		if (!sent->has_numlock || sent->numlock != numlock) {
			if (numlock) {
				river_xkb_keyboard_v1_send_numlock_enabled(object);
			} else {
				river_xkb_keyboard_v1_send_numlock_disabled(object);
			}
		}
	}
	sent->has_layout_index = true;
	sent->layout_index = layout_index;
	sent->layout_name = layout_name;
	sent->has_capslock = true;
	sent->capslock = capslock;
	sent->has_numlock = true;
	sent->numlock = numlock;
}
