// SPDX-FileCopyrightText: © 2025 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/LibinputDevice.zig for Oath, 2026-10-18.

#include "river.h"

static const struct river_libinput_device_v1_interface device_impl;
static const struct river_libinput_device_v1_interface device_inert_impl;
static void handle_destroy(struct wl_resource *object);

void rv_libinput_device_init(struct rv_libinput_device *device, struct libinput_device *handle) {
	*device = (struct rv_libinput_device){
		.libinput = handle,
	};
	wl_list_init(&device->objects);
	wl_list_insert(server.libinput_config.devices.prev, &device->link);
	{
		struct wl_resource *config_v1, *tmp;
		wl_resource_for_each_safe(config_v1, tmp, &server.libinput_config.objects) {
			rv_libinput_device_create_object(device, config_v1);
		}
	}
}

void rv_libinput_device_create_object(struct rv_libinput_device *device, struct wl_resource *config_v1) {
	struct wl_resource *object = wl_resource_create(wl_resource_get_client(config_v1),
		&river_libinput_device_v1_interface, wl_resource_get_version(config_v1), 0);
	if (object == NULL) {
		rv_err(RV_SCOPE_INPUT, "out of memory");
		wl_resource_post_no_memory(config_v1);
		return;
	}
	wl_list_insert(device->objects.prev, wl_resource_get_link(object));
	wl_resource_set_implementation(object, &device_impl, device, handle_destroy);
	river_libinput_config_v1_send_libinput_device(config_v1, object);
	{
		struct rv_input_device *base = wl_container_of(device, base, libinput);
		assert(!base->virtual);
		struct wl_resource *input_device_v1, *tmp;
		wl_resource_for_each_safe(input_device_v1, tmp, &base->objects) {
			if (wl_resource_get_client(object) == wl_resource_get_client(input_device_v1)) {
				river_libinput_device_v1_send_input_device(object, input_device_v1);
			}
		}
	}
	river_libinput_device_v1_send_send_events_support(object,
		libinput_device_config_send_events_get_modes(device->libinput));
	river_libinput_device_v1_send_send_events_default(object,
		libinput_device_config_send_events_get_default_mode(device->libinput));
	river_libinput_device_v1_send_send_events_current(object,
		libinput_device_config_send_events_get_mode(device->libinput));
	const int tap_finger_count = libinput_device_config_tap_get_finger_count(device->libinput);
	river_libinput_device_v1_send_tap_support(object, tap_finger_count);
	if (tap_finger_count > 0) {
		river_libinput_device_v1_send_tap_default(object,
			libinput_device_config_tap_get_default_enabled(device->libinput));
		river_libinput_device_v1_send_tap_current(object,
			libinput_device_config_tap_get_enabled(device->libinput));
		river_libinput_device_v1_send_tap_button_map_default(object,
			libinput_device_config_tap_get_default_button_map(device->libinput));
		river_libinput_device_v1_send_tap_button_map_current(object,
			libinput_device_config_tap_get_button_map(device->libinput));
		river_libinput_device_v1_send_drag_default(object,
			libinput_device_config_tap_get_default_drag_enabled(device->libinput));
		river_libinput_device_v1_send_drag_current(object,
			libinput_device_config_tap_get_drag_enabled(device->libinput));
		river_libinput_device_v1_send_drag_lock_default(object,
			libinput_device_config_tap_get_default_drag_lock_enabled(device->libinput));
		river_libinput_device_v1_send_drag_lock_current(object,
			libinput_device_config_tap_get_drag_lock_enabled(device->libinput));
	}
	const int three_finger_drag_finger_count = libinput_device_config_3fg_drag_get_finger_count(device->libinput);
	river_libinput_device_v1_send_three_finger_drag_support(object, three_finger_drag_finger_count);
	if (three_finger_drag_finger_count >= 3) {
		river_libinput_device_v1_send_three_finger_drag_default(object,
			libinput_device_config_3fg_drag_get_default_enabled(device->libinput));
		river_libinput_device_v1_send_three_finger_drag_current(object,
			libinput_device_config_3fg_drag_get_enabled(device->libinput));
	}
	const int has_matrix = libinput_device_config_calibration_has_matrix(device->libinput);
	river_libinput_device_v1_send_calibration_matrix_support(object, has_matrix);
	if (has_matrix != 0) {
		float matrix[6];
		struct wl_array array = { .size = sizeof(matrix), .alloc = sizeof(matrix), .data = matrix };
		(void)libinput_device_config_calibration_get_default_matrix(device->libinput, matrix);
		river_libinput_device_v1_send_calibration_matrix_default(object, &array);
		(void)libinput_device_config_calibration_get_matrix(device->libinput, matrix);
		river_libinput_device_v1_send_calibration_matrix_current(object, &array);
	}
	const uint32_t profiles = libinput_device_config_accel_get_profiles(device->libinput);
	river_libinput_device_v1_send_accel_profiles_support(object, profiles);
	if (profiles != 0) {
		river_libinput_device_v1_send_accel_profile_default(object,
			libinput_device_config_accel_get_default_profile(device->libinput));
		river_libinput_device_v1_send_accel_profile_current(object,
			libinput_device_config_accel_get_profile(device->libinput));
		double speed[1] = { libinput_device_config_accel_get_default_speed(device->libinput) };
		struct wl_array array = { .size = sizeof(speed), .alloc = sizeof(speed), .data = speed };
		river_libinput_device_v1_send_accel_speed_default(object, &array);
		speed[0] = libinput_device_config_accel_get_speed(device->libinput);
		river_libinput_device_v1_send_accel_speed_current(object, &array);
	}
	const int natural_scroll = libinput_device_config_scroll_has_natural_scroll(device->libinput);
	river_libinput_device_v1_send_natural_scroll_support(object, natural_scroll);
	if (natural_scroll != 0) {
		const int default_ = libinput_device_config_scroll_get_default_natural_scroll_enabled(device->libinput);
		river_libinput_device_v1_send_natural_scroll_default(object, default_ != 0 ?
			RIVER_LIBINPUT_DEVICE_V1_NATURAL_SCROLL_STATE_ENABLED :
			RIVER_LIBINPUT_DEVICE_V1_NATURAL_SCROLL_STATE_DISABLED);
		const int current = libinput_device_config_scroll_get_natural_scroll_enabled(device->libinput);
		river_libinput_device_v1_send_natural_scroll_current(object, current != 0 ?
			RIVER_LIBINPUT_DEVICE_V1_NATURAL_SCROLL_STATE_ENABLED :
			RIVER_LIBINPUT_DEVICE_V1_NATURAL_SCROLL_STATE_DISABLED);
	}
	const int left_handed = libinput_device_config_left_handed_is_available(device->libinput);
	river_libinput_device_v1_send_left_handed_support(object, left_handed);
	if (left_handed != 0) {
		const int default_ = libinput_device_config_left_handed_get_default(device->libinput);
		river_libinput_device_v1_send_left_handed_default(object, default_ != 0 ?
			RIVER_LIBINPUT_DEVICE_V1_LEFT_HANDED_STATE_ENABLED :
			RIVER_LIBINPUT_DEVICE_V1_LEFT_HANDED_STATE_DISABLED);
		const int current = libinput_device_config_left_handed_get(device->libinput);
		river_libinput_device_v1_send_left_handed_current(object, current != 0 ?
			RIVER_LIBINPUT_DEVICE_V1_LEFT_HANDED_STATE_ENABLED :
			RIVER_LIBINPUT_DEVICE_V1_LEFT_HANDED_STATE_DISABLED);
	}
	const uint32_t click_methods = libinput_device_config_click_get_methods(device->libinput);
	river_libinput_device_v1_send_click_method_support(object, click_methods);
	if (click_methods != 0) {
		river_libinput_device_v1_send_click_method_default(object,
			libinput_device_config_click_get_default_method(device->libinput));
		river_libinput_device_v1_send_click_method_current(object,
			libinput_device_config_click_get_method(device->libinput));
		if ((click_methods & LIBINPUT_CONFIG_CLICK_METHOD_CLICKFINGER) != 0) {
			river_libinput_device_v1_send_clickfinger_button_map_default(object,
				libinput_device_config_click_get_default_clickfinger_button_map(device->libinput));
			river_libinput_device_v1_send_clickfinger_button_map_current(object,
				libinput_device_config_click_get_clickfinger_button_map(device->libinput));
		}
	}
	const int middle_emulation = libinput_device_config_middle_emulation_is_available(device->libinput);
	river_libinput_device_v1_send_middle_emulation_support(object, middle_emulation);
	if (middle_emulation != 0) {
		river_libinput_device_v1_send_middle_emulation_default(object,
			libinput_device_config_middle_emulation_get_default_enabled(device->libinput));
		river_libinput_device_v1_send_middle_emulation_current(object,
			libinput_device_config_middle_emulation_get_enabled(device->libinput));
	}
	const uint32_t scroll_methods = libinput_device_config_scroll_get_methods(device->libinput);
	river_libinput_device_v1_send_scroll_method_support(object, scroll_methods);
	if (scroll_methods != 0) {
		river_libinput_device_v1_send_scroll_method_default(object,
			libinput_device_config_scroll_get_default_method(device->libinput));
		river_libinput_device_v1_send_scroll_method_current(object,
			libinput_device_config_scroll_get_method(device->libinput));
		if ((scroll_methods & LIBINPUT_CONFIG_SCROLL_ON_BUTTON_DOWN) != 0) {
			river_libinput_device_v1_send_scroll_button_default(object,
				libinput_device_config_scroll_get_default_button(device->libinput));
			river_libinput_device_v1_send_scroll_button_current(object,
				libinput_device_config_scroll_get_button(device->libinput));
			river_libinput_device_v1_send_scroll_button_lock_default(object,
				libinput_device_config_scroll_get_default_button_lock(device->libinput));
			river_libinput_device_v1_send_scroll_button_lock_current(object,
				libinput_device_config_scroll_get_button_lock(device->libinput));
		}
	}
	const int dwt = libinput_device_config_dwt_is_available(device->libinput);
	river_libinput_device_v1_send_dwt_support(object, dwt);
	if (dwt != 0) {
		const enum libinput_config_dwt_state default_ = libinput_device_config_dwt_get_default_enabled(device->libinput);
		river_libinput_device_v1_send_dwt_default(object, default_ != 0 ?
			RIVER_LIBINPUT_DEVICE_V1_DWT_STATE_ENABLED :
			RIVER_LIBINPUT_DEVICE_V1_DWT_STATE_DISABLED);
		const enum libinput_config_dwt_state current = libinput_device_config_dwt_get_enabled(device->libinput);
		river_libinput_device_v1_send_dwt_current(object, current != 0 ?
			RIVER_LIBINPUT_DEVICE_V1_DWT_STATE_ENABLED :
			RIVER_LIBINPUT_DEVICE_V1_DWT_STATE_DISABLED);
	}
	const int dwtp = libinput_device_config_dwtp_is_available(device->libinput);
	river_libinput_device_v1_send_dwtp_support(object, dwtp);
	if (dwtp != 0) {
		const enum libinput_config_dwtp_state default_ =
			libinput_device_config_dwtp_get_default_enabled(device->libinput);
		river_libinput_device_v1_send_dwtp_default(object, default_ != 0 ?
			RIVER_LIBINPUT_DEVICE_V1_DWTP_STATE_ENABLED :
			RIVER_LIBINPUT_DEVICE_V1_DWTP_STATE_DISABLED);
		const enum libinput_config_dwtp_state current = libinput_device_config_dwtp_get_enabled(device->libinput);
		river_libinput_device_v1_send_dwtp_current(object, current != 0 ?
			RIVER_LIBINPUT_DEVICE_V1_DWTP_STATE_ENABLED :
			RIVER_LIBINPUT_DEVICE_V1_DWTP_STATE_DISABLED);
	}
	const int rotation = libinput_device_config_rotation_is_available(device->libinput);
	river_libinput_device_v1_send_rotation_support(object, rotation);
	if (rotation != 0) {
		river_libinput_device_v1_send_rotation_default(object,
			libinput_device_config_rotation_get_default_angle(device->libinput));
		river_libinput_device_v1_send_rotation_current(object,
			libinput_device_config_rotation_get_angle(device->libinput));
	}
}

