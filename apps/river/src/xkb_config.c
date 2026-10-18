// SPDX-FileCopyrightText: © 2026 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/XkbConfig.zig for Oath, 2026-10-18.

// strerrorname_np() names an errno the way Zig's @tagName(std.c.E) does, minus the "E".
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif

#include "river.h"

#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

static const struct river_xkb_config_v1_interface config_impl;
static const struct river_xkb_config_v1_interface config_inert_impl;
static void handle_server_destroy(struct wl_listener *listener, void *data);
static void bind(struct wl_client *client, void *data, uint32_t version, uint32_t id);
static void handle_destroy(struct wl_resource *object);
static bool create_keymap(struct wl_resource *object, uint32_t id, enum xkb_keymap_format format, int32_t fd);

bool rv_xkb_config_init(struct rv_xkb_config *config) {
	struct xkb_context *context = xkb_context_new(XKB_CONTEXT_NO_FLAGS);
	if (context == NULL) {
		// error.XkbContextFailed
		return false;
	}

	// Passing null here indicates that defaults from libxkbcommon and
	// its XKB_DEFAULT_LAYOUT, XKB_DEFAULT_OPTIONS, etc. should be used.
	struct xkb_keymap *default_keymap = xkb_keymap_new_from_names(context, NULL, XKB_KEYMAP_COMPILE_NO_FLAGS);
	if (default_keymap == NULL) {
		xkb_context_unref(context);
		// error.XkbKeymapFailed
		return false;
	}

	struct wl_global *global = wl_global_create(server.wl_server, &river_xkb_config_v1_interface, 1, config, bind);
	if (global == NULL) {
		xkb_keymap_unref(default_keymap);
		xkb_context_unref(context);
		return false;
	}

	*config = (struct rv_xkb_config){
		.global = global,
		.context = xkb_context_ref(context),
		.default_keymap = xkb_keymap_ref(default_keymap),
		.server_destroy = { .notify = handle_server_destroy },
	};
	wl_list_init(&config->objects);
	wl_list_init(&config->keymaps);
	wl_list_init(&config->keyboards);

	wl_display_add_destroy_listener(server.wl_server, &config->server_destroy);

	xkb_keymap_unref(default_keymap);
	xkb_context_unref(context);
	return true;
}

static void handle_server_destroy(struct wl_listener *listener, void *data) {
	struct rv_xkb_config *config = wl_container_of(listener, config, server_destroy);

	wl_global_destroy(config->global);
	xkb_context_unref(config->context);
	xkb_keymap_unref(config->default_keymap);
}

static void bind(struct wl_client *client, void *data, uint32_t version, uint32_t id) {
	struct rv_xkb_config *config = data;
	struct wl_resource *object = wl_resource_create(client, &river_xkb_config_v1_interface, version, id);
	if (object == NULL) {
		wl_client_post_no_memory(client);
		rv_err(RV_SCOPE_INPUT, "out of memory");
		return;
	}
	wl_resource_set_implementation(object, &config_impl, config, handle_destroy);
	wl_list_insert(config->objects.prev, wl_resource_get_link(object));
	{
		struct rv_xkb_keyboard *device, *tmp;
		wl_list_for_each_safe(device, tmp, &config->keyboards, link) {
			rv_xkb_keyboard_create_object(device, object);
		}
	}
}

static void request_inert_stop(struct wl_client *client, struct wl_resource *object) {
}

static void request_inert_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

static void request_inert_create_keymap(struct wl_client *client, struct wl_resource *object, uint32_t id,
		int32_t fd, uint32_t format) {
}

static const struct river_xkb_config_v1_interface config_inert_impl = {
	.stop = request_inert_stop,
	.destroy = request_inert_destroy,
	.create_keymap = request_inert_create_keymap,
};

static void handle_destroy(struct wl_resource *object) {
	wl_list_remove(wl_resource_get_link(object));
}

static void request_stop(struct wl_client *client, struct wl_resource *object) {
	wl_list_remove(wl_resource_get_link(object));
	river_xkb_config_v1_send_finished(object);
	wl_resource_set_implementation(object, &config_inert_impl, NULL, NULL);
}

static void request_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_post_error(object, RIVER_XKB_CONFIG_V1_ERROR_INVALID_DESTROY, "%s",
		"destroy before finished event sent");
}

