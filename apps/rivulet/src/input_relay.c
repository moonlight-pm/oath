// SPDX-FileCopyrightText: © 2021 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/InputRelay.zig for Oath, 2026-10-18.

#include "rivulet.h"

static void handle_input_method_commit(struct wl_listener *listener, void *data);
static void handle_input_method_grab_keyboard(struct wl_listener *listener, void *data);
static void handle_input_method_destroy(struct wl_listener *listener, void *data);
static void handle_input_method_new_popup(struct wl_listener *listener, void *data);
static void handle_input_method_grab_keyboard_destroy(struct wl_listener *listener, void *data);

void rv_input_relay_init(struct rv_input_relay *relay) {
	*relay = (struct rv_input_relay){
		.input_method = NULL,
		.text_input = NULL,
	};
	relay->input_method_commit.notify = handle_input_method_commit;
	relay->grab_keyboard.notify = handle_input_method_grab_keyboard;
	relay->input_method_destroy.notify = handle_input_method_destroy;
	relay->input_method_new_popup.notify = handle_input_method_new_popup;
	relay->grab_keyboard_destroy.notify = handle_input_method_grab_keyboard_destroy;

	wl_list_init(&relay->text_inputs);
	wl_list_init(&relay->input_popups);
}

void rv_input_relay_new_input_method(struct rv_input_relay *relay, struct wlr_input_method_v2 *input_method) {
	struct rv_seat *seat = wl_container_of(relay, seat, relay);

	rv_debug(RV_SCOPE_INPUT, "new input method on seat %s", seat->wlr_seat->name);

	// Only one input_method can be bound to a seat.
	if (relay->input_method != NULL) {
		rv_info(RV_SCOPE_INPUT, "seat %s already has an input method", seat->wlr_seat->name);
		wlr_input_method_v2_send_unavailable(input_method);
		return;
	}

	relay->input_method = input_method;

	wl_signal_add(&input_method->events.commit, &relay->input_method_commit);
	wl_signal_add(&input_method->events.grab_keyboard, &relay->grab_keyboard);
	wl_signal_add(&input_method->events.destroy, &relay->input_method_destroy);
	wl_signal_add(&input_method->events.new_popup_surface, &relay->input_method_new_popup);

	struct wlr_surface *surface = rv_seat_focus_surface(seat->focused);
	if (surface != NULL) {
		rv_input_relay_focus(relay, surface);
	}
}

static void handle_input_method_commit(struct wl_listener *listener, void *data) {
	struct rv_input_relay *relay = wl_container_of(listener, relay, input_method_commit);
	assert(relay->input_method != NULL);
	struct wlr_input_method_v2 *input_method = relay->input_method;

	if (!input_method->client_active) {
		return;
	}
	struct rv_text_input *text_input = relay->text_input;
	if (text_input == NULL) {
		return;
	}

	if (input_method->current.preedit.text != NULL) {
		char *preedit_text = input_method->current.preedit.text;
		wlr_text_input_v3_send_preedit_string(
			text_input->wlr_text_input,
			preedit_text,
			input_method->current.preedit.cursor_begin,
			input_method->current.preedit.cursor_end
		);
	}

	if (input_method->current.commit_text != NULL) {
		char *commit_text = input_method->current.commit_text;
		wlr_text_input_v3_send_commit_string(text_input->wlr_text_input, commit_text);
	}

	if (input_method->current.delete.before_length != 0 ||
			input_method->current.delete.after_length != 0) {
		wlr_text_input_v3_send_delete_surrounding_text(
			text_input->wlr_text_input,
			input_method->current.delete.before_length,
			input_method->current.delete.after_length
		);
	}

	wlr_text_input_v3_send_done(text_input->wlr_text_input);
}

static void handle_input_method_destroy(struct wl_listener *listener, void *data) {
	struct rv_input_relay *relay = wl_container_of(listener, relay, input_method_destroy);

	wl_list_remove(&relay->input_method_commit.link);
	wl_list_remove(&relay->grab_keyboard.link);
	wl_list_remove(&relay->input_method_destroy.link);
	wl_list_remove(&relay->input_method_new_popup.link);
	relay->input_method = NULL;

	rv_input_relay_focus(relay, NULL);

	assert(relay->text_input == NULL);
}

static void handle_input_method_grab_keyboard(struct wl_listener *listener, void *data) {
	struct rv_input_relay *relay = wl_container_of(listener, relay, grab_keyboard);
	struct wlr_input_method_keyboard_grab_v2 *keyboard_grab = data;
	struct rv_seat *seat = wl_container_of(relay, seat, relay);

	struct wlr_keyboard *active_keyboard = wlr_seat_get_keyboard(seat->wlr_seat);
	wlr_input_method_keyboard_grab_v2_set_keyboard(keyboard_grab, active_keyboard);

	wl_signal_add(&keyboard_grab->events.destroy, &relay->grab_keyboard_destroy);
}

