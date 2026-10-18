// SPDX-FileCopyrightText: © 2025 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/KeyboardGroup.zig for Oath, 2026-10-18.

#include "rivulet.h"

static void handle_key(struct wl_listener *listener, void *data);
static void handle_modifiers(struct wl_listener *listener, void *data);
static void led_update(struct wlr_keyboard *state, uint32_t leds);
static bool keysym_is_modifier(xkb_keysym_t keysym);
static bool handle_builtin_binding(xkb_keysym_t keysym);
static struct wlr_input_method_keyboard_grab_v2 *get_input_method_grab(struct rv_keyboard_group *group);

static const struct wlr_keyboard_impl keyboard_group_impl = {
	.name = "rivulet.KeyboardGroup",
	.led_update = led_update,
};

// group.pressed.getIndex(keycode); -1 if absent.
static int pressed_index(const struct rv_keyboard_group_pressed *pressed, uint32_t keycode) {
	for (uint32_t i = 0; i < pressed->len; i++) {
		if (pressed->keys[i] == keycode) {
			return (int)i;
		}
	}
	return -1;
}

struct rv_keyboard_group *rv_keyboard_group_create(struct rv_seat *seat, struct rv_keyboard_config config,
		bool virtual) {
	struct rv_keyboard_group *group = calloc(1, sizeof(*group));
	if (group == NULL) {
		return NULL;
	}
	*group = (struct rv_keyboard_group){
		.ref_count = 1,
		.seat = seat,
		.virtual = virtual,
		.config = config,
		.modifiers_old = 0,
		.pressed = { .len = 0 },
	};
	group->key.notify = handle_key;
	group->modifiers.notify = handle_modifiers;

	// group.pressed.ensureTotalCapacity(pressed_count_max): a fixed array, cannot fail.

	wl_list_insert(seat->keyboard_groups.prev, &group->link);

	wlr_keyboard_init(&group->state, &keyboard_group_impl, "rivulet.KeyboardGroup");
	group->state.data = group;

	// wlroots will log an error on failure, there's not much we can do to recover unfortunately.
	(void)wlr_keyboard_set_keymap(&group->state, config.keymap);
	wlr_keyboard_set_repeat_info(&group->state, config.repeat_rate, config.repeat_delay);

	wl_signal_add(&group->state.events.key, &group->key);
	wl_signal_add(&group->state.events.modifiers, &group->modifiers);

	return group;
}

struct rv_keyboard_group *rv_keyboard_group_ref(struct rv_keyboard_group *group) {
	group->ref_count += 1;
	return group;
}

void rv_keyboard_group_unref(struct rv_keyboard_group *group, const uint32_t *to_release, size_t to_release_len) {
	for (size_t i = 0; i < to_release_len; i++) {
		struct wlr_keyboard_key_event event = {
			.time_msec = rv_util_msec_timestamp(),
			.keycode = to_release[i],
			.update_state = true,
			.state = WL_KEYBOARD_KEY_STATE_RELEASED,
		};
		rv_keyboard_group_process_key(group, &event);
	}

	group->ref_count -= 1;
	if (group->ref_count > 0) {
		return;
	}

	wl_list_remove(&group->link);

	wl_list_remove(&group->key.link);
	wl_list_remove(&group->modifiers.link);

	// If the currently active keyboard of a seat is destroyed we need to set
	// a new active keyboard. Otherwise wlroots may send an enter event without
	// first having sent a keymap event if Seat.keyboardNotifyEnter() is called
	// before a new active keyboard is set.
	if (wlr_seat_get_keyboard(group->seat->wlr_seat) == &group->state) {
		if (!wl_list_empty(&group->seat->keyboard_groups)) {
			struct rv_keyboard_group *other = wl_container_of(group->seat->keyboard_groups.next, other, link);
			wlr_seat_set_keyboard(group->seat->wlr_seat, &other->state);
		}
	}

	wlr_keyboard_finish(&group->state);

	free(group);
}

