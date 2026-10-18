// SPDX-FileCopyrightText: © 2021 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/TextInput.zig for Oath, 2026-10-18.

#include "rivulet.h"

static void handle_enable(struct wl_listener *listener, void *data);
static void handle_commit(struct wl_listener *listener, void *data);
static void handle_disable(struct wl_listener *listener, void *data);
static void handle_destroy(struct wl_listener *listener, void *data);

bool rv_text_input_create(struct wlr_text_input_v3 *wlr_text_input) {
	struct rv_seat *seat = wlr_text_input->seat->data;

	struct rv_text_input *text_input = calloc(1, sizeof(*text_input));
	if (text_input == NULL) {
		return false;
	}

	rv_debug(RV_SCOPE_INPUT, "new text input on seat %s", seat->wlr_seat->name);

	*text_input = (struct rv_text_input){
		.wlr_text_input = wlr_text_input,
	};
	text_input->enable.notify = handle_enable;
	text_input->commit.notify = handle_commit;
	text_input->disable.notify = handle_disable;
	text_input->destroy.notify = handle_destroy;

	wl_list_insert(seat->relay.text_inputs.prev, &text_input->link);

	wl_signal_add(&wlr_text_input->events.enable, &text_input->enable);
	wl_signal_add(&wlr_text_input->events.commit, &text_input->commit);
	wl_signal_add(&wlr_text_input->events.disable, &text_input->disable);
	wl_signal_add(&wlr_text_input->events.destroy, &text_input->destroy);

	return true;
}

static void handle_enable(struct wl_listener *listener, void *data) {
	struct rv_text_input *text_input = wl_container_of(listener, text_input, enable);
	struct rv_seat *seat = text_input->wlr_text_input->seat->data;

	if (text_input->wlr_text_input->focused_surface == NULL) {
		rv_err(RV_SCOPE_INPUT, "client requested to enable text input without focus, ignoring request");
		return;
	}

	// The same text_input object may be enabled multiple times consecutively
	// without first disabling it. Enabling a different text input object without
	// first disabling the current one is disallowed by the protocol however.
	if (seat->relay.text_input != NULL) {
		struct rv_text_input *currently_enabled = seat->relay.text_input;
		if (text_input != currently_enabled) {
			rv_err(RV_SCOPE_INPUT,
				"client requested to enable more than one text input on a single seat, ignoring request");
			return;
		}
	}

	seat->relay.text_input = text_input;

	if (seat->relay.input_method != NULL) {
		struct wlr_input_method_v2 *input_method = seat->relay.input_method;
		wlr_input_method_v2_send_activate(input_method);
		rv_input_relay_send_input_method_state(&seat->relay);
	}
}

static void handle_commit(struct wl_listener *listener, void *data) {
	struct rv_text_input *text_input = wl_container_of(listener, text_input, commit);
	struct rv_seat *seat = text_input->wlr_text_input->seat->data;

	if (seat->relay.text_input != text_input) {
		rv_err(RV_SCOPE_INPUT, "inactive text input tried to commit an update, client bug?");
		return;
	}

	if (seat->relay.input_method != NULL) {
		rv_input_relay_send_input_method_state(&seat->relay);
	}
}

static void handle_disable(struct wl_listener *listener, void *data) {
	struct rv_text_input *text_input = wl_container_of(listener, text_input, disable);
	struct rv_seat *seat = text_input->wlr_text_input->seat->data;

	if (seat->relay.text_input == text_input) {
		rv_input_relay_disable_text_input(&seat->relay);
	}
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_text_input *text_input = wl_container_of(listener, text_input, destroy);
	struct rv_seat *seat = text_input->wlr_text_input->seat->data;

	if (seat->relay.text_input == text_input) {
		rv_input_relay_disable_text_input(&seat->relay);
	}

	wl_list_remove(&text_input->enable.link);
	wl_list_remove(&text_input->commit.link);
	wl_list_remove(&text_input->disable.link);
	wl_list_remove(&text_input->destroy.link);

	wl_list_remove(&text_input->link);
	free(text_input);
}