void rv_libinput_device_deinit(struct rv_libinput_device *device) {
	{
		struct wl_resource *object, *tmp;
		wl_resource_for_each_safe(object, tmp, &device->objects) {
			wl_list_remove(wl_resource_get_link(object));
			river_libinput_device_v1_send_removed(object);
			wl_resource_set_implementation(object, &device_inert_impl, NULL, NULL);
		}
	}
	assert(wl_list_empty(&device->objects));
	wl_list_remove(&device->link);
}

static void request_inert_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

static void request_inert_set_uint(struct wl_client *client, struct wl_resource *object, uint32_t result,
		uint32_t value) {
}

static void request_inert_set_array(struct wl_client *client, struct wl_resource *object, uint32_t result,
		struct wl_array *value) {
}

static void request_inert_apply_accel_config(struct wl_client *client, struct wl_resource *object,
		uint32_t result, struct wl_resource *config) {
}

static const struct river_libinput_device_v1_interface device_inert_impl = {
	.destroy = request_inert_destroy,
	.set_send_events = request_inert_set_uint,
	.set_tap = request_inert_set_uint,
	.set_tap_button_map = request_inert_set_uint,
	.set_drag = request_inert_set_uint,
	.set_drag_lock = request_inert_set_uint,
	.set_three_finger_drag = request_inert_set_uint,
	.set_calibration_matrix = request_inert_set_array,
	.set_accel_profile = request_inert_set_uint,
	.set_accel_speed = request_inert_set_array,
	.apply_accel_config = request_inert_apply_accel_config,
	.set_natural_scroll = request_inert_set_uint,
	.set_left_handed = request_inert_set_uint,
	.set_click_method = request_inert_set_uint,
	.set_clickfinger_button_map = request_inert_set_uint,
	.set_middle_emulation = request_inert_set_uint,
	.set_scroll_method = request_inert_set_uint,
	.set_scroll_button = request_inert_set_uint,
	.set_scroll_button_lock = request_inert_set_uint,
	.set_dwt = request_inert_set_uint,
	.set_dwtp = request_inert_set_uint,
	.set_rotation = request_inert_set_uint,
};