bool rv_keyboard_group_match(const struct rv_keyboard_group *group, struct rv_keyboard_config *config) {
	const struct rv_keyboard_config *a = &group->config;
	struct rv_keyboard_config *b = config;
	if (a->repeat_rate != b->repeat_rate) {
		return false;
	}
	if (a->repeat_delay != b->repeat_delay) {
		return false;
	}

	if (a->keymap == b->keymap) {
		return true;
	}
	if (a->keymap == NULL || b->keymap == NULL) {
		return false;
	}

	// Can't get away with a cheap pointer comparison.
	// TODO implement a non-terrible way to do this upstream in xkbcommon
#if defined(RV_HAVE_XKB_KEYMAP_GET_AS_STRING2)
	char *a_string = xkb_keymap_get_as_string2(a->keymap, XKB_KEYMAP_USE_ORIGINAL_FORMAT,
		XKB_KEYMAP_SERIALIZE_NO_FLAGS);
	char *b_string = xkb_keymap_get_as_string2(b->keymap, XKB_KEYMAP_USE_ORIGINAL_FORMAT,
		XKB_KEYMAP_SERIALIZE_NO_FLAGS);
#else
	// xkbcommon < 1.12 has no xkb_keymap_get_as_string2(); both keymaps are
	// serialized the same way, which is all this comparison needs.
	char *a_string = xkb_keymap_get_as_string(a->keymap, XKB_KEYMAP_USE_ORIGINAL_FORMAT);
	char *b_string = xkb_keymap_get_as_string(b->keymap, XKB_KEYMAP_USE_ORIGINAL_FORMAT);
#endif
	bool result = false;
	if (a_string == NULL || b_string == NULL) {
		// Ugh, no good options here, we don't know why the function failed.
		// xkbcommon really needs a better API for this.
		rv_err(RV_SCOPE_INPUT, "xkb_keymap_get_as_string2() failed");
		result = false;
	} else if (strcmp(a_string, b_string) == 0) {
		// Consolidate so we don't have to do this expensive/silly comparison again
		xkb_keymap_unref(config->keymap);
		config->keymap = xkb_keymap_ref(group->config.keymap);
		result = true;
	}
	free(b_string);
	free(a_string);
	return result;
}

void rv_keyboard_group_process_key(struct rv_keyboard_group *group, const struct wlr_keyboard_key_event *event) {
	int index = pressed_index(&group->pressed, event->keycode);
	if (index >= 0) {
		struct rv_keyboard_group_press *key = &group->pressed.values[index];
		assert(key->count > 0);
		if (event->state == WL_KEYBOARD_KEY_STATE_PRESSED) {
			key->count += 1;
		} else {
			key->count -= 1;
			if (key->count == 0) {
				struct wlr_keyboard_key_event key_event = {
					.time_msec = event->time_msec,
					.keycode = event->keycode,
					.update_state = true,
					.state = WL_KEYBOARD_KEY_STATE_RELEASED,
				};
				// Calls handleKey(), which will remove from pressed
				wlr_keyboard_notify_key(&group->state, &key_event);
			}
		}
	} else if (event->state == WL_KEYBOARD_KEY_STATE_PRESSED) {
		if (group->pressed.len < RV_KEYBOARD_GROUP_PRESSED_COUNT_MAX) {
			struct wlr_keyboard_key_event key_event = {
				.time_msec = event->time_msec,
				.keycode = event->keycode,
				.update_state = true,
				.state = WL_KEYBOARD_KEY_STATE_PRESSED,
			};
			// Calls handleKey(), which will add to pressed
			wlr_keyboard_notify_key(&group->state, &key_event);
		}
	}
	// Release events without a prior press event are ignored.
}

