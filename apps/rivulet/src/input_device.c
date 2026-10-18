// SPDX-FileCopyrightText: © 2022 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/InputDevice.zig for Oath, 2026-10-18.

#include "rivulet.h"

static void handle_remove(struct wl_listener *listener, void *data);
static void handle_destroy(struct wl_resource *object);
static const struct river_input_device_v1_interface device_impl;
static const struct river_input_device_v1_interface device_inert_impl;

// @tagName(wlr_device.type)
static const char *device_type_name(enum wlr_input_device_type type) {
	switch (type) {
	case WLR_INPUT_DEVICE_KEYBOARD:
		return "keyboard";
	case WLR_INPUT_DEVICE_POINTER:
		return "pointer";
	case WLR_INPUT_DEVICE_TOUCH:
		return "touch";
	case WLR_INPUT_DEVICE_TABLET:
		return "tablet";
	case WLR_INPUT_DEVICE_TABLET_PAD:
		return "tablet_pad";
	case WLR_INPUT_DEVICE_SWITCH:
		return "switch";
	}
	abort();
}

// zig-wlroots InputDevice.getLibinputDevice()
static struct libinput_device *get_libinput_device(struct wlr_input_device *wlr_device) {
#if WLR_HAS_LIBINPUT_BACKEND
	if (!wlr_input_device_is_libinput(wlr_device)) {
		return NULL;
	}
	return wlr_libinput_get_device_handle(wlr_device);
#else
	return NULL;
#endif
}

bool rv_input_device_init(struct rv_input_device *device, struct rv_seat *seat, struct wlr_input_device *wlr_device,
		bool virtual) {
	*device = (struct rv_input_device){
		.seat = seat,
		.wlr_device = wlr_device,
		.virtual = virtual,
		.config = {
			.scroll_factor = 1.0,
			.map_to_output = NULL,
			.map_to_rectangle = { .x = 0, .y = 0, .width = 0, .height = 0 },
		},
	};
	device->remove.notify = handle_remove;
	wl_list_init(&device->objects);
	wl_list_insert(server.input_manager.devices.prev, &device->link);

	wlr_device->data = device;
	wl_signal_add(&wlr_device->events.destroy, &device->remove);

	rv_debug(RV_SCOPE_INPUT, "new %sinput device: %s-%s",
		virtual ? "virtual " : "",
		device_type_name(wlr_device->type),
		wlr_device->name != NULL ? wlr_device->name : "unknown");

	if (!virtual) {
		struct wl_resource *im_v1, *tmp;
		wl_resource_for_each_safe(im_v1, tmp, &server.input_manager.objects) {
			rv_input_device_create_object(device, im_v1);
		}
		struct libinput_device *handle = get_libinput_device(wlr_device);
		if (handle != NULL) {
			rv_libinput_device_init(&device->libinput, handle);
		}
		if (wlr_device->type == WLR_INPUT_DEVICE_KEYBOARD) {
			rv_xkb_keyboard_init(&device->xkb_keyboard);
		}
	}

	// The wlroots Wayland and X11 backends support multiple outputs
	// exposed as multiple windows in the host session. However, this
	// requires mapping pointer/touch devices to the outputs suggested
	// by the backend to make input work as expected.
	const char *output_name = NULL;
	switch (wlr_device->type) {
	case WLR_INPUT_DEVICE_POINTER:
		output_name = wlr_pointer_from_input_device(wlr_device)->output_name;
		break;
	case WLR_INPUT_DEVICE_TOUCH:
		output_name = wlr_touch_from_input_device(wlr_device)->output_name;
		break;
	default:
		output_name = NULL;
		break;
	}
	if (output_name != NULL) {
		struct rv_output *output;
		wl_list_for_each(output, &server.om.outputs, link) {
			struct wlr_output *wlr_output = output->wlr_output;
			if (wlr_output == NULL) {
				continue;
			}
			if (strcmp(output_name, wlr_output->name) == 0) {
				device->config.map_to_output = wlr_output;
				break;
			}
		}
	}

	return true;
}

void rv_input_device_create_object(struct rv_input_device *device, struct wl_resource *im_v1) {
	assert(!device->virtual);
	enum river_input_device_v1_type device_type;
	switch (device->wlr_device->type) {
	case WLR_INPUT_DEVICE_KEYBOARD:
		device_type = RIVER_INPUT_DEVICE_V1_TYPE_KEYBOARD;
		break;
	case WLR_INPUT_DEVICE_POINTER:
		device_type = RIVER_INPUT_DEVICE_V1_TYPE_POINTER;
		break;
	case WLR_INPUT_DEVICE_TOUCH:
		device_type = RIVER_INPUT_DEVICE_V1_TYPE_TOUCH;
		break;
	case WLR_INPUT_DEVICE_TABLET:
		device_type = RIVER_INPUT_DEVICE_V1_TYPE_TABLET;
		break;
	case WLR_INPUT_DEVICE_SWITCH:
	case WLR_INPUT_DEVICE_TABLET_PAD:
		return;
	default:
		abort();
	}
	struct wl_resource *object = wl_resource_create(wl_resource_get_client(im_v1), &river_input_device_v1_interface,
		wl_resource_get_version(im_v1), 0);
	if (object == NULL) {
		rv_err(RV_SCOPE_INPUT, "out of memory");
		wl_resource_post_no_memory(im_v1);
		return;
	}
	river_input_manager_v1_send_input_device(im_v1, object);
	wl_list_insert(device->objects.prev, wl_resource_get_link(object));
	wl_resource_set_implementation(object, &device_impl, device, handle_destroy);
	river_input_device_v1_send_type(object, device_type);
	river_input_device_v1_send_name(object, device->wlr_device->name != NULL ? device->wlr_device->name : "");
}