static void handle_destroy(struct wl_resource *object) {
	wl_list_remove(wl_resource_get_link(object));
}

// LibinputDevice.Result (file-private in Zig)
struct result {
	struct wl_resource *object; // river_libinput_result_v1
};

// Result.create(): Zig ?Result. Returns false (null) after posting no_memory.
static bool result_create(struct result *result, struct wl_resource *object, uint32_t id) {
	struct wl_resource *result_v1 = wl_resource_create(wl_resource_get_client(object),
		&river_libinput_result_v1_interface, wl_resource_get_version(object), id);
	if (result_v1 == NULL) {
		rv_err(RV_SCOPE_INPUT, "out of memory");
		wl_resource_post_no_memory(object);
		return false;
	}
	*result = (struct result){ .object = result_v1 };
	return true;
}

// Result.send(): the result events are destructors (zig-wayland destroySend*).
static bool result_send(const struct result *result, uint32_t status) {
	switch (status) {
	case LIBINPUT_CONFIG_STATUS_SUCCESS:
		river_libinput_result_v1_send_success(result->object);
		break;
	case LIBINPUT_CONFIG_STATUS_UNSUPPORTED:
		river_libinput_result_v1_send_unsupported(result->object);
		break;
	default:
		river_libinput_result_v1_send_invalid(result->object);
		break;
	}
	wl_resource_destroy(result->object);
	return status == LIBINPUT_CONFIG_STATUS_SUCCESS;
}

