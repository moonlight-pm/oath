// SPDX-FileCopyrightText: © 2025 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/LibinputConfig.zig for Oath, 2026-10-18.

#include "river.h"

static const struct river_libinput_config_v1_interface config_impl;
static const struct river_libinput_config_v1_interface config_inert_impl;
static void handle_server_destroy(struct wl_listener *listener, void *data);
static void bind(struct wl_client *client, void *data, uint32_t version, uint32_t id);
static void handle_destroy(struct wl_resource *object);

bool rv_libinput_config_init(struct rv_libinput_config *config) {
	struct wl_global *global = wl_global_create(server.wl_server, &river_libinput_config_v1_interface, 1,
		config, bind);
	if (global == NULL) {
		return false;
	}
	*config = (struct rv_libinput_config){
		.global = global,
		.server_destroy = { .notify = handle_server_destroy },
	};
	wl_list_init(&config->objects);
	wl_list_init(&config->devices);
	wl_display_add_destroy_listener(server.wl_server, &config->server_destroy);
	return true;
}

static void handle_server_destroy(struct wl_listener *listener, void *data) {
	struct rv_libinput_config *config = wl_container_of(listener, config, server_destroy);

	wl_global_destroy(config->global);
}

static void bind(struct wl_client *client, void *data, uint32_t version, uint32_t id) {
	struct rv_libinput_config *config = data;
	struct wl_resource *object = wl_resource_create(client, &river_libinput_config_v1_interface, version, id);
	if (object == NULL) {
		wl_client_post_no_memory(client);
		rv_err(RV_SCOPE_INPUT, "out of memory");
		return;
	}
	wl_resource_set_implementation(object, &config_impl, config, handle_destroy);
	wl_list_insert(config->objects.prev, wl_resource_get_link(object));
	{
		struct rv_libinput_device *device, *tmp;
		wl_list_for_each_safe(device, tmp, &config->devices, link) {
			rv_libinput_device_create_object(device, object);
		}
	}
}

static void request_inert_stop(struct wl_client *client, struct wl_resource *object) {
}

static void request_inert_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

static void request_inert_create_accel_config(struct wl_client *client, struct wl_resource *object,
		uint32_t id, uint32_t profile) {
}

static const struct river_libinput_config_v1_interface config_inert_impl = {
	.stop = request_inert_stop,
	.destroy = request_inert_destroy,
	.create_accel_config = request_inert_create_accel_config,
};

static void handle_destroy(struct wl_resource *object) {
	wl_list_remove(wl_resource_get_link(object));
}

static void request_stop(struct wl_client *client, struct wl_resource *object) {
	wl_list_remove(wl_resource_get_link(object));
	river_libinput_config_v1_send_finished(object);
	wl_resource_set_implementation(object, &config_inert_impl, NULL, NULL);
}

static void request_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_post_error(object, RIVER_LIBINPUT_CONFIG_V1_ERROR_INVALID_DESTROY, "%s",
		"destroy before finished event sent");
}

static void request_create_accel_config(struct wl_client *client, struct wl_resource *object,
		uint32_t id, uint32_t profile_arg) {
	enum libinput_config_accel_profile profile;
	switch (profile_arg) {
	case RIVER_LIBINPUT_DEVICE_V1_ACCEL_PROFILE_NONE:
		profile = LIBINPUT_CONFIG_ACCEL_PROFILE_NONE;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_ACCEL_PROFILE_FLAT:
		profile = LIBINPUT_CONFIG_ACCEL_PROFILE_FLAT;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_ACCEL_PROFILE_ADAPTIVE:
		profile = LIBINPUT_CONFIG_ACCEL_PROFILE_ADAPTIVE;
		break;
	case RIVER_LIBINPUT_DEVICE_V1_ACCEL_PROFILE_CUSTOM:
		profile = LIBINPUT_CONFIG_ACCEL_PROFILE_CUSTOM;
		break;
	default:
		wl_resource_post_error(object, RIVER_LIBINPUT_CONFIG_V1_ERROR_INVALID_ARG, "%s",
			"invalid accel_profile enum value");
		return;
	}
	if (!rv_libinput_accel_config_create(wl_resource_get_client(object),
			(uint32_t)wl_resource_get_version(object), id, profile)) {
		rv_err(RV_SCOPE_INPUT, "out of memory");
		wl_resource_post_no_memory(object);
		return;
	}
}

static const struct river_libinput_config_v1_interface config_impl = {
	.stop = request_stop,
	.destroy = request_destroy,
	.create_accel_config = request_create_accel_config,
};
