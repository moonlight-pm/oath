// SPDX-FileCopyrightText: © 2025 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/LibinputDevice.zig, LibinputConfig.zig,
// LibinputAccelConfig.zig, XkbConfig.zig, XkbKeyboard.zig and XkbKeymap.zig
// for Oath, 2026-10-18.
//
// Owned by the config area. Included only from river.h; needs nothing by
// value from other areas, so river.h includes it first after util.h.

#ifndef RV_CONFIG_H
#define RV_CONFIG_H

// ---------------------------------------------------------------------------
// LibinputDevice.zig
// Embedded by value in struct rv_input_device (field `libinput`); recover the
// device with wl_container_of(libinput_device, device, libinput).
// ---------------------------------------------------------------------------

struct rv_libinput_device {
	struct libinput_device *libinput;
	struct wl_list objects; // river_libinput_device_v1 resources, linked with wl_resource_get_link()

	// LibinputConfig.devices
	struct wl_list link;
};

void rv_libinput_device_init(struct rv_libinput_device *device, struct libinput_device *handle);
// config_v1 is a river_libinput_config_v1 resource.
void rv_libinput_device_create_object(struct rv_libinput_device *device, struct wl_resource *config_v1);
void rv_libinput_device_deinit(struct rv_libinput_device *device);

// ---------------------------------------------------------------------------
// LibinputConfig.zig (embedded by value in struct rv_server as libinput_config)
// ---------------------------------------------------------------------------

struct rv_libinput_config {
	struct wl_global *global;
	struct wl_list objects; // river_libinput_config_v1 resources, linked with wl_resource_get_link()
	struct wl_list devices; // struct rv_libinput_device.link

	struct wl_listener server_destroy; // default: .init(handleServerDestroy)
};

// Zig !void: returns false if the global could not be created.
bool rv_libinput_config_init(struct rv_libinput_config *config);

// ---------------------------------------------------------------------------
// LibinputAccelConfig.zig
// ---------------------------------------------------------------------------

struct rv_libinput_accel_config {
	struct wl_resource *object; // river_libinput_accel_config_v1
	struct libinput_config_accel *libinput; // nullable
};

// Zig !void: returns false on OOM or resource creation failure.
bool rv_libinput_accel_config_create(struct wl_client *client, uint32_t version, uint32_t id,
	enum libinput_config_accel_profile profile);

// ---------------------------------------------------------------------------
// XkbConfig.zig (embedded by value in struct rv_server as xkb_config)
// ---------------------------------------------------------------------------

struct rv_xkb_config {
	struct wl_global *global;
	struct wl_list objects; // river_xkb_config_v1 resources, linked with wl_resource_get_link()
	struct wl_list keymaps; // struct rv_xkb_keymap.link
	struct wl_list keyboards; // struct rv_xkb_keyboard.link

	struct xkb_context *context;
	struct xkb_keymap *default_keymap;

	struct wl_listener server_destroy; // default: .init(handleServerDestroy)
};

// Zig !void (error.XkbContextFailed, error.XkbKeymapFailed, global creation
// failure): returns false on any of them.
bool rv_xkb_config_init(struct rv_xkb_config *config);

// ---------------------------------------------------------------------------
// XkbKeyboard.zig
// Embedded by value in struct rv_input_device (field `xkb_keyboard`); recover
// the device with wl_container_of(xkb_keyboard, device, xkb_keyboard).
// ---------------------------------------------------------------------------

// XkbKeyboard.sent (anonymous struct in Zig); all-zero is the Zig default (all null).
struct rv_xkb_keyboard_sent {
	bool has_layout_index;
	uint32_t layout_index;
	// This string is owned by our global xkb.Context.
	const char *layout_name; // nullable
	bool has_capslock;
	bool capslock;
	bool has_numlock;
	bool numlock;
};

struct rv_xkb_keyboard {
	struct wl_list objects; // river_xkb_keyboard_v1 resources, linked with wl_resource_get_link()

	struct rv_xkb_keyboard_sent sent;

	// XkbConfig.keyboards
	struct wl_list link;
};

void rv_xkb_keyboard_init(struct rv_xkb_keyboard *xkb_keyboard);
// config_v1 is a river_xkb_config_v1 resource.
void rv_xkb_keyboard_create_object(struct rv_xkb_keyboard *xkb_keyboard, struct wl_resource *config_v1);
void rv_xkb_keyboard_deinit(struct rv_xkb_keyboard *xkb_keyboard);
// layout_name may be NULL.
void rv_xkb_keyboard_send_state(struct rv_xkb_keyboard *xkb_keyboard, xkb_layout_index_t layout_index,
	const char *layout_name, bool capslock, bool numlock);

// ---------------------------------------------------------------------------
// XkbKeymap.zig
// ---------------------------------------------------------------------------

struct rv_xkb_keymap {
	struct wl_resource *object; // river_xkb_keymap_v1
	struct xkb_keymap *xkb_keymap;

	// XkbConfig.keymaps
	struct wl_list link;
};

// Zig !void (OutOfMemory, ResourceCreateFailed): returns false on either.
bool rv_xkb_keymap_create(struct wl_client *client, uint32_t version, uint32_t id, struct xkb_keymap *xkb_keymap);
// Zig !void (ResourceCreateFailed): returns false on failure.
bool rv_xkb_keymap_create_failed(struct wl_client *client, uint32_t version, uint32_t id, const char *error_msg);

#endif