static void invalid_arg(struct wl_resource *object, const char *msg) {
	wl_resource_post_error(object, RIVER_LIBINPUT_DEVICE_V1_ERROR_INVALID_ARG, "%s", msg);
}

static void request_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

static void request_set_send_events(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t mode) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	const enum libinput_config_status status = libinput_device_config_send_events_set_mode(device->libinput, mode);
	if (result_send(&result, status)) {
		const uint32_t current = libinput_device_config_send_events_get_mode(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_send_events_current(o, current);
		}
	}
}

static void request_set_tap(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t state) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_tap_state arg;
	switch (state) {
	case RIVER_LIBINPUT_DEVICE_V1_TAP_STATE_DISABLED:
		arg = LIBINPUT_CONFIG_TAP_DISABLED;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_TAP_STATE_ENABLED:
		arg = LIBINPUT_CONFIG_TAP_ENABLED;
		break;
	default:
		invalid_arg(object, "invalid tap_state enum value");
		return;
	}
	const enum libinput_config_status status = libinput_device_config_tap_set_enabled(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_tap_state current = libinput_device_config_tap_get_enabled(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_tap_current(o, current);
		}
	}
}

static void request_set_tap_button_map(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t button_map) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_tap_button_map arg;
	switch (button_map) {
	case RIVER_LIBINPUT_DEVICE_V1_TAP_BUTTON_MAP_LRM:
		arg = LIBINPUT_CONFIG_TAP_MAP_LRM;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_TAP_BUTTON_MAP_LMR:
		arg = LIBINPUT_CONFIG_TAP_MAP_LMR;
		break;
	default:
		invalid_arg(object, "invalid tap_button_map enum value");
		return;
	}
	const enum libinput_config_status status = libinput_device_config_tap_set_button_map(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_tap_button_map current =
			libinput_device_config_tap_get_button_map(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_tap_button_map_current(o, current);
		}
	}
}