void rv_input_device_deinit(struct rv_input_device *device) {
	if (!device->virtual) {
		{
			struct wl_resource *object, *tmp;
			wl_resource_for_each_safe(object, tmp, &device->objects) {
				wl_list_remove(wl_resource_get_link(object));
				river_input_device_v1_send_removed(object);
				wl_resource_set_implementation(object, &device_inert_impl, NULL, NULL);
			}
		}
		if (get_libinput_device(device->wlr_device) != NULL) {
			rv_libinput_device_deinit(&device->libinput);
		}
		if (device->wlr_device->type == WLR_INPUT_DEVICE_KEYBOARD) {
			rv_xkb_keyboard_deinit(&device->xkb_keyboard);
		}
	}

	wl_list_remove(&device->remove.link);
	wl_list_remove(&device->link);
	rv_seat_update_capabilities(device->seat);

	device->wlr_device->data = NULL;
}

void rv_input_device_assign_to_seat(struct rv_input_device *device, struct rv_seat *new) {
	struct rv_seat *old = device->seat;
	if (new == old) {
		return;
	}
	rv_seat_detach_device(old, device);
	rv_seat_attach_device(new, device);
	rv_seat_update_capabilities(old);
	rv_seat_update_capabilities(new);
}

// Retuns the curretly active mapping for the device, or an empty box if
// the movement of the device is unrestricted.
struct wlr_box rv_input_device_active_mapping(const struct rv_input_device *device) {
	struct wlr_box mapping = device->config.map_to_rectangle;
	if (!wlr_box_empty(&mapping)) {
		return mapping;
	}
	if (device->config.map_to_output != NULL) {
		wlr_output_layout_get_box(server.om.output_layout, device->config.map_to_output, &mapping);
	}
	return mapping;
}

static void handle_remove(struct wl_listener *listener, void *data) {
	struct rv_input_device *device = wl_container_of(listener, device, remove);

	rv_debug(RV_SCOPE_INPUT, "removed input device: %s-%s",
		device_type_name(device->wlr_device->type),
		device->wlr_device->name != NULL ? device->wlr_device->name : "unknown");

	switch (device->wlr_device->type) {
	case WLR_INPUT_DEVICE_KEYBOARD: {
		struct rv_keyboard *keyboard = wl_container_of(device, keyboard, device);
		rv_keyboard_device_destroy(keyboard);
		break;
	}
	case WLR_INPUT_DEVICE_POINTER:
	case WLR_INPUT_DEVICE_TOUCH:
		rv_input_device_deinit(device);
		free(device);
		break;
	case WLR_INPUT_DEVICE_TABLET: {
		struct rv_tablet *tablet = wl_container_of(device, tablet, device);
		rv_tablet_destroy(tablet);
		break;
	}
	case WLR_INPUT_DEVICE_SWITCH:
	case WLR_INPUT_DEVICE_TABLET_PAD:
	default:
		abort(); // unreachable
	}
}

static void request_inert_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

static void request_inert_assign_to_seat(struct wl_client *client, struct wl_resource *object, const char *name) {
}

static void request_inert_set_repeat_info(struct wl_client *client, struct wl_resource *object, int32_t rate,
		int32_t delay) {
}

static void request_inert_set_scroll_factor(struct wl_client *client, struct wl_resource *object,
		wl_fixed_t factor) {
}

static void request_inert_map_to_output(struct wl_client *client, struct wl_resource *object,
		struct wl_resource *output) {
}

static void request_inert_map_to_rectangle(struct wl_client *client, struct wl_resource *object, int32_t x,
		int32_t y, int32_t width, int32_t height) {
}

static const struct river_input_device_v1_interface device_inert_impl = {
	.destroy = request_inert_destroy,
	.assign_to_seat = request_inert_assign_to_seat,
	.set_repeat_info = request_inert_set_repeat_info,
	.set_scroll_factor = request_inert_set_scroll_factor,
	.map_to_output = request_inert_map_to_output,
	.map_to_rectangle = request_inert_map_to_rectangle,
};

static void handle_destroy(struct wl_resource *object) {
	wl_list_remove(wl_resource_get_link(object));
}

static void request_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

static void request_assign_to_seat(struct wl_client *client, struct wl_resource *object, const char *name) {
	struct rv_input_device *device = wl_resource_get_user_data(object);
	struct rv_seat *seat, *tmp;
	wl_list_for_each_safe(seat, tmp, &server.input_manager.seats, link) {
		if (strcmp(name, seat->wlr_seat->name) == 0) {
			rv_input_device_assign_to_seat(device, seat);
		}
	}
	rv_info(RV_SCOPE_INPUT, "client requested input device be assigned to non-existant seat '%s'", name);
}

