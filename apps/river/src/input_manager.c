// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/InputManager.zig for Oath, 2026-10-18.

#include "river.h"

static const char *const default_seat_name = "default";

static void bind(struct wl_client *client, void *data, uint32_t version, uint32_t id);
static void handle_new_input(struct wl_listener *listener, void *data);
static void handle_new_virtual_pointer(struct wl_listener *listener, void *data);
static void handle_new_virtual_keyboard(struct wl_listener *listener, void *data);
static void handle_new_constraint(struct wl_listener *listener, void *data);
static void handle_new_input_method(struct wl_listener *listener, void *data);
static void handle_new_text_input(struct wl_listener *listener, void *data);

bool rv_input_manager_init(struct rv_input_manager *input_manager) {
	struct rv_input_manager im = {0};

	im.global = wl_global_create(server.wl_server, &river_input_manager_v1_interface, 1, input_manager, bind);
	if (im.global == NULL) {
		return false;
	}
	// These are automatically freed when the display is destroyed
	im.idle_notifier = wlr_idle_notifier_v1_create(server.wl_server);
	if (im.idle_notifier == NULL) {
		return false;
	}
	im.relative_pointer_manager = wlr_relative_pointer_manager_v1_create(server.wl_server);
	if (im.relative_pointer_manager == NULL) {
		return false;
	}
	im.pointer_gestures = wlr_pointer_gestures_v1_create(server.wl_server);
	if (im.pointer_gestures == NULL) {
		return false;
	}
	im.virtual_pointer_manager = wlr_virtual_pointer_manager_v1_create(server.wl_server);
	if (im.virtual_pointer_manager == NULL) {
		return false;
	}
	im.virtual_keyboard_manager = wlr_virtual_keyboard_manager_v1_create(server.wl_server);
	if (im.virtual_keyboard_manager == NULL) {
		return false;
	}
	im.pointer_constraints = wlr_pointer_constraints_v1_create(server.wl_server);
	if (im.pointer_constraints == NULL) {
		return false;
	}
	im.input_method_manager = wlr_input_method_manager_v2_create(server.wl_server);
	if (im.input_method_manager == NULL) {
		return false;
	}
	im.text_input_manager = wlr_text_input_manager_v3_create(server.wl_server);
	if (im.text_input_manager == NULL) {
		return false;
	}
	im.tablet_manager = wlr_tablet_v2_create(server.wl_server);
	if (im.tablet_manager == NULL) {
		return false;
	}

	*input_manager = im;
	input_manager->new_input.notify = handle_new_input;
	input_manager->new_virtual_pointer.notify = handle_new_virtual_pointer;
	input_manager->new_virtual_keyboard.notify = handle_new_virtual_keyboard;
	input_manager->new_constraint.notify = handle_new_constraint;
	input_manager->new_input_method.notify = handle_new_input_method;
	input_manager->new_text_input.notify = handle_new_text_input;

	wl_list_init(&input_manager->objects);
	wl_list_init(&input_manager->devices);
	wl_list_init(&input_manager->seats);

	if (!rv_seat_create(default_seat_name)) {
		return false;
	}

#if WLR_HAS_XWAYLAND
	if (server.xwayland != NULL) {
		wlr_xwayland_set_seat(server.xwayland, rv_input_manager_default_seat(input_manager)->wlr_seat);
	}
#endif

	wl_signal_add(&server.backend->events.new_input, &input_manager->new_input);
	wl_signal_add(&input_manager->virtual_pointer_manager->events.new_virtual_pointer,
		&input_manager->new_virtual_pointer);
	wl_signal_add(&input_manager->virtual_keyboard_manager->events.new_virtual_keyboard,
		&input_manager->new_virtual_keyboard);
	wl_signal_add(&input_manager->pointer_constraints->events.new_constraint, &input_manager->new_constraint);
	wl_signal_add(&input_manager->input_method_manager->events.new_input_method,
		&input_manager->new_input_method);
	wl_signal_add(&input_manager->text_input_manager->events.new_text_input, &input_manager->new_text_input);
	return true;
}

void rv_input_manager_deinit(struct rv_input_manager *input_manager) {
	wl_global_destroy(input_manager->global);

	// This function must be called after the backend has been destroyed
	assert(wl_list_empty(&input_manager->objects));
	assert(wl_list_empty(&input_manager->devices));

	wl_list_remove(&input_manager->new_virtual_pointer.link);
	wl_list_remove(&input_manager->new_virtual_keyboard.link);
	wl_list_remove(&input_manager->new_constraint.link);
	wl_list_remove(&input_manager->new_input_method.link);
	wl_list_remove(&input_manager->new_text_input.link);

	while (!wl_list_empty(&input_manager->seats)) {
		struct rv_seat *seat = wl_container_of(input_manager->seats.next, seat, link);
		rv_seat_destroy(seat);
	}
}

