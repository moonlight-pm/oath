// SPDX-License-Identifier: GPL-3.0-only
// Oath: rivulet's keyboard-shortcuts-inhibit, end to end (test/inhibit.sh).
// One process, two connections to a headless rivulet:
//   wm   a minimal River window manager that binds Super+A and Super+left
//        button and counts their presses;
//   app  a layer surface over the output with exclusive keyboard focus, and a
//        virtual keyboard and pointer to type and click.
// Without an inhibitor the window manager gets both chords and the app gets
// neither A nor the click. With an active inhibitor and a pointer lock the
// app gets both and the window manager neither. With the inhibitor gone the
// chords are the window manager's again, and an inhibitor whose surface
// loses keyboard focus turns inactive.

#define _GNU_SOURCE
#include <poll.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>
#include <xkbcommon/xkbcommon.h>

#include "keyboard-shortcuts-inhibit-unstable-v1-client-protocol.h"
#include "pointer-constraints-unstable-v1-client-protocol.h"
#include "river-layer-shell-v1-client-protocol.h"
#include "river-window-management-v1-client-protocol.h"
#include "river-xkb-bindings-v1-client-protocol.h"
#include "virtual-keyboard-unstable-v1-client-protocol.h"
#include "wlr-layer-shell-unstable-v1-client-protocol.h"
#include "wlr-virtual-pointer-unstable-v1-client-protocol.h"

// linux/input-event-codes.h
#define KEY_A 30
#define KEY_LEFTMETA 125
#define BTN_LEFT 0x110

static struct {
	struct wl_display *display;
	struct river_window_manager_v1 *manager;
	struct river_xkb_bindings_v1 *xkb_bindings;
	struct river_layer_shell_v1 *layer_shell;
	struct river_seat_v1 *seat;
	bool bound, clear_focus;
	int key_presses, button_presses;
} wm;

static struct {
	struct wl_display *display;
	struct wl_compositor *compositor;
	struct wl_shm *shm;
	struct wl_seat *seat;
	struct zwlr_layer_shell_v1 *layer_shell;
	struct zwp_keyboard_shortcuts_inhibit_manager_v1 *inhibit_manager;
	struct zwp_pointer_constraints_v1 *constraints;
	struct zwp_virtual_keyboard_manager_v1 *vk_manager;
	struct zwlr_virtual_pointer_manager_v1 *vp_manager;
	struct wl_surface *surface;
	struct zwlr_layer_surface_v1 *layer_surface;
	struct wl_keyboard *keyboard;
	struct wl_pointer *pointer;
	struct zwp_virtual_keyboard_v1 *vk;
	struct zwlr_virtual_pointer_v1 *vp;
	struct zwp_keyboard_shortcuts_inhibitor_v1 *inhibitor;
	struct zwp_locked_pointer_v1 *lock;
	uint32_t width, height, logo_mask;
	bool configured, keyboard_focus, pointer_focus, inhibit_active, locked;
	int a_presses, button_presses;
} app;