static void request_set_drag(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t state) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_drag_state arg;
	switch (state) {
	case RIVER_LIBINPUT_DEVICE_V1_DRAG_STATE_DISABLED:
		arg = LIBINPUT_CONFIG_DRAG_DISABLED;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_DRAG_STATE_ENABLED:
		arg = LIBINPUT_CONFIG_DRAG_ENABLED;
		break;
	default:
		invalid_arg(object, "invalid drag_state enum value");
		return;
	}
	const enum libinput_config_status status = libinput_device_config_tap_set_drag_enabled(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_drag_state current = libinput_device_config_tap_get_drag_enabled(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_drag_current(o, current);
		}
	}
}

static void request_set_drag_lock(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t state) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_drag_lock_state arg;
	switch (state) {
	case RIVER_LIBINPUT_DEVICE_V1_DRAG_LOCK_STATE_DISABLED:
		arg = LIBINPUT_CONFIG_DRAG_LOCK_DISABLED;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_DRAG_LOCK_STATE_ENABLED_TIMEOUT:
		arg = LIBINPUT_CONFIG_DRAG_LOCK_ENABLED_TIMEOUT;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_DRAG_LOCK_STATE_ENABLED_STICKY:
		arg = LIBINPUT_CONFIG_DRAG_LOCK_ENABLED_STICKY;
		break;
	default:
		invalid_arg(object, "invalid drag_lock_state enum value");
		return;
	}
	const enum libinput_config_status status =
		libinput_device_config_tap_set_drag_lock_enabled(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_drag_lock_state current =
			libinput_device_config_tap_get_drag_lock_enabled(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_drag_lock_current(o, current);
		}
	}
}

static void request_set_three_finger_drag(struct wl_client *client, struct wl_resource *object,
		uint32_t result_id, uint32_t state) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_3fg_drag_state arg;
	switch (state) {
	case RIVER_LIBINPUT_DEVICE_V1_THREE_FINGER_DRAG_STATE_DISABLED:
		arg = LIBINPUT_CONFIG_3FG_DRAG_DISABLED;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_THREE_FINGER_DRAG_STATE_ENABLED_3FG:
		arg = LIBINPUT_CONFIG_3FG_DRAG_ENABLED_3FG;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_THREE_FINGER_DRAG_STATE_ENABLED_4FG:
		arg = LIBINPUT_CONFIG_3FG_DRAG_ENABLED_4FG;
		break;
	default:
		invalid_arg(object, "invalid three_finger_drag_state enum value");
		return;
	}
	const enum libinput_config_status status = libinput_device_config_3fg_drag_set_enabled(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_3fg_drag_state current =
			libinput_device_config_3fg_drag_get_enabled(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_three_finger_drag_current(o, current);
		}
	}
}

static void request_set_calibration_matrix(struct wl_client *client, struct wl_resource *object,
		uint32_t result_id, struct wl_array *matrix_arg) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	if (matrix_arg->size != sizeof(float[6])) {
		invalid_arg(object, "invalid calibration matrix");
		return;
	}
	// The wire protocol/libwayland only guarantee 32-bit word alignment, enough for f32.
	const float *matrix = matrix_arg->data;
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	const enum libinput_config_status status =
		libinput_device_config_calibration_set_matrix(device->libinput, matrix);
	if (result_send(&result, status)) {
		float current[6];
		struct wl_array array = { .size = sizeof(current), .alloc = sizeof(current), .data = current };
		(void)libinput_device_config_calibration_get_matrix(device->libinput, current);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_calibration_matrix_current(o, &array);
		}
	}
}