static void handle_key(struct wl_listener *listener, void *data) {
	struct rv_keyboard_group *group = wl_container_of(listener, group, key);
	struct wlr_keyboard_key_event *event = data;

	struct xkb_state *xkb_state = group->state.xkb_state;
	if (xkb_state == NULL) {
		rv_err(RV_SCOPE_INPUT, "no xkb_state available");
		return;
	}

	{
		struct rv_keyboard_group *g, *tmp;
		wl_list_for_each_safe(g, tmp, &group->seat->keyboard_groups, link) {
			for (uint32_t i = 0; i < g->pressed.len; i++) {
				struct rv_keyboard_group_press press = g->pressed.values[i];
				if (press.consumer.tag != RV_KEYBOARD_GROUP_KEY_CONSUMER_BINDING) {
					continue;
				}
				struct rv_xkb_binding *binding = press.consumer.binding;
				if (binding == NULL) {
					continue;
				}
				rv_xkb_binding_stop_repeat(binding);
			}
		}
	}

	// Every sent press event, to a regular client or the input method, should have
	// the corresponding release event sent to the same client.
	// Similarly, no press event means no release event.
	struct rv_keyboard_group_key_consumer consumer;
	if (event->state == WL_KEYBOARD_KEY_STATE_RELEASED) {
		// Decision is made on press; release only follows it
		// group.pressed.fetchSwapRemove(event.keycode).?
		int index = pressed_index(&group->pressed, event->keycode);
		assert(index >= 0);
		struct rv_keyboard_group_press value = group->pressed.values[index];
		uint32_t last = group->pressed.len - 1;
		group->pressed.keys[index] = group->pressed.keys[last];
		group->pressed.values[index] = group->pressed.values[last];
		group->pressed.len -= 1;
		assert(value.count == 0);
		consumer = value.consumer;
		goto consumer_found;
	}
	{
		// Translate libinput keycode -> xkbcommon
		xkb_keycode_t xkb_keycode = event->keycode + 8;
		uint32_t modifiers = wlr_keyboard_get_modifiers(&group->state);
		const xkb_keysym_t *syms = NULL;
		int syms_len = xkb_state_key_get_syms(xkb_state, xkb_keycode, &syms);
		for (int i = 0; i < syms_len; i++) {
			if (handle_builtin_binding(syms[i])) {
				rv_debug(RV_SCOPE_INPUT, "matched builtin binding");
				consumer = (struct rv_keyboard_group_key_consumer){ .tag = RV_KEYBOARD_GROUP_KEY_CONSUMER_BUILTIN };
				goto consumer_found;
			}
		}
		struct rv_xkb_binding *binding = rv_seat_match_xkb_binding(group->seat, xkb_keycode, modifiers, xkb_state);
		if (binding != NULL) {
			rv_debug(RV_SCOPE_INPUT, "matched xkb binding");
			group->seat->xkb_bindings_seat.ensure_next_key_eaten = false;
			consumer = (struct rv_keyboard_group_key_consumer){
				.tag = RV_KEYBOARD_GROUP_KEY_CONSUMER_BINDING,
				.binding = binding->sent_pressed ? NULL : binding,
			};
			goto consumer_found;
		}
		if (group->seat->xkb_bindings_seat.ensure_next_key_eaten) {
			// This approach for filtering out modifiers feels like a hack.
			// Open questions:
			// - Are there keycodes that should be considered a modifier which
			//   are not yet checked by keysymIsModifier()?
			// - Is it possible to test whether keysymIsModifier() is complete?
			// - Is there a way to test the effect the keycode would have on
			//   the active modifiers of the xkb_state?
			// - Could we add a function to libxkbcommon to make that possible?
			syms_len = xkb_state_key_get_syms(xkb_state, xkb_keycode, &syms);
			for (int i = 0; i < syms_len; i++) {
				if (!keysym_is_modifier(syms[i])) {
					group->seat->xkb_bindings_seat.ensure_next_key_eaten = false;
					consumer = (struct rv_keyboard_group_key_consumer){
						.tag = RV_KEYBOARD_GROUP_KEY_CONSUMER_ENSURE_EATEN,
					};
					goto consumer_found;
				}
			}
		}
		if (get_input_method_grab(group) != NULL) {
			consumer = (struct rv_keyboard_group_key_consumer){ .tag = RV_KEYBOARD_GROUP_KEY_CONSUMER_IM_GRAB };
			goto consumer_found;
		}
		consumer = (struct rv_keyboard_group_key_consumer){ .tag = RV_KEYBOARD_GROUP_KEY_CONSUMER_FOCUS };
	}
consumer_found:

	if (event->state == WL_KEYBOARD_KEY_STATE_PRESSED) {
		// group.pressed.putAssumeCapacityNoClobber()
		assert(pressed_index(&group->pressed, event->keycode) < 0);
		assert(group->pressed.len < RV_KEYBOARD_GROUP_PRESSED_COUNT_MAX);
		group->pressed.keys[group->pressed.len] = event->keycode;
		group->pressed.values[group->pressed.len] = (struct rv_keyboard_group_press){
			.consumer = consumer,
			.count = 1,
		};
		group->pressed.len += 1;
	}

	switch (consumer.tag) {
	case RV_KEYBOARD_GROUP_KEY_CONSUMER_BUILTIN:
		break;
	case RV_KEYBOARD_GROUP_KEY_CONSUMER_BINDING:
		if (consumer.binding != NULL) {
			if (event->state == WL_KEYBOARD_KEY_STATE_PRESSED) {
				rv_xkb_binding_pressed(consumer.binding);
			} else {
				rv_xkb_binding_released(consumer.binding);
			}
		}
		break;
	case RV_KEYBOARD_GROUP_KEY_CONSUMER_ENSURE_EATEN:
		if (event->state == WL_KEYBOARD_KEY_STATE_PRESSED) {
			group->seat->xkb_bindings_seat.scheduled.ate_unbound_key = true;
			rv_window_manager_dirty_windowing(&server.wm);
		}
		break;
	case RV_KEYBOARD_GROUP_KEY_CONSUMER_IM_GRAB: {
		struct wlr_input_method_keyboard_grab_v2 *keyboard_grab = get_input_method_grab(group);
		if (keyboard_grab != NULL) {
			wlr_input_method_keyboard_grab_v2_set_keyboard(keyboard_grab, &group->state);
			wlr_input_method_keyboard_grab_v2_send_key(keyboard_grab, event->time_msec, event->keycode,
				event->state);
		}
		break;
	}
	case RV_KEYBOARD_GROUP_KEY_CONSUMER_FOCUS:
		wlr_seat_set_keyboard(group->seat->wlr_seat, &group->state);
		wlr_seat_keyboard_notify_key(group->seat->wlr_seat, event->time_msec, event->keycode, event->state);
		break;
	}

	rv_keyboard_group_send_state(group);
}