static const struct river_input_manager_v1_interface im_impl;
static const struct river_input_manager_v1_interface im_inert_impl;
static void handle_destroy(struct wl_resource *im_v1);

static void bind(struct wl_client *client, void *data, uint32_t version, uint32_t id) {
	struct rv_input_manager *im = data;
	struct wl_resource *im_v1 = wl_resource_create(client, &river_input_manager_v1_interface, version, id);
	if (im_v1 == NULL) {
		wl_client_post_no_memory(client);
		rv_err(RV_SCOPE_INPUT, "out of memory");
		return;
	}
	wl_resource_set_implementation(im_v1, &im_impl, im, handle_destroy);
	wl_list_insert(im->objects.prev, wl_resource_get_link(im_v1));
	{
		struct rv_input_device *device, *tmp;
		wl_list_for_each_safe(device, tmp, &im->devices, link) {
			if (!device->virtual) {
				rv_input_device_create_object(device, im_v1);
			}
		}
	}
}

static void request_inert_stop(struct wl_client *client, struct wl_resource *im_v1) {
}

static void request_inert_destroy(struct wl_client *client, struct wl_resource *im_v1) {
	wl_resource_destroy(im_v1);
}

static void request_inert_create_seat(struct wl_client *client, struct wl_resource *im_v1, const char *name) {
}

static void request_inert_destroy_seat(struct wl_client *client, struct wl_resource *im_v1, const char *name) {
}

static const struct river_input_manager_v1_interface im_inert_impl = {
	.stop = request_inert_stop,
	.destroy = request_inert_destroy,
	.create_seat = request_inert_create_seat,
	.destroy_seat = request_inert_destroy_seat,
};

static void handle_destroy(struct wl_resource *im_v1) {
	wl_list_remove(wl_resource_get_link(im_v1));
}

static void request_stop(struct wl_client *client, struct wl_resource *im_v1) {
	wl_list_remove(wl_resource_get_link(im_v1));
	river_input_manager_v1_send_finished(im_v1);
	wl_resource_set_implementation(im_v1, &im_inert_impl, NULL, NULL);
}

static void request_destroy(struct wl_client *client, struct wl_resource *im_v1) {
	wl_resource_post_error(im_v1, RIVER_INPUT_MANAGER_V1_ERROR_INVALID_DESTROY, "%s",
		"destroy before finished event sent");
}

static void request_create_seat(struct wl_client *client, struct wl_resource *im_v1, const char *name) {
	struct rv_input_manager *im = wl_resource_get_user_data(im_v1);
	struct rv_seat *seat, *tmp;
	wl_list_for_each_safe(seat, tmp, &im->seats, link) {
		if (strcmp(name, seat->wlr_seat->name) == 0) {
			return;
		}
	}
	if (!rv_seat_create(name)) {
		wl_client_post_no_memory(wl_resource_get_client(im_v1));
		rv_err(RV_SCOPE_INPUT, "out of memory");
		return;
	}
}

static void request_destroy_seat(struct wl_client *client, struct wl_resource *im_v1, const char *name) {
	struct rv_input_manager *im = wl_resource_get_user_data(im_v1);
	struct wl_list *link = im->seats.next;
	// skip default seat
	if (link != &im->seats) {
		link = link->next;
	}
	for (; link != &im->seats; link = link->next) {
		struct rv_seat *seat = wl_container_of(link, seat, link);
		if (strcmp(name, seat->wlr_seat->name) == 0) {
			seat->destroying = true;
			rv_window_manager_dirty_windowing(&server.wm);
			break;
		}
	}
}

static const struct river_input_manager_v1_interface im_impl = {
	.stop = request_stop,
	.destroy = request_destroy,
	.create_seat = request_create_seat,
	.destroy_seat = request_destroy_seat,
};

struct rv_seat *rv_input_manager_default_seat(struct rv_input_manager *input_manager) {
	assert(!wl_list_empty(&input_manager->seats)); // seats.first().?
	struct rv_seat *seat = wl_container_of(input_manager->seats.next, seat, link);
	return seat;
}

void rv_input_manager_process_events(struct rv_input_manager *input_manager) {
	assert(server.wm.state.tag == RV_WINDOW_MANAGER_STATE_IDLE);

	struct rv_seat *seat, *tmp;
	wl_list_for_each_safe(seat, tmp, &input_manager->seats, link) {
		rv_seat_process_events(seat);
	}
}