static void request_set_accel_profile(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t profile) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_accel_profile arg;
	switch (profile) {
	case RIVER_LIBINPUT_DEVICE_V1_ACCEL_PROFILE_NONE:
		arg = LIBINPUT_CONFIG_ACCEL_PROFILE_NONE;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_ACCEL_PROFILE_FLAT:
		arg = LIBINPUT_CONFIG_ACCEL_PROFILE_FLAT;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_ACCEL_PROFILE_ADAPTIVE:
		arg = LIBINPUT_CONFIG_ACCEL_PROFILE_ADAPTIVE;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_ACCEL_PROFILE_CUSTOM:
		arg = LIBINPUT_CONFIG_ACCEL_PROFILE_CUSTOM;
		break;
	default:
		invalid_arg(object, "invalid accel_profile enum value");
		return;
	}
	const enum libinput_config_status status = libinput_device_config_accel_set_profile(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_accel_profile current =
			libinput_device_config_accel_get_profile(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_accel_profile_current(o, current);
		}
	}
}

static void request_set_accel_speed(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		struct wl_array *speed_arg) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	if (speed_arg->size != sizeof(double)) {
		invalid_arg(object, "invalid accel speed");
		return;
	}
	// The wire protocol/libwayland only guarantee 32-bit word alignment.
	double speed;
	memcpy(&speed, speed_arg->data, sizeof(speed));
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	const enum libinput_config_status status = libinput_device_config_accel_set_speed(device->libinput, speed);
	if (result_send(&result, status)) {
		double current[1] = { libinput_device_config_accel_get_speed(device->libinput) };
		struct wl_array array = { .size = sizeof(current), .alloc = sizeof(current), .data = current };
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_accel_speed_current(o, &array);
		}
	}
}

static void request_apply_accel_config(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		struct wl_resource *config_arg) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	struct rv_libinput_accel_config *accel_config = wl_resource_get_user_data(config_arg);
	struct libinput_config_accel *config = accel_config->libinput;
	if (config == NULL) {
		(void)result_send(&result, LIBINPUT_CONFIG_STATUS_INVALID);
		return;
	}
	const enum libinput_config_status status = libinput_device_config_accel_apply(device->libinput, config);
	if (result_send(&result, status)) {
		const enum libinput_config_accel_profile current =
			libinput_device_config_accel_get_profile(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_accel_profile_current(o, current);
		}
	}
}

static void request_set_natural_scroll(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t state) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	int arg;
	switch (state) {
	case RIVER_LIBINPUT_DEVICE_V1_NATURAL_SCROLL_STATE_DISABLED:
		arg = 0;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_NATURAL_SCROLL_STATE_ENABLED:
		arg = 1;
		break;
	default:
		invalid_arg(object, "invalid natural_scroll_state enum value");
		return;
	}
	const enum libinput_config_status status =
		libinput_device_config_scroll_set_natural_scroll_enabled(device->libinput, arg);
	if (result_send(&result, status)) {
		const int current = libinput_device_config_scroll_get_natural_scroll_enabled(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_natural_scroll_current(o, current != 0 ?
				RIVER_LIBINPUT_DEVICE_V1_NATURAL_SCROLL_STATE_ENABLED :
				RIVER_LIBINPUT_DEVICE_V1_NATURAL_SCROLL_STATE_DISABLED);
		}
	}
}

static void request_set_left_handed(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t state) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	int arg;
	switch (state) {
	case RIVER_LIBINPUT_DEVICE_V1_LEFT_HANDED_STATE_DISABLED:
		arg = 0;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_LEFT_HANDED_STATE_ENABLED:
		arg = 1;
		break;
	default:
		invalid_arg(object, "invalid left_handed_state enum value");
		return;
	}
	const enum libinput_config_status status = libinput_device_config_left_handed_set(device->libinput, arg);
	if (result_send(&result, status)) {
		const int current = libinput_device_config_left_handed_get(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_left_handed_current(o, current != 0 ?
				RIVER_LIBINPUT_DEVICE_V1_LEFT_HANDED_STATE_ENABLED :
				RIVER_LIBINPUT_DEVICE_V1_LEFT_HANDED_STATE_DISABLED);
		}
	}
}