static bool keysym_is_modifier(xkb_keysym_t keysym) {
	switch (keysym) {
	case XKB_KEY_Shift_L:
	case XKB_KEY_Shift_R:
	case XKB_KEY_Control_L:
	case XKB_KEY_Control_R:
	case XKB_KEY_Caps_Lock:
	case XKB_KEY_Shift_Lock:

	case XKB_KEY_Meta_L:
	case XKB_KEY_Meta_R:
	case XKB_KEY_Alt_L:
	case XKB_KEY_Alt_R:
	case XKB_KEY_Super_L:
	case XKB_KEY_Super_R:
	case XKB_KEY_Hyper_L:
	case XKB_KEY_Hyper_R:

	case XKB_KEY_Num_Lock:

	case XKB_KEY_ISO_Lock:
	case XKB_KEY_ISO_Level2_Latch:
	case XKB_KEY_ISO_Level3_Shift:
	case XKB_KEY_ISO_Level3_Latch:
	case XKB_KEY_ISO_Level3_Lock:
	case XKB_KEY_ISO_Level5_Shift:
	case XKB_KEY_ISO_Level5_Latch:
	case XKB_KEY_ISO_Level5_Lock:
	case XKB_KEY_ISO_Group_Shift:
	case XKB_KEY_ISO_Group_Latch:
	case XKB_KEY_ISO_Group_Lock:
	case XKB_KEY_ISO_Next_Group:
	case XKB_KEY_ISO_Next_Group_Lock:
	case XKB_KEY_ISO_Prev_Group:
	case XKB_KEY_ISO_Prev_Group_Lock:
	case XKB_KEY_ISO_First_Group:
	case XKB_KEY_ISO_First_Group_Lock:
	case XKB_KEY_ISO_Last_Group:
	case XKB_KEY_ISO_Last_Group_Lock:
		return true;
	default:
		return false;
	}
}

void rv_keyboard_group_process_modifiers(struct rv_keyboard_group *group, struct wlr_keyboard_modifiers modifiers) {
	wlr_keyboard_notify_modifiers(&group->state, modifiers.depressed, modifiers.latched, modifiers.locked,
		modifiers.group);
}

static void handle_modifiers(struct wl_listener *listener, void *data) {
	struct rv_keyboard_group *group = wl_container_of(listener, group, modifiers);

	{
		uint32_t old = group->modifiers_old;
		uint32_t new = wlr_keyboard_get_modifiers(&group->state);
		uint32_t watched = group->seat->xkb_bindings_seat.requested.mods_watched;
		if ((old & watched) != (new & watched)) {
			group->seat->xkb_bindings_seat.scheduled.has_mods_update = true;
			group->seat->xkb_bindings_seat.scheduled.mods_update = (struct rv_xkb_bindings_seat_scheduled_mods_update){
				.old = old,
				.new = new,
			};
			rv_window_manager_dirty_windowing(&server.wm);
		}
		group->modifiers_old = new;
	}

	struct wlr_input_method_keyboard_grab_v2 *keyboard_grab = get_input_method_grab(group);
	if (keyboard_grab != NULL) {
		wlr_input_method_keyboard_grab_v2_set_keyboard(keyboard_grab, &group->state);
		wlr_input_method_keyboard_grab_v2_send_modifiers(keyboard_grab, &group->state.modifiers);
	} else {
		wlr_seat_set_keyboard(group->seat->wlr_seat, &group->state);
		wlr_seat_keyboard_notify_modifiers(group->seat->wlr_seat, &group->state.modifiers);
	}
	rv_keyboard_group_send_state(group);
}

