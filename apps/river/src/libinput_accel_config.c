// SPDX-FileCopyrightText: © 2025 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/LibinputAccelConfig.zig for Oath, 2026-10-18.

#include "river.h"

static const struct river_libinput_accel_config_v1_interface accel_config_impl;
static void handle_destroy(struct wl_resource *object);

bool rv_libinput_accel_config_create(struct wl_client *client, uint32_t version, uint32_t id,
		enum libinput_config_accel_profile profile) {
	struct rv_libinput_accel_config *accel_config = calloc(1, sizeof(*accel_config));
	if (accel_config == NULL) {
		return false;
	}
	struct wl_resource *object = wl_resource_create(client, &river_libinput_accel_config_v1_interface,
		(int)version, id);
	if (object == NULL) {
		free(accel_config);
		return false;
	}
	*accel_config = (struct rv_libinput_accel_config){
		.object = object,
		.libinput = libinput_config_accel_create(profile),
	};
	wl_resource_set_implementation(object, &accel_config_impl, accel_config, handle_destroy);
	return true;
}

static void handle_destroy(struct wl_resource *object) {
	struct rv_libinput_accel_config *accel_config = wl_resource_get_user_data(object);
	if (accel_config->libinput != NULL) {
		libinput_config_accel_destroy(accel_config->libinput);
	}
	free(accel_config);
}

static struct rv_libinput_accel_config *get_accel_config(struct wl_resource *object) {
	struct rv_libinput_accel_config *accel_config = wl_resource_get_user_data(object);
	assert(accel_config->object == object);
	return accel_config;
}

static void request_destroy(struct wl_client *client, struct wl_resource *object) {
	(void)get_accel_config(object);
	wl_resource_destroy(object);
}

static void request_set_points(struct wl_client *client, struct wl_resource *object, uint32_t result_id,
		uint32_t type, struct wl_array *step_arg, struct wl_array *points_arg) {
	struct rv_libinput_accel_config *accel_config = get_accel_config(object);
	enum libinput_config_accel_type accel_type;
	switch (type) {
	case RIVER_LIBINPUT_ACCEL_CONFIG_V1_ACCEL_TYPE_FALLBACK:
		accel_type = LIBINPUT_ACCEL_TYPE_FALLBACK;
		break;
	case RIVER_LIBINPUT_ACCEL_CONFIG_V1_ACCEL_TYPE_MOTION:
		accel_type = LIBINPUT_ACCEL_TYPE_MOTION;
		break;
	case RIVER_LIBINPUT_ACCEL_CONFIG_V1_ACCEL_TYPE_SCROLL:
		accel_type = LIBINPUT_ACCEL_TYPE_SCROLL;
		break;
	default:
		wl_resource_post_error(object, RIVER_LIBINPUT_ACCEL_CONFIG_V1_ERROR_INVALID_ARG, "%s",
			"invalid accel_type enum value");
		return;
	}
	if (step_arg->size != sizeof(double)) {
		wl_resource_post_error(object, RIVER_LIBINPUT_ACCEL_CONFIG_V1_ERROR_INVALID_ARG, "%s",
			"invalid step argument");
		return;
	}
	// The wire protocol/libwayland only guarantee 32-bit word alignment.
	double step;
	memcpy(&step, step_arg->data, sizeof(step));
	if (points_arg->size % sizeof(double) != 0) {
		wl_resource_post_error(object, RIVER_LIBINPUT_ACCEL_CONFIG_V1_ERROR_INVALID_ARG, "%s",
			"invalid points argument");
		return;
	}
	// util.gpa.alloc(f64, 0) returns an empty slice without allocating.
	size_t points_len = points_arg->size / sizeof(double);
	double *points = NULL;
	if (points_len > 0) {
		points = malloc(points_len * sizeof(double));
		if (points == NULL) {
			rv_err(RV_SCOPE_INPUT, "out of memory");
			wl_resource_post_no_memory(object);
			return;
		}
		memcpy(points, points_arg->data, points_arg->size);
	}
	struct wl_resource *result = wl_resource_create(wl_resource_get_client(object),
		&river_libinput_result_v1_interface, wl_resource_get_version(object), result_id);
	if (result == NULL) {
		rv_err(RV_SCOPE_INPUT, "out of memory");
		wl_resource_post_no_memory(object);
		goto out;
	}
	struct libinput_config_accel *libinput = accel_config->libinput;
	if (libinput == NULL) {
		river_libinput_result_v1_send_invalid(result);
		wl_resource_destroy(result);
		goto out;
	}
	switch (libinput_config_accel_set_points(libinput, accel_type, step, points_len, points)) {
	case LIBINPUT_CONFIG_STATUS_SUCCESS:
		river_libinput_result_v1_send_success(result);
		break;
	case LIBINPUT_CONFIG_STATUS_UNSUPPORTED:
		river_libinput_result_v1_send_unsupported(result);
		break;
	default:
		river_libinput_result_v1_send_invalid(result);
		break;
	}
	wl_resource_destroy(result);

out:
	free(points);
}

static const struct river_libinput_accel_config_v1_interface accel_config_impl = {
	.destroy = request_destroy,
	.set_points = request_set_points,
};