static void request_create_keymap(struct wl_client *client, struct wl_resource *object, uint32_t id,
		int32_t fd, uint32_t format_arg) {
	enum xkb_keymap_format format;
	switch (format_arg) {
	case RIVER_XKB_CONFIG_V1_KEYMAP_FORMAT_TEXT_V1:
		format = XKB_KEYMAP_FORMAT_TEXT_V1;
		break;
	case RIVER_XKB_CONFIG_V1_KEYMAP_FORMAT_TEXT_V2:
		format = XKB_KEYMAP_FORMAT_TEXT_V2;
		break;
	default:
		wl_resource_post_error(object, RIVER_XKB_CONFIG_V1_ERROR_INVALID_FORMAT, "%s",
			"invalid format enum value");
		return;
	}
	if (!create_keymap(object, id, format, fd)) {
		// error.OutOfMemory, error.ResourceCreateFailed
		rv_err(RV_SCOPE_INPUT, "out of memory");
		wl_resource_post_no_memory(object);
		return;
	}
}

static const struct river_xkb_config_v1_interface config_impl = {
	.stop = request_stop,
	.destroy = request_destroy,
	.create_keymap = request_create_keymap,
};

// The Zig error std.Io.File.stat() returns for an errno on Linux (std.Io.Threaded),
// formatted with {} as "error.<Name>". Programmer-bug errnos (EBADF, EINVAL, ...)
// are error.Unexpected in a release build.
static const char *stat_error_name(int err) {
	switch (err) {
	case EACCES:
		return "error.AccessDenied";
	case ENOMEM:
		return "error.SystemResources";
	default:
		return "error.Unexpected";
	}
}

// @tagName(@as(std.c.E, @enumFromInt(errno))): the errno name without its leading "E".
static const char *errno_tag_name(int err) {
	const char *name = strerrorname_np(err);
	if (name == NULL || name[0] != 'E') {
		return "(unknown errno)";
	}
	return name + 1;
}

// The goal of this function is to handle whatever fd the client has sent us without crashing.
// The fd may be invalid, impossible to mmap, not contain a valid keymap, etc.
// This requires us to avoid the syscall wrappers in std.posix which assert on EBADF for example.
// Zig !void (OutOfMemory, ResourceCreateFailed): returns false on either.
static bool create_keymap(struct wl_resource *object, uint32_t id, enum xkb_keymap_format format, int32_t fd) {
	struct wl_client *client = wl_resource_get_client(object);
	const uint32_t version = (uint32_t)wl_resource_get_version(object);
	bool ok;

	struct stat st;
	int rc;
	do {
		rc = fstat(fd, &st);
	} while (rc == -1 && errno == EINTR);
	if (rc == -1) {
		rv_err(RV_SCOPE_INPUT, "failed to stat keymap fd: %s", stat_error_name(errno));
		ok = rv_xkb_keymap_create_failed(client, version, id, "failed to stat keymap fd");
		goto close_fd;
	}
	const uint64_t size = (uint64_t)st.st_size;

	// Must be zero terminated
	if (size < 1) {
		rv_err(RV_SCOPE_INPUT, "keymap too small");
		ok = rv_xkb_keymap_create_failed(client, version, id, "keymap too small");
		goto close_fd;
	}
	if (size > 1024 * 1024) {
		rv_err(RV_SCOPE_INPUT, "keymap too large: %llu bytes", (unsigned long long)size);
		ok = rv_xkb_keymap_create_failed(client, version, id, "keymap too large");
		goto close_fd;
	}
	const size_t keymap_len = (size_t)(size - 1);

	void *keymap_ptr = mmap(NULL, keymap_len, PROT_READ, MAP_PRIVATE, fd, 0);
	if (keymap_ptr == MAP_FAILED) {
		rv_err(RV_SCOPE_INPUT, "failed to mmap() keymap fd: %s", errno_tag_name(errno));
		ok = rv_xkb_keymap_create_failed(client, version, id, "failed to mmap() keymap fd");
		goto close_fd;
	}

	struct xkb_keymap *keymap = xkb_keymap_new_from_buffer(server.xkb_config.context, keymap_ptr, keymap_len,
		format, XKB_KEYMAP_COMPILE_NO_FLAGS);
	if (keymap == NULL) {
		rv_err(RV_SCOPE_INPUT, "failed to parse xkb keymap");
		ok = rv_xkb_keymap_create_failed(client, version, id, "failed to parse xkb keymap");
		goto munmap;
	}

	ok = rv_xkb_keymap_create(client, version, id, keymap);

	xkb_keymap_unref(keymap);
munmap:
	(void)munmap(keymap_ptr, keymap_len);
close_fd:
	(void)close(fd);
	return ok;
}