// Handle any builtin, hardcoded compositor keybindings such as VT switching.
// Returns true if the keysym was handled.
static bool handle_builtin_binding(xkb_keysym_t keysym) {
	if (keysym >= XKB_KEY_XF86Switch_VT_1 && keysym <= XKB_KEY_XF86Switch_VT_12) {
		rv_debug(RV_SCOPE_INPUT, "switch VT keysym received");
		if (server.session != NULL) {
			unsigned int vt = keysym - XKB_KEY_XF86Switch_VT_1 + 1;
			rv_info(RV_SCOPE_DEFAULT, "switching to VT %u", vt);
#if WLR_HAS_SESSION
			if (!wlr_session_change_vt(server.session, vt)) {
				rv_err(RV_SCOPE_DEFAULT, "changing VT failed");
			}
#endif
		}
		return true;
	}
	return false;
}

// Returns null if the keyboard is not grabbed by an input method,
// or if the group is for a virtual keyboard.
// TODO: it would be good if virtual keyboards that are not associated with the
// input method client would pass through the input method grab.
// See https://gitlab.freedesktop.org/wlroots/wlroots/-/issues/2322
static struct wlr_input_method_keyboard_grab_v2 *get_input_method_grab(struct rv_keyboard_group *group) {
	if (group->virtual) {
		return NULL;
	}
	struct wlr_input_method_v2 *input_method = group->seat->relay.input_method;
	if (input_method != NULL) {
		if (input_method->keyboard_grab != NULL) {
			return input_method->keyboard_grab;
		}
	}
	return NULL;
}

void rv_keyboard_group_process_keymap(struct rv_keyboard_group *group, struct xkb_keymap *keymap) {
	// wlroots will log an error on failure, there's not much we can do to recover unfortunately.
	(void)wlr_keyboard_set_keymap(&group->state, keymap);
}

void rv_keyboard_group_send_state(struct rv_keyboard_group *group) {
	struct xkb_keymap *keymap = group->config.keymap;
	assert(keymap != NULL); // group.config.keymap.?
	xkb_layout_index_t layout_index = group->state.modifiers.group;
	const char *layout_name = xkb_keymap_layout_get_name(keymap, layout_index);
	xkb_mod_mask_t caps_mask = xkb_keymap_mod_get_mask(keymap, XKB_MOD_NAME_CAPS);
	bool capslock = (group->state.modifiers.locked & caps_mask) != 0;
	xkb_mod_mask_t num_mask = xkb_keymap_mod_get_mask(keymap, XKB_VMOD_NAME_NUM);
	bool numlock = (group->state.modifiers.locked & num_mask) != 0;

	struct rv_xkb_keyboard *xkb_keyboard, *tmp;
	wl_list_for_each_safe(xkb_keyboard, tmp, &server.xkb_config.keyboards, link) {
		struct rv_input_device *device = wl_container_of(xkb_keyboard, device, xkb_keyboard);
		struct rv_keyboard *keyboard = wl_container_of(device, keyboard, device);
		if (keyboard->group != group) {
			continue;
		}

		rv_xkb_keyboard_send_state(xkb_keyboard, layout_index, layout_name, capslock, numlock);
	}
}

static void led_update(struct wlr_keyboard *state, uint32_t leds) {
	struct rv_keyboard_group *group = wl_container_of(state, group, state);
	struct rv_input_device *device, *tmp;
	wl_list_for_each_safe(device, tmp, &server.input_manager.devices, link) {
		if (device->wlr_device->type != WLR_INPUT_DEVICE_KEYBOARD) {
			continue;
		}
		struct rv_keyboard *keyboard = wl_container_of(device, keyboard, device);
		if (keyboard->group != group) {
			continue;
		}
		struct wlr_keyboard *wlr_keyboard = wlr_keyboard_from_input_device(device->wlr_device);
		wlr_keyboard_led_update(wlr_keyboard, leds);
	}
}