static void request_set_click_method(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t method) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_click_method arg;
	switch (method) {
	case RIVER_LIBINPUT_DEVICE_V1_CLICK_METHOD_NONE:
		arg = LIBINPUT_CONFIG_CLICK_METHOD_NONE;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_CLICK_METHOD_BUTTON_AREAS:
		arg = LIBINPUT_CONFIG_CLICK_METHOD_BUTTON_AREAS;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_CLICK_METHOD_CLICKFINGER:
		arg = LIBINPUT_CONFIG_CLICK_METHOD_CLICKFINGER;
		break;
	default:
		invalid_arg(object, "invalid click_method enum value");
		return;
	}
	const enum libinput_config_status status = libinput_device_config_click_set_method(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_click_method current = libinput_device_config_click_get_method(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_click_method_current(o, current);
		}
	}
}

static void request_set_clickfinger_button_map(struct wl_client *client, struct wl_resource *object,
		uint32_t result_id, uint32_t button_map) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_clickfinger_button_map arg;
	switch (button_map) {
	case RIVER_LIBINPUT_DEVICE_V1_CLICKFINGER_BUTTON_MAP_LRM:
		arg = LIBINPUT_CONFIG_CLICKFINGER_MAP_LRM;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_CLICKFINGER_BUTTON_MAP_LMR:
		arg = LIBINPUT_CONFIG_CLICKFINGER_MAP_LMR;
		break;
	default:
		invalid_arg(object, "invalid clickfinger_button_map enum value");
		return;
	}
	const enum libinput_config_status status =
		libinput_device_config_click_set_clickfinger_button_map(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_clickfinger_button_map current =
			libinput_device_config_click_get_clickfinger_button_map(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_clickfinger_button_map_current(o, current);
		}
	}
}

static void request_set_middle_emulation(struct wl_client *client, struct wl_resource *object,
		uint32_t result_id, uint32_t state) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_middle_emulation_state arg;
	switch (state) {
	case RIVER_LIBINPUT_DEVICE_V1_MIDDLE_EMULATION_STATE_DISABLED:
		arg = LIBINPUT_CONFIG_MIDDLE_EMULATION_DISABLED;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_MIDDLE_EMULATION_STATE_ENABLED:
		arg = LIBINPUT_CONFIG_MIDDLE_EMULATION_ENABLED;
		break;
	default:
		invalid_arg(object, "invalid middle_emulation_state enum value");
		return;
	}
	const enum libinput_config_status status =
		libinput_device_config_middle_emulation_set_enabled(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_middle_emulation_state current =
			libinput_device_config_middle_emulation_get_enabled(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_middle_emulation_current(o, current);
		}
	}
}

static void request_set_scroll_method(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t method) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_scroll_method arg;
	switch (method) {
	case RIVER_LIBINPUT_DEVICE_V1_SCROLL_METHOD_NO_SCROLL:
		arg = LIBINPUT_CONFIG_SCROLL_NO_SCROLL;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_SCROLL_METHOD_TWO_FINGER:
		arg = LIBINPUT_CONFIG_SCROLL_2FG;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_SCROLL_METHOD_EDGE:
		arg = LIBINPUT_CONFIG_SCROLL_EDGE;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_SCROLL_METHOD_ON_BUTTON_DOWN:
		arg = LIBINPUT_CONFIG_SCROLL_ON_BUTTON_DOWN;
		break;
	default:
		invalid_arg(object, "invalid scroll_method enum value");
		return;
	}
	const enum libinput_config_status status = libinput_device_config_scroll_set_method(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_scroll_method current = libinput_device_config_scroll_get_method(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_scroll_method_current(o, current);
		}
	}
}

static void request_set_scroll_button(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t button) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	const enum libinput_config_status status = libinput_device_config_scroll_set_button(device->libinput, button);
	if (result_send(&result, status)) {
		const uint32_t current = libinput_device_config_scroll_get_button(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_scroll_button_current(o, current);
		}
	}
}