static void request_set_repeat_info(struct wl_client *client, struct wl_resource *object, int32_t rate,
		int32_t delay) {
	struct rv_input_device *device = wl_resource_get_user_data(object);
	if (rate < 0 || delay < 0) {
		wl_resource_post_error(object, RIVER_INPUT_DEVICE_V1_ERROR_INVALID_REPEAT_INFO, "%s",
			"negative rate/delay");
		return;
	}
	if (device->wlr_device->type == WLR_INPUT_DEVICE_KEYBOARD) {
		struct rv_keyboard *keyboard = wl_container_of(device, keyboard, device);
		rv_keyboard_set_repeat_info(keyboard, rate, delay);
	}
}

static void request_set_scroll_factor(struct wl_client *client, struct wl_resource *object, wl_fixed_t factor_fixed) {
	struct rv_input_device *device = wl_resource_get_user_data(object);
	double factor = wl_fixed_to_double(factor_fixed);
	if (factor < 0) {
		wl_resource_post_error(object, RIVER_INPUT_DEVICE_V1_ERROR_INVALID_SCROLL_FACTOR, "%s",
			"negative scroll factor");
		return;
	}
	device->config.scroll_factor = factor;
}

static void request_map_to_output(struct wl_client *client, struct wl_resource *object,
		struct wl_resource *output) {
	struct rv_input_device *device = wl_resource_get_user_data(object);
	switch (device->wlr_device->type) {
	case WLR_INPUT_DEVICE_POINTER:
	case WLR_INPUT_DEVICE_TOUCH:
	case WLR_INPUT_DEVICE_TABLET:
		break;
	case WLR_INPUT_DEVICE_KEYBOARD:
	case WLR_INPUT_DEVICE_SWITCH:
	case WLR_INPUT_DEVICE_TABLET_PAD:
	default:
		return;
	}
	if (output != NULL) {
		struct wlr_output *wlr_output = wlr_output_from_resource(output);
		if (wlr_output == NULL) {
			return;
		}
		device->config.map_to_output = wlr_output;
	} else {
		device->config.map_to_output = NULL;
	}
	switch (device->wlr_device->type) {
	case WLR_INPUT_DEVICE_TOUCH:
	case WLR_INPUT_DEVICE_TABLET:
		wlr_cursor_map_input_to_output(device->seat->cursor.wlr_cursor, device->wlr_device,
			device->config.map_to_output);
		break;
	// River implements pointer mappings without help from wlroots
	case WLR_INPUT_DEVICE_POINTER:
		break;
	case WLR_INPUT_DEVICE_KEYBOARD:
	case WLR_INPUT_DEVICE_SWITCH:
	case WLR_INPUT_DEVICE_TABLET_PAD:
	default:
		abort(); // unreachable
	}
}

static void request_map_to_rectangle(struct wl_client *client, struct wl_resource *object, int32_t x, int32_t y,
		int32_t width, int32_t height) {
	struct rv_input_device *device = wl_resource_get_user_data(object);
	if (width < 0 || height < 0) {
		wl_resource_post_error(object, RIVER_INPUT_DEVICE_V1_ERROR_INVALID_MAP_TO_RECTANGLE, "%s",
			"negative rectangle width/height");
		return;
	}
	switch (device->wlr_device->type) {
	case WLR_INPUT_DEVICE_POINTER:
	case WLR_INPUT_DEVICE_TOUCH:
	case WLR_INPUT_DEVICE_TABLET:
		break;
	case WLR_INPUT_DEVICE_KEYBOARD:
	case WLR_INPUT_DEVICE_SWITCH:
	case WLR_INPUT_DEVICE_TABLET_PAD:
	default:
		return;
	}
	device->config.map_to_rectangle = (struct wlr_box){
		.x = x,
		.y = y,
		.width = width,
		.height = height,
	};
	switch (device->wlr_device->type) {
	case WLR_INPUT_DEVICE_TOUCH:
	case WLR_INPUT_DEVICE_TABLET:
		wlr_cursor_map_input_to_region(device->seat->cursor.wlr_cursor, device->wlr_device,
			&device->config.map_to_rectangle);
		break;
	// River implements pointer mappings without help from wlroots
	case WLR_INPUT_DEVICE_POINTER:
		break;
	case WLR_INPUT_DEVICE_KEYBOARD:
	case WLR_INPUT_DEVICE_SWITCH:
	case WLR_INPUT_DEVICE_TABLET_PAD:
	default:
		abort(); // unreachable
	}
}

static const struct river_input_device_v1_interface device_impl = {
	.destroy = request_destroy,
	.assign_to_seat = request_assign_to_seat,
	.set_repeat_info = request_set_repeat_info,
	.set_scroll_factor = request_set_scroll_factor,
	.map_to_output = request_map_to_output,
	.map_to_rectangle = request_map_to_rectangle,
};
