// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Keyboard.zig for Oath, 2026-10-18.

#include "rivulet.h"

static void queue_key(struct wl_listener *listener, void *data);
static void queue_modifiers(struct wl_listener *listener, void *data);
static void queue_keymap(struct wl_listener *listener, void *data);
static bool should_set_keymap(void);
static void maybe_destroy(struct rv_keyboard *keyboard);

struct rv_keyboard *rv_keyboard_create(struct rv_seat *seat, struct wlr_input_device *wlr_device, bool virtual) {
	struct wlr_keyboard *wlr_keyboard = wlr_keyboard_from_input_device(wlr_device);

	struct rv_keyboard *keyboard = calloc(1, sizeof(*keyboard));
	if (keyboard == NULL) {
		return NULL;
	}

	struct xkb_keymap *keymap;
	if (virtual) {
		if (wlr_keyboard->keymap != NULL) {
			keymap = xkb_keymap_ref(wlr_keyboard->keymap);
		} else {
			keymap = NULL;
		}
	} else {
		keymap = xkb_keymap_ref(server.xkb_config.default_keymap);
	}
	*keyboard = (struct rv_keyboard){
		.device_destroyed = false,
		.queued_events = 0,
		.config = {
			.keymap = keymap,
			.repeat_rate = 40,
			.repeat_delay = 400,
		},
		.pressed = { .len = 0 },
		.group = NULL,
	};
	keyboard->key.notify = queue_key;
	keyboard->modifiers.notify = queue_modifiers;
	keyboard->keymap.notify = queue_keymap;

	// keyboard.pressed.ensureTotalCapacity(pressed_count_max): a fixed array, cannot fail.

	// Zig !void, but the body cannot fail.
	rv_input_device_init(&keyboard->device, seat, wlr_device, virtual);

	wlr_keyboard->data = keyboard;

	wl_signal_add(&wlr_keyboard->events.key, &keyboard->key);
	wl_signal_add(&wlr_keyboard->events.modifiers, &keyboard->modifiers);

	if (virtual) {
		wl_signal_add(&wlr_keyboard->events.keymap, &keyboard->keymap);
	} else {
		wl_list_init(&keyboard->keymap.link);
		if (should_set_keymap()) {
			(void)wlr_keyboard_set_keymap(wlr_keyboard, keyboard->config.keymap);
		}
	}

	return keyboard;
}

static void should_set_keymap_iter(struct wlr_backend *backend, void *data) {
	bool *wayland_or_x11 = data;
#if WLR_HAS_X11_BACKEND
	if (wlr_backend_is_wl(backend) || wlr_backend_is_x11(backend)) {
		*wayland_or_x11 = true;
	}
#else
	if (wlr_backend_is_wl(backend)) {
		*wayland_or_x11 = true;
	}
#endif
}

// We don't want to ever set a keymap for backend-created wlr.Keyboards.
// Setting a keymap means that modifiers will be buggy and LEDs will be out of sync.
// However, if we don't set a keymap we don't get modifiers events from the backend
// at all currently due to a wlroots bug. This is even worse, so set a keymap
// despite the bugginess for backends that generate keyboard modifiers events.
// TODO(wlroots) https://gitlab.freedesktop.org/wlroots/wlroots/-/merge_requests/5324
static bool should_set_keymap(void) {
	bool wayland_or_x11 = false;
	wlr_multi_for_each_backend(server.backend, should_set_keymap_iter, &wayland_or_x11);
	return wayland_or_x11;
}

void rv_keyboard_set_group(struct rv_keyboard *keyboard) {
	assert(keyboard->group == NULL);
	struct rv_seat *seat = keyboard->device.seat;
	// Virtual keyboards set their own keymap and require independent modifier state.
	// Therefore, they are always placed in their own group of one.
	if (!keyboard->device.virtual) {
		struct rv_keyboard_group *group, *tmp;
		wl_list_for_each_safe(group, tmp, &seat->keyboard_groups, link) {
			if (rv_keyboard_group_match(group, &keyboard->config)) {
				keyboard->group = rv_keyboard_group_ref(group);
				return;
			}
		}
	}
	keyboard->group = rv_keyboard_group_create(seat, keyboard->config, keyboard->device.virtual);
	if (keyboard->group == NULL) {
		rv_err(RV_SCOPE_INPUT, "out of memory");
		return;
	}
}

void rv_keyboard_set_repeat_info(struct rv_keyboard *keyboard, int32_t rate, int32_t delay) {
	assert(!keyboard->device.virtual);
	keyboard->config.repeat_rate = rate;
	keyboard->config.repeat_delay = delay;
	if (keyboard->group != NULL) {
		rv_keyboard_group_unref(keyboard->group, keyboard->pressed.keys, keyboard->pressed.len);
		keyboard->group = NULL;
	}
	rv_keyboard_set_group(keyboard);
}

void rv_keyboard_set_keymap(struct rv_keyboard *keyboard, struct xkb_keymap *keymap) {
	assert(!keyboard->device.virtual);
	if (should_set_keymap()) {
		// As in the Zig: the keymap set here is keyboard.config.keymap before it is replaced below.
		(void)wlr_keyboard_set_keymap(wlr_keyboard_from_input_device(keyboard->device.wlr_device),
			keyboard->config.keymap);
	}
	if (keyboard->config.keymap != NULL) {
		xkb_keymap_unref(keyboard->config.keymap);
	}
	keyboard->config.keymap = xkb_keymap_ref(keymap);
	if (keyboard->group != NULL) {
		rv_keyboard_group_unref(keyboard->group, keyboard->pressed.keys, keyboard->pressed.len);
		keyboard->group = NULL;
	}
	rv_keyboard_set_group(keyboard);
}