static void handle_input_method_new_popup(struct wl_listener *listener, void *data) {
	struct rv_input_relay *relay = wl_container_of(listener, relay, input_method_new_popup);
	struct wlr_input_popup_surface_v2 *wlr_popup = data;

	if (!rv_input_popup_create(wlr_popup, relay)) {
		rv_err(RV_SCOPE_INPUT, "out of memory");
		return;
	}
}

static void handle_input_method_grab_keyboard_destroy(struct wl_listener *listener, void *data) {
	struct rv_input_relay *relay = wl_container_of(listener, relay, grab_keyboard_destroy);
	assert(relay->input_method != NULL);
	struct wlr_input_method_v2 *input_method = relay->input_method;
	assert(input_method->keyboard_grab != NULL);
	struct wlr_input_method_keyboard_grab_v2 *keyboard_grab = input_method->keyboard_grab;
	wl_list_remove(&relay->grab_keyboard_destroy.link);

	if (keyboard_grab->keyboard != NULL) {
		struct wlr_keyboard *keyboard = keyboard_grab->keyboard;
		wlr_seat_keyboard_notify_modifiers(input_method->seat, &keyboard->modifiers);
	}
}

void rv_input_relay_disable_text_input(struct rv_input_relay *relay) {
	assert(relay->text_input != NULL);
	relay->text_input = NULL;

	if (relay->input_method != NULL) {
		struct wlr_input_method_v2 *input_method = relay->input_method;
		{
			struct rv_input_popup *popup, *tmp;
			wl_list_for_each_safe(popup, tmp, &relay->input_popups, link) {
				rv_input_popup_update(popup);
			}
		}
		wlr_input_method_v2_send_deactivate(input_method);
		wlr_input_method_v2_send_done(input_method);
	}
}

void rv_input_relay_send_input_method_state(struct rv_input_relay *relay) {
	assert(relay->input_method != NULL);
	struct wlr_input_method_v2 *input_method = relay->input_method;
	assert(relay->text_input != NULL);
	struct wlr_text_input_v3 *wlr_text_input = relay->text_input->wlr_text_input;

	// TODO Send these events only if something changed.
	// On activation all events must be sent for all active features.

	if (wlr_text_input->active_features & WLR_TEXT_INPUT_V3_FEATURE_SURROUNDING_TEXT) {
		if (wlr_text_input->current.surrounding.text != NULL) {
			char *text = wlr_text_input->current.surrounding.text;
			wlr_input_method_v2_send_surrounding_text(
				input_method,
				text,
				wlr_text_input->current.surrounding.cursor,
				wlr_text_input->current.surrounding.anchor
			);
		}
	}

	wlr_input_method_v2_send_text_change_cause(input_method, wlr_text_input->current.text_change_cause);

	if (wlr_text_input->active_features & WLR_TEXT_INPUT_V3_FEATURE_CONTENT_TYPE) {
		wlr_input_method_v2_send_content_type(
			input_method,
			wlr_text_input->current.content_type.hint,
			wlr_text_input->current.content_type.purpose
		);
	}

	{
		struct rv_input_popup *popup, *tmp;
		wl_list_for_each_safe(popup, tmp, &relay->input_popups, link) {
			rv_input_popup_update(popup);
		}
	}

	wlr_input_method_v2_send_done(input_method);
}

void rv_input_relay_focus(struct rv_input_relay *relay, struct wlr_surface *new_focus) {
	// Send leave events
	{
		struct rv_text_input *text_input, *tmp;
		wl_list_for_each_safe(text_input, tmp, &relay->text_inputs, link) {
			if (text_input->wlr_text_input->focused_surface != NULL) {
				struct wlr_surface *surface = text_input->wlr_text_input->focused_surface;
				// This function should not be called unless focus changes
				assert(surface != new_focus);
				(void)surface;
				wlr_text_input_v3_send_leave(text_input->wlr_text_input);
			}
		}
	}

	// Clear currently enabled text input
	if (relay->text_input != NULL) {
		rv_input_relay_disable_text_input(relay);
	}

	// Send enter events if we have an input method.
	// No text input for the new surface should be enabled yet as the client
	// should wait until it receives an enter event.
	if (new_focus != NULL) {
		struct wlr_surface *surface = new_focus;
		if (relay->input_method != NULL) {
			struct rv_text_input *text_input, *tmp;
			wl_list_for_each_safe(text_input, tmp, &relay->text_inputs, link) {
				if (wl_resource_get_client(text_input->wlr_text_input->resource) ==
						wl_resource_get_client(surface->resource)) {
					wlr_text_input_v3_send_enter(text_input->wlr_text_input, surface);
				}
			}
		}
	}
}