static void request_set_scroll_button_lock(struct wl_client *client, struct wl_resource *object,
		uint32_t result_id, uint32_t state) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_scroll_button_lock_state arg;
	switch (state) {
	case RIVER_LIBINPUT_DEVICE_V1_SCROLL_BUTTON_LOCK_STATE_DISABLED:
		arg = LIBINPUT_CONFIG_SCROLL_BUTTON_LOCK_DISABLED;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_SCROLL_BUTTON_LOCK_STATE_ENABLED:
		arg = LIBINPUT_CONFIG_SCROLL_BUTTON_LOCK_ENABLED;
		break;
	default:
		invalid_arg(object, "invalid scroll_button_lock_state enum value");
		return;
	}
	const enum libinput_config_status status = libinput_device_config_scroll_set_button_lock(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_scroll_button_lock_state current =
			libinput_device_config_scroll_get_button_lock(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_scroll_button_lock_current(o, current);
		}
	}
}

static void request_set_dwt(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t state) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_dwt_state arg;
	switch (state) {
	case RIVER_LIBINPUT_DEVICE_V1_DWT_STATE_DISABLED:
		arg = LIBINPUT_CONFIG_DWT_DISABLED;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_DWT_STATE_ENABLED:
		arg = LIBINPUT_CONFIG_DWT_ENABLED;
		break;
	default:
		invalid_arg(object, "invalid dwt_state enum value");
		return;
	}
	const enum libinput_config_status status = libinput_device_config_dwt_set_enabled(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_dwt_state current = libinput_device_config_dwt_get_enabled(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_dwt_current(o, current != 0 ?
				RIVER_LIBINPUT_DEVICE_V1_DWT_STATE_ENABLED :
				RIVER_LIBINPUT_DEVICE_V1_DWT_STATE_DISABLED);
		}
	}
}

static void request_set_dwtp(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t state) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	enum libinput_config_dwtp_state arg;
	switch (state) {
	case RIVER_LIBINPUT_DEVICE_V1_DWTP_STATE_DISABLED:
		arg = LIBINPUT_CONFIG_DWTP_DISABLED;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_DWTP_STATE_ENABLED:
		arg = LIBINPUT_CONFIG_DWTP_ENABLED;
		break;
	default:
		invalid_arg(object, "invalid dwtp_state enum value");
		return;
	}
	const enum libinput_config_status status = libinput_device_config_dwtp_set_enabled(device->libinput, arg);
	if (result_send(&result, status)) {
		const enum libinput_config_dwtp_state current = libinput_device_config_dwtp_get_enabled(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_dwtp_current(o, current != 0 ?
				RIVER_LIBINPUT_DEVICE_V1_DWTP_STATE_ENABLED :
				RIVER_LIBINPUT_DEVICE_V1_DWTP_STATE_DISABLED);
		}
	}
}

static void request_set_rotation(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t angle) {
	struct rv_libinput_device *device = wl_resource_get_user_data(object);
	struct result result;
	if (!result_create(&result, object, result_id)) {
		return;
	}
	const enum libinput_config_status status = libinput_device_config_rotation_set_angle(device->libinput, angle);
	if (result_send(&result, status)) {
		const unsigned int current = libinput_device_config_rotation_get_angle(device->libinput);
		struct wl_resource *o, *tmp;
		wl_resource_for_each_safe(o, tmp, &device->objects) {
			river_libinput_device_v1_send_rotation_current(o, current);
		}
	}
}

static const struct river_libinput_device_v1_interface device_impl = {
	.destroy = request_destroy,
	.set_send_events = request_set_send_events,
	.set_tap = request_set_tap,
	.set_tap_button_map = request_set_tap_button_map,
	.set_drag = request_set_drag,
	.set_drag_lock = request_set_drag_lock,
	.set_three_finger_drag = request_set_three_finger_drag,
	.set_calibration_matrix = request_set_calibration_matrix,
	.set_accel_profile = request_set_accel_profile,
	.set_accel_speed = request_set_accel_speed,
	.apply_accel_config = request_apply_accel_config,
	.set_natural_scroll = request_set_natural_scroll,
	.set_left_handed = request_set_left_handed,
	.set_click_method = request_set_click_method,
	.set_clickfinger_button_map = request_set_clickfinger_button_map,
	.set_middle_emulation = request_set_middle_emulation,
	.set_scroll_method = request_set_scroll_method,
	.set_scroll_button = request_set_scroll_button,
	.set_scroll_button_lock = request_set_scroll_button_lock,
	.set_dwt = request_set_dwt,
	.set_dwtp = request_set_dwtp,
	.set_rotation = request_set_rotation,
};