void rv_keyboard_device_destroy(struct rv_keyboard *keyboard) {
	assert(!keyboard->device_destroyed);
	keyboard->device_destroyed = true;

	wl_list_remove(&keyboard->key.link);
	wl_list_remove(&keyboard->modifiers.link);
	wl_list_remove(&keyboard->keymap.link);

	rv_input_device_deinit(&keyboard->device);

	maybe_destroy(keyboard);
}

static void maybe_destroy(struct rv_keyboard *keyboard) {
	if (!keyboard->device_destroyed || keyboard->queued_events > 0) {
		return;
	}

	if (keyboard->config.keymap != NULL) {
		xkb_keymap_unref(keyboard->config.keymap);
	}
	if (keyboard->group != NULL) {
		rv_keyboard_group_unref(keyboard->group, keyboard->pressed.keys, keyboard->pressed.len);
	}

	free(keyboard);
}

void rv_keyboard_drop_event(struct rv_keyboard *keyboard) {
	keyboard->queued_events -= 1;
	maybe_destroy(keyboard);
}

void rv_keyboard_process_key(struct rv_keyboard *keyboard, const struct wlr_keyboard_key_event *key) {
	struct rv_keyboard_pressed *pressed = &keyboard->pressed;
	if (key->state == WL_KEYBOARD_KEY_STATE_RELEASED) {
		// pressed.swapRemove(key.keycode)
		for (uint32_t i = 0; i < pressed->len; i++) {
			if (pressed->keys[i] == key->keycode) {
				pressed->keys[i] = pressed->keys[pressed->len - 1];
				pressed->len -= 1;
				break;
			}
		}
	} else {
		assert(key->state == WL_KEYBOARD_KEY_STATE_PRESSED);
		if (pressed->len < RV_KEYBOARD_GROUP_PRESSED_COUNT_MAX) {
			// pressed.putAssumeCapacity(key.keycode, {})
			bool found = false;
			for (uint32_t i = 0; i < pressed->len; i++) {
				if (pressed->keys[i] == key->keycode) {
					found = true;
					break;
				}
			}
			if (!found) {
				pressed->keys[pressed->len] = key->keycode;
				pressed->len += 1;
			}
		}
	}
	if (keyboard->group != NULL) {
		rv_keyboard_group_process_key(keyboard->group, key);
	}
	rv_keyboard_drop_event(keyboard);
}

void rv_keyboard_process_modifiers(struct rv_keyboard *keyboard, struct wlr_keyboard_modifiers modifiers) {
	if (keyboard->group != NULL) {
		rv_keyboard_group_process_modifiers(keyboard->group, modifiers);
	}
	rv_keyboard_drop_event(keyboard);
}

void rv_keyboard_process_keymap(struct rv_keyboard *keyboard, struct xkb_keymap *keymap) {
	if (keyboard->group != NULL) {
		rv_keyboard_group_process_keymap(keyboard->group, keymap);
	}
	rv_keyboard_drop_event(keyboard);
	xkb_keymap_unref(keymap);
}

static void queue_key(struct wl_listener *listener, void *data) {
	struct rv_keyboard *keyboard = wl_container_of(listener, keyboard, key);
	struct wlr_keyboard_key_event *event = data;
	assert(!keyboard->device_destroyed);
	keyboard->queued_events += 1;
	struct rv_seat_event seat_event = {
		.tag = RV_SEAT_EVENT_KEYBOARD_KEY,
		.keyboard_key = {
			.keyboard = keyboard,
			.key = *event,
		},
	};
	if (!rv_seat_queue_event(keyboard->device.seat, seat_event)) {
		keyboard->queued_events -= 1;
	}
}

static void queue_modifiers(struct wl_listener *listener, void *data) {
	struct rv_keyboard *keyboard = wl_container_of(listener, keyboard, modifiers);
	assert(!keyboard->device_destroyed);
	struct wlr_keyboard *wlr_keyboard = wlr_keyboard_from_input_device(keyboard->device.wlr_device);
	keyboard->queued_events += 1;
	struct rv_seat_event seat_event = {
		.tag = RV_SEAT_EVENT_KEYBOARD_MODIFIERS,
		.keyboard_modifiers = {
			.keyboard = keyboard,
			.modifiers = wlr_keyboard->modifiers,
		},
	};
	if (!rv_seat_queue_event(keyboard->device.seat, seat_event)) {
		keyboard->queued_events -= 1;
	}
}

static void queue_keymap(struct wl_listener *listener, void *data) {
	struct rv_keyboard *keyboard = wl_container_of(listener, keyboard, keymap);
	assert(!keyboard->device_destroyed);
	struct wlr_keyboard *wlr_keyboard = wlr_keyboard_from_input_device(keyboard->device.wlr_device);
	struct xkb_keymap *keymap = wlr_keyboard->keymap;
	if (keymap == NULL) {
		return;
	}
	keyboard->queued_events += 1;
	struct rv_seat_event seat_event = {
		.tag = RV_SEAT_EVENT_KEYBOARD_KEYMAP,
		.keyboard_keymap = {
			.keyboard = keyboard,
			.keymap = xkb_keymap_ref(keymap),
		},
	};
	if (!rv_seat_queue_event(keyboard->device.seat, seat_event)) {
		keyboard->queued_events -= 1;
		xkb_keymap_unref(keymap);
	}
}