static void handle_new_input(struct wl_listener *listener, void *data) {
	struct rv_input_manager *input_manager = wl_container_of(listener, input_manager, new_input);
	struct wlr_input_device *wlr_device = data;

	rv_seat_attach_new_device(rv_input_manager_default_seat(input_manager), wlr_device, false);
}

static void handle_new_virtual_pointer(struct wl_listener *listener, void *data) {
	struct rv_input_manager *input_manager = wl_container_of(listener, input_manager, new_virtual_pointer);
	struct wlr_virtual_pointer_v1_new_pointer_event *event = data;

	// TODO Support multiple seats and don't ignore
	if (event->suggested_seat != NULL) {
		rv_debug(RV_SCOPE_INPUT, "Ignoring seat suggestion from virtual pointer");
	}
	// TODO dont ignore output suggestion
	if (event->suggested_output != NULL) {
		rv_debug(RV_SCOPE_INPUT, "Ignoring output suggestion from virtual pointer");
	}

	rv_seat_attach_new_device(rv_input_manager_default_seat(input_manager), &event->new_pointer->pointer.base, true);
}

// InputManager.NoKeymapVirtKeyboard (file-private in Zig):
// Ignore virtual keyboards completely until the client sets a keymap
// Yes, wlroots should probably do this for us.
struct rv_input_manager_no_keymap_virt_keyboard {
	struct wlr_virtual_keyboard_v1 *virtual_keyboard;
	struct wl_listener destroy; // default: .init(handleVirtKeyboardDestroy)
	struct wl_listener keymap; // default: .init(handleKeymap)
};

static void handle_virt_keyboard_destroy(struct wl_listener *listener, void *data) {
	struct rv_input_manager_no_keymap_virt_keyboard *no_keymap = wl_container_of(listener, no_keymap, destroy);

	wl_list_remove(&no_keymap->destroy.link);
	wl_list_remove(&no_keymap->keymap.link);

	free(no_keymap);
}

static void handle_keymap(struct wl_listener *listener, void *data) {
	struct rv_input_manager_no_keymap_virt_keyboard *no_keymap = wl_container_of(listener, no_keymap, keymap);
	struct wlr_virtual_keyboard_v1 *virtual_keyboard = no_keymap->virtual_keyboard;

	handle_virt_keyboard_destroy(&no_keymap->destroy, &virtual_keyboard->keyboard.base);

	struct rv_seat *seat = virtual_keyboard->seat->data;
	rv_seat_attach_new_device(seat, &virtual_keyboard->keyboard.base, true);
}

static void handle_new_virtual_keyboard(struct wl_listener *listener, void *data) {
	struct wlr_virtual_keyboard_v1 *virtual_keyboard = data;

	struct rv_input_manager_no_keymap_virt_keyboard *no_keymap = calloc(1, sizeof(*no_keymap));
	if (no_keymap == NULL) {
		rv_err(RV_SCOPE_INPUT, "out of memory");
		return;
	}

	*no_keymap = (struct rv_input_manager_no_keymap_virt_keyboard){
		.virtual_keyboard = virtual_keyboard,
	};
	no_keymap->destroy.notify = handle_virt_keyboard_destroy;
	no_keymap->keymap.notify = handle_keymap;
	wl_signal_add(&virtual_keyboard->keyboard.base.events.destroy, &no_keymap->destroy);
	wl_signal_add(&virtual_keyboard->keyboard.events.keymap, &no_keymap->keymap);
}

static void handle_new_constraint(struct wl_listener *listener, void *data) {
	struct wlr_pointer_constraint_v1 *wlr_constraint = data;
	if (!rv_pointer_constraint_create(wlr_constraint)) {
		rv_err(RV_SCOPE_INPUT, "out of memory");
		wl_resource_post_no_memory(wlr_constraint->resource);
	}
}

static void handle_new_input_method(struct wl_listener *listener, void *data) {
	struct wlr_input_method_v2 *input_method = data;
	struct rv_seat *seat = input_method->seat->data;

	rv_debug(RV_SCOPE_INPUT, "new input method on seat %s", seat->wlr_seat->name);

	rv_input_relay_new_input_method(&seat->relay, input_method);
}

static void handle_new_text_input(struct wl_listener *listener, void *data) {
	struct wlr_text_input_v3 *wlr_text_input = data;
	if (!rv_text_input_create(wlr_text_input)) {
		rv_err(RV_SCOPE_INPUT, "out of memory");
		wl_resource_post_no_memory(wlr_text_input->resource);
		return;
	}
}