static uint32_t now_ms(void) {
	struct timespec ts;
	clock_gettime(CLOCK_MONOTONIC, &ts);
	return (uint32_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
}

// Read and dispatch both connections for up to timeout_ms.
static void pump(int timeout_ms) {
	struct wl_display *displays[2] = { wm.display, app.display };
	struct pollfd fds[2];
	for (int i = 0; i < 2; i++) {
		while (wl_display_prepare_read(displays[i]) != 0) {
			wl_display_dispatch_pending(displays[i]);
		}
		wl_display_flush(displays[i]);
		fds[i] = (struct pollfd){ .fd = wl_display_get_fd(displays[i]), .events = POLLIN };
	}
	poll(fds, 2, timeout_ms);
	for (int i = 0; i < 2; i++) {
		if (fds[i].revents & POLLIN) {
			wl_display_read_events(displays[i]);
		} else {
			wl_display_cancel_read(displays[i]);
		}
		if (wl_display_dispatch_pending(displays[i]) < 0) {
			fprintf(stderr, "FAIL: connection %d lost\n", i);
			exit(1);
		}
	}
}

static void settle(void) {
	for (int i = 0; i < 30; i++) {
		pump(10);
	}
}

#define WAIT(cond, what) \
	do { \
		for (int i_ = 0; i_ < 500 && !(cond); i_++) { \
			pump(10); \
		} \
		if (!(cond)) { \
			fprintf(stderr, "FAIL: timed out waiting for %s\n", what); \
			exit(1); \
		} \
	} while (0)

static int failures = 0;

static void expect(bool ok, const char *what) {
	printf("%s  %s\n", ok ? "ok  " : "FAIL", what);
	if (!ok) {
		failures++;
	}
}

// The window manager.

static void xkb_binding_pressed(void *data, struct river_xkb_binding_v1 *b) {
	wm.key_presses++;
}
static void xkb_binding_released(void *data, struct river_xkb_binding_v1 *b) {}
static void xkb_binding_stop_repeat(void *data, struct river_xkb_binding_v1 *b) {}
static const struct river_xkb_binding_v1_listener xkb_binding_listener = {
	.pressed = xkb_binding_pressed,
	.released = xkb_binding_released,
	.stop_repeat = xkb_binding_stop_repeat,
};

static void pointer_binding_pressed(void *data, struct river_pointer_binding_v1 *b) {
	wm.button_presses++;
}
static void pointer_binding_released(void *data, struct river_pointer_binding_v1 *b) {}
static const struct river_pointer_binding_v1_listener pointer_binding_listener = {
	.pressed = pointer_binding_pressed,
	.released = pointer_binding_released,
};

static void wm_unavailable(void *data, struct river_window_manager_v1 *m) {
	fprintf(stderr, "FAIL: window manager unavailable\n");
	exit(1);
}
static void wm_finished(void *data, struct river_window_manager_v1 *m) {}
static void wm_manage_start(void *data, struct river_window_manager_v1 *m) {
	if (!wm.bound && wm.seat != NULL) {
		struct river_xkb_binding_v1 *k = river_xkb_bindings_v1_get_xkb_binding(wm.xkb_bindings, wm.seat,
			XKB_KEY_a, RIVER_SEAT_V1_MODIFIERS_MOD4);
		river_xkb_binding_v1_add_listener(k, &xkb_binding_listener, NULL);
		river_xkb_binding_v1_enable(k);
		struct river_pointer_binding_v1 *p =
			river_seat_v1_get_pointer_binding(wm.seat, BTN_LEFT, RIVER_SEAT_V1_MODIFIERS_MOD4);
		river_pointer_binding_v1_add_listener(p, &pointer_binding_listener, NULL);
		river_pointer_binding_v1_enable(p);
		wm.bound = true;
	}
	if (wm.clear_focus) {
		river_seat_v1_clear_focus(wm.seat);
		wm.clear_focus = false;
	}
	river_window_manager_v1_manage_finish(m);
}
static void wm_render_start(void *data, struct river_window_manager_v1 *m) {
	river_window_manager_v1_render_finish(m);
}
static void wm_session_locked(void *data, struct river_window_manager_v1 *m) {}
static void wm_session_unlocked(void *data, struct river_window_manager_v1 *m) {}
static void wm_window(void *data, struct river_window_manager_v1 *m, struct river_window_v1 *w) {}
static void wm_output(void *data, struct river_window_manager_v1 *m, struct river_output_v1 *o) {}
static void wm_seat(void *data, struct river_window_manager_v1 *m, struct river_seat_v1 *s) {
	if (wm.seat == NULL) {
		wm.seat = s;
		river_window_manager_v1_manage_dirty(m);
	}
}
static const struct river_window_manager_v1_listener wm_listener = {
	.unavailable = wm_unavailable,
	.finished = wm_finished,
	.manage_start = wm_manage_start,
	.render_start = wm_render_start,
	.session_locked = wm_session_locked,
	.session_unlocked = wm_session_unlocked,
	.window = wm_window,
	.output = wm_output,
	.seat = wm_seat,
};

static void wm_global(void *data, struct wl_registry *r, uint32_t name, const char *iface, uint32_t version) {
	if (strcmp(iface, river_window_manager_v1_interface.name) == 0) {
		wm.manager = wl_registry_bind(r, name, &river_window_manager_v1_interface, 1);
		river_window_manager_v1_add_listener(wm.manager, &wm_listener, NULL);
	} else if (strcmp(iface, river_xkb_bindings_v1_interface.name) == 0) {
		wm.xkb_bindings = wl_registry_bind(r, name, &river_xkb_bindings_v1_interface, 1);
	} else if (strcmp(iface, river_layer_shell_v1_interface.name) == 0) {
		wm.layer_shell = wl_registry_bind(r, name, &river_layer_shell_v1_interface, 1);
	}
}
static void global_remove(void *data, struct wl_registry *r, uint32_t name) {}
static const struct wl_registry_listener wm_registry_listener = { .global = wm_global, .global_remove = global_remove };

// The app.

static void kb_keymap(void *data, struct wl_keyboard *k, uint32_t format, int32_t fd, uint32_t size) {
	close(fd);
}
static void kb_enter(void *data, struct wl_keyboard *k, uint32_t serial, struct wl_surface *s, struct wl_array *keys) {
	app.keyboard_focus = s == app.surface;
}
static void kb_leave(void *data, struct wl_keyboard *k, uint32_t serial, struct wl_surface *s) {
	app.keyboard_focus = false;
}
static void kb_key(void *data, struct wl_keyboard *k, uint32_t serial, uint32_t time, uint32_t key, uint32_t state) {
	if (key == KEY_A && state == WL_KEYBOARD_KEY_STATE_PRESSED) {
		app.a_presses++;
	}
}
static void kb_modifiers(void *data, struct wl_keyboard *k, uint32_t serial, uint32_t depressed, uint32_t latched,
	uint32_t locked, uint32_t group) {}
static void kb_repeat_info(void *data, struct wl_keyboard *k, int32_t rate, int32_t delay) {}
static const struct wl_keyboard_listener keyboard_listener = {
	.keymap = kb_keymap,
	.enter = kb_enter,
	.leave = kb_leave,
	.key = kb_key,
	.modifiers = kb_modifiers,
	.repeat_info = kb_repeat_info,
};

static void ptr_enter(void *data, struct wl_pointer *p, uint32_t serial, struct wl_surface *s, wl_fixed_t x,
	wl_fixed_t y) {
	app.pointer_focus = s == app.surface;
}
static void ptr_leave(void *data, struct wl_pointer *p, uint32_t serial, struct wl_surface *s) {
	app.pointer_focus = false;
}
static void ptr_motion(void *data, struct wl_pointer *p, uint32_t time, wl_fixed_t x, wl_fixed_t y) {}
static void ptr_button(void *data, struct wl_pointer *p, uint32_t serial, uint32_t time, uint32_t button,
	uint32_t state) {
	if (button == BTN_LEFT && state == WL_POINTER_BUTTON_STATE_PRESSED) {
		app.button_presses++;
	}
}
static void ptr_axis(void *data, struct wl_pointer *p, uint32_t time, uint32_t axis, wl_fixed_t value) {}
static void ptr_frame(void *data, struct wl_pointer *p) {}
static void ptr_axis_source(void *data, struct wl_pointer *p, uint32_t source) {}
static void ptr_axis_stop(void *data, struct wl_pointer *p, uint32_t time, uint32_t axis) {}
static void ptr_axis_discrete(void *data, struct wl_pointer *p, uint32_t axis, int32_t discrete) {}
static const struct wl_pointer_listener pointer_listener = {
	.enter = ptr_enter,
	.leave = ptr_leave,
	.motion = ptr_motion,
	.button = ptr_button,
	.axis = ptr_axis,
	.frame = ptr_frame,
	.axis_source = ptr_axis_source,
	.axis_stop = ptr_axis_stop,
	.axis_discrete = ptr_axis_discrete,
};

static void seat_capabilities(void *data, struct wl_seat *s, uint32_t caps) {
	if ((caps & WL_SEAT_CAPABILITY_KEYBOARD) && app.keyboard == NULL) {
		app.keyboard = wl_seat_get_keyboard(s);
		wl_keyboard_add_listener(app.keyboard, &keyboard_listener, NULL);
	}
	if ((caps & WL_SEAT_CAPABILITY_POINTER) && app.pointer == NULL) {
		app.pointer = wl_seat_get_pointer(s);
		wl_pointer_add_listener(app.pointer, &pointer_listener, NULL);
	}
}
static void seat_name(void *data, struct wl_seat *s, const char *name) {}
static const struct wl_seat_listener seat_listener = { .capabilities = seat_capabilities, .name = seat_name };

static void layer_configure(void *data, struct zwlr_layer_surface_v1 *l, uint32_t serial, uint32_t w, uint32_t h) {
	zwlr_layer_surface_v1_ack_configure(l, serial);
	app.width = w;
	app.height = h;
	app.configured = true;
}
static void layer_closed(void *data, struct zwlr_layer_surface_v1 *l) {
	fprintf(stderr, "FAIL: layer surface closed\n");
	exit(1);
}
static const struct zwlr_layer_surface_v1_listener layer_listener = {
	.configure = layer_configure,
	.closed = layer_closed,
};

static void inhibitor_active(void *data, struct zwp_keyboard_shortcuts_inhibitor_v1 *i) {
	app.inhibit_active = true;
}
static void inhibitor_inactive(void *data, struct zwp_keyboard_shortcuts_inhibitor_v1 *i) {
	app.inhibit_active = false;
}
static const struct zwp_keyboard_shortcuts_inhibitor_v1_listener inhibitor_listener = {
	.active = inhibitor_active,
	.inactive = inhibitor_inactive,
};

static void lock_locked(void *data, struct zwp_locked_pointer_v1 *l) {
	app.locked = true;
}
static void lock_unlocked(void *data, struct zwp_locked_pointer_v1 *l) {
	app.locked = false;
}
static const struct zwp_locked_pointer_v1_listener lock_listener = {
	.locked = lock_locked,
	.unlocked = lock_unlocked,
};

static void app_global(void *data, struct wl_registry *r, uint32_t name, const char *iface, uint32_t version) {
	if (strcmp(iface, wl_compositor_interface.name) == 0) {
		app.compositor = wl_registry_bind(r, name, &wl_compositor_interface, 4);
	} else if (strcmp(iface, wl_shm_interface.name) == 0) {
		app.shm = wl_registry_bind(r, name, &wl_shm_interface, 1);
	} else if (strcmp(iface, wl_seat_interface.name) == 0 && app.seat == NULL) {
		app.seat = wl_registry_bind(r, name, &wl_seat_interface, 5);
		wl_seat_add_listener(app.seat, &seat_listener, NULL);
	} else if (strcmp(iface, zwlr_layer_shell_v1_interface.name) == 0) {
		app.layer_shell = wl_registry_bind(r, name, &zwlr_layer_shell_v1_interface, 1);
	} else if (strcmp(iface, zwp_keyboard_shortcuts_inhibit_manager_v1_interface.name) == 0) {
		app.inhibit_manager = wl_registry_bind(r, name, &zwp_keyboard_shortcuts_inhibit_manager_v1_interface, 1);
	} else if (strcmp(iface, zwp_pointer_constraints_v1_interface.name) == 0) {
		app.constraints = wl_registry_bind(r, name, &zwp_pointer_constraints_v1_interface, 1);
	} else if (strcmp(iface, zwp_virtual_keyboard_manager_v1_interface.name) == 0) {
		app.vk_manager = wl_registry_bind(r, name, &zwp_virtual_keyboard_manager_v1_interface, 1);
	} else if (strcmp(iface, zwlr_virtual_pointer_manager_v1_interface.name) == 0) {
		app.vp_manager = wl_registry_bind(r, name, &zwlr_virtual_pointer_manager_v1_interface, 1);
	}
}
static const struct wl_registry_listener app_registry_listener = { .global = app_global, .global_remove = global_remove };

static struct wl_buffer *make_buffer(uint32_t w, uint32_t h) {
	size_t stride = w * 4, size = stride * h;
	int fd = memfd_create("rivulet-inhibit", MFD_CLOEXEC);
	if (fd < 0 || ftruncate(fd, size) < 0) {
		perror("memfd");
		exit(1);
	}
	struct wl_shm_pool *pool = wl_shm_create_pool(app.shm, fd, size);
	struct wl_buffer *buffer = wl_shm_pool_create_buffer(pool, 0, w, h, stride, WL_SHM_FORMAT_XRGB8888);
	wl_shm_pool_destroy(pool);
	close(fd);
	return buffer;
}

static void send_keymap(void) {
	struct xkb_context *ctx = xkb_context_new(XKB_CONTEXT_NO_FLAGS);
	struct xkb_keymap *keymap = xkb_keymap_new_from_names(ctx, NULL, XKB_KEYMAP_COMPILE_NO_FLAGS);
	if (keymap == NULL) {
		fprintf(stderr, "FAIL: no xkb keymap\n");
		exit(1);
	}
	app.logo_mask = 1u << xkb_keymap_mod_get_index(keymap, XKB_MOD_NAME_LOGO);
	char *text = xkb_keymap_get_as_string(keymap, XKB_KEYMAP_FORMAT_TEXT_V1);
	size_t size = strlen(text) + 1;
	int fd = memfd_create("rivulet-inhibit-keymap", MFD_CLOEXEC);
	if (fd < 0 || write(fd, text, size) != (ssize_t)size) {
		perror("keymap");
		exit(1);
	}
	zwp_virtual_keyboard_v1_keymap(app.vk, WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1, fd, size);
	close(fd);
	free(text);
	xkb_keymap_unref(keymap);
	xkb_context_unref(ctx);
}

// A virtual keyboard's keys do not update its modifiers; the client sends them.
static void super(bool down) {
	zwp_virtual_keyboard_v1_key(app.vk, now_ms(), KEY_LEFTMETA,
		down ? WL_KEYBOARD_KEY_STATE_PRESSED : WL_KEYBOARD_KEY_STATE_RELEASED);
	zwp_virtual_keyboard_v1_modifiers(app.vk, down ? app.logo_mask : 0, 0, 0, 0);
	settle();
}

static void super_a(void) {
	super(true);
	zwp_virtual_keyboard_v1_key(app.vk, now_ms(), KEY_A, WL_KEYBOARD_KEY_STATE_PRESSED);
	settle();
	zwp_virtual_keyboard_v1_key(app.vk, now_ms(), KEY_A, WL_KEYBOARD_KEY_STATE_RELEASED);
	settle();
	super(false);
}

static void super_click(void) {
	super(true);
	zwlr_virtual_pointer_v1_button(app.vp, now_ms(), BTN_LEFT, WL_POINTER_BUTTON_STATE_PRESSED);
	zwlr_virtual_pointer_v1_frame(app.vp);
	settle();
	zwlr_virtual_pointer_v1_button(app.vp, now_ms(), BTN_LEFT, WL_POINTER_BUTTON_STATE_RELEASED);
	zwlr_virtual_pointer_v1_frame(app.vp);
	settle();
	super(false);
}

static void move_to(uint32_t x, uint32_t y) {
	zwlr_virtual_pointer_v1_motion_absolute(app.vp, now_ms(), x, y, app.width, app.height);
	zwlr_virtual_pointer_v1_frame(app.vp);
	settle();
}

int main(void) {
	wm.display = wl_display_connect(NULL);
	app.display = wl_display_connect(NULL);
	if (wm.display == NULL || app.display == NULL) {
		fprintf(stderr, "FAIL: cannot connect to the compositor\n");
		return 1;
	}
	wl_registry_add_listener(wl_display_get_registry(wm.display), &wm_registry_listener, NULL);
	wl_display_roundtrip(wm.display);
	if (wm.manager == NULL || wm.xkb_bindings == NULL || wm.layer_shell == NULL) {
		fprintf(stderr, "FAIL: no river window manager globals\n");
		return 1;
	}
	wl_registry_add_listener(wl_display_get_registry(app.display), &app_registry_listener, NULL);
	wl_display_roundtrip(app.display);
	expect(app.inhibit_manager != NULL, "zwp_keyboard_shortcuts_inhibit_manager_v1 is advertised");
	if (app.inhibit_manager == NULL || app.layer_shell == NULL || app.constraints == NULL ||
		app.vk_manager == NULL || app.vp_manager == NULL || app.seat == NULL) {
		fprintf(stderr, "FAIL: missing globals\n");
		return 1;
	}
	WAIT(wm.bound, "the window manager's bindings");
	// A headless seat has no keyboard or pointer until these exist.
	app.vk = zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(app.vk_manager, app.seat);
	send_keymap();
	app.vp = zwlr_virtual_pointer_manager_v1_create_virtual_pointer(app.vp_manager, app.seat);
	WAIT(app.keyboard != NULL && app.pointer != NULL, "the seat's keyboard and pointer");

	app.surface = wl_compositor_create_surface(app.compositor);
	app.layer_surface = zwlr_layer_shell_v1_get_layer_surface(app.layer_shell, app.surface, NULL,
		ZWLR_LAYER_SHELL_V1_LAYER_TOP, "rivulet-inhibit");
	zwlr_layer_surface_v1_add_listener(app.layer_surface, &layer_listener, NULL);
	zwlr_layer_surface_v1_set_anchor(app.layer_surface,
		ZWLR_LAYER_SURFACE_V1_ANCHOR_TOP | ZWLR_LAYER_SURFACE_V1_ANCHOR_BOTTOM |
			ZWLR_LAYER_SURFACE_V1_ANCHOR_LEFT | ZWLR_LAYER_SURFACE_V1_ANCHOR_RIGHT);
	zwlr_layer_surface_v1_set_keyboard_interactivity(app.layer_surface, 1);
	wl_surface_commit(app.surface);
	WAIT(app.configured, "the layer surface's configure");
	wl_surface_attach(app.surface, make_buffer(app.width, app.height), 0, 0);
	wl_surface_commit(app.surface);
	WAIT(app.keyboard_focus, "keyboard focus on the layer surface");

	move_to(app.width / 2, app.height / 2);
	WAIT(app.pointer_focus, "pointer focus on the layer surface");

	// Without an inhibitor: the chords are the window manager's.
	super_a();
	expect(wm.key_presses == 1 && app.a_presses == 0, "no inhibitor: Super+A runs the binding, the app gets no A");
	super_click();
	expect(wm.button_presses == 1 && app.button_presses == 0,
		"no inhibitor: Super+click runs the binding, the app gets no click");

	// With an active inhibitor and the pointer locked: the app gets them.
	move_to(app.width / 2 + 1, app.height / 2);
	WAIT(app.pointer_focus, "pointer focus again after the binding");
	app.inhibitor = zwp_keyboard_shortcuts_inhibit_manager_v1_inhibit_shortcuts(app.inhibit_manager, app.surface,
		app.seat);
	zwp_keyboard_shortcuts_inhibitor_v1_add_listener(app.inhibitor, &inhibitor_listener, NULL);
	WAIT(app.inhibit_active, "the inhibitor's active event");
	expect(app.inhibit_active, "the inhibitor is active while its surface has keyboard focus");
	app.lock = zwp_pointer_constraints_v1_lock_pointer(app.constraints, app.surface, app.pointer, NULL,
		ZWP_POINTER_CONSTRAINTS_V1_LIFETIME_PERSISTENT);
	zwp_locked_pointer_v1_add_listener(app.lock, &lock_listener, NULL);
	wl_surface_commit(app.surface); // a constraint applies on the next commit
	move_to(app.width / 2, app.height / 2);
	WAIT(app.locked, "the pointer lock");
	super_a();
	expect(wm.key_presses == 1 && app.a_presses == 1, "inhibited: Super+A reaches the app, not the binding");
	super_click();
	expect(wm.button_presses == 1 && app.button_presses == 1,
		"inhibited and locked: Super+click reaches the app, not the binding");

	// The inhibitor gone: the chords are the window manager's again.
	zwp_keyboard_shortcuts_inhibitor_v1_destroy(app.inhibitor);
	settle();
	super_a();
	expect(wm.key_presses == 2 && app.a_presses == 1, "inhibitor destroyed: Super+A runs the binding again");
	super_click();
	expect(wm.button_presses == 2 && app.button_presses == 1,
		"inhibitor destroyed: Super+click runs the binding again");

	// Focus leaves the surface: its inhibitor turns inactive. Ending exclusive
	// focus leaves focus where it is; moving it is the window manager's call.
	app.inhibitor = zwp_keyboard_shortcuts_inhibit_manager_v1_inhibit_shortcuts(app.inhibit_manager, app.surface,
		app.seat);
	zwp_keyboard_shortcuts_inhibitor_v1_add_listener(app.inhibitor, &inhibitor_listener, NULL);
	WAIT(app.inhibit_active, "the second inhibitor's active event");
	zwlr_layer_surface_v1_set_keyboard_interactivity(app.layer_surface, 0);
	wl_surface_commit(app.surface);
	settle();
	wm.clear_focus = true;
	river_window_manager_v1_manage_dirty(wm.manager);
	WAIT(!app.keyboard_focus, "keyboard focus to leave the surface");
	WAIT(!app.inhibit_active, "the inhibitor's inactive event");
	expect(!app.inhibit_active, "the inhibitor turns inactive when its surface loses keyboard focus");

	printf("%s\n", failures == 0 ? "inhibit: ok" : "inhibit: FAILED");
	return failures == 0 ? 0 : 1;
}
