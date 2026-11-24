// SPDX-License-Identifier: GPL-3.0-only
// Oath: a client that dies while the window manager focuses its unmapped
// window (test/unmapped-focus.sh). One process, two connections to a headless
// rivulet:
//   wm   a minimal River window manager: it sizes every new window and
//        focuses it at once, before the client has drawn anything;
//   app  an xdg toplevel that acks its configure but never attaches a
//        buffer, then disconnects.
// The seat must let the window go when its toplevel is destroyed before map;
// it did so only on unmap, and Window.destroy() aborted (Sola's Paint dying
// while it opened a picture took the desk down, 2026-11-22).

#define _GNU_SOURCE
#include <poll.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <wayland-client.h>

#include "river-window-management-v1-client-protocol.h"
#include "xdg-shell-client-protocol.h"

static struct {
	struct wl_display *display;
	struct river_window_manager_v1 *manager;
	struct river_seat_v1 *seat;
	struct river_window_v1 *window;
	bool managed, focused, closed;
} wm;

static struct {
	struct wl_display *display;
	struct wl_compositor *compositor;
	struct xdg_wm_base *wm_base;
	bool configured;
} app;

// Read and dispatch the connections that are open for up to timeout_ms.
static void pump(int timeout_ms) {
	struct wl_display *displays[2] = { wm.display, app.display };
	struct pollfd fds[2];
	int n = 0;
	for (int i = 0; i < 2; i++) {
		if (displays[i] == NULL) {
			continue;
		}
		while (wl_display_prepare_read(displays[i]) != 0) {
			wl_display_dispatch_pending(displays[i]);
		}
		wl_display_flush(displays[i]);
		fds[n++] = (struct pollfd){ .fd = wl_display_get_fd(displays[i]), .events = POLLIN };
	}
	poll(fds, n, timeout_ms);
	n = 0;
	for (int i = 0; i < 2; i++) {
		if (displays[i] == NULL) {
			continue;
		}
		if (fds[n++].revents & POLLIN) {
			wl_display_read_events(displays[i]);
		} else {
			wl_display_cancel_read(displays[i]);
		}
		if (wl_display_dispatch_pending(displays[i]) < 0) {
			fprintf(stderr, "FAIL: connection %d lost (rivulet died?)\n", i);
			exit(1);
		}
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

// The window manager.

static void win_closed(void *data, struct river_window_v1 *w) {
	wm.closed = true;
}
static void win_dimensions_hint(void *data, struct river_window_v1 *w, int32_t min_width, int32_t min_height,
	int32_t max_width, int32_t max_height) {}
static void win_dimensions(void *data, struct river_window_v1 *w, int32_t width, int32_t height) {}
static void win_app_id(void *data, struct river_window_v1 *w, const char *app_id) {}
static void win_title(void *data, struct river_window_v1 *w, const char *title) {}
static void win_parent(void *data, struct river_window_v1 *w, struct river_window_v1 *parent) {}
static void win_decoration_hint(void *data, struct river_window_v1 *w, uint32_t hint) {}
static void win_pointer_move_requested(void *data, struct river_window_v1 *w, struct river_seat_v1 *seat) {}
static void win_pointer_resize_requested(void *data, struct river_window_v1 *w, struct river_seat_v1 *seat,
	uint32_t edges) {}
static void win_show_window_menu_requested(void *data, struct river_window_v1 *w, int32_t x, int32_t y) {}
static void win_maximize_requested(void *data, struct river_window_v1 *w) {}
static void win_unmaximize_requested(void *data, struct river_window_v1 *w) {}
static void win_fullscreen_requested(void *data, struct river_window_v1 *w, struct river_output_v1 *output) {}
static void win_exit_fullscreen_requested(void *data, struct river_window_v1 *w) {}
static void win_minimize_requested(void *data, struct river_window_v1 *w) {}
static void win_unreliable_pid(void *data, struct river_window_v1 *w, int32_t pid) {}
static void win_presentation_hint(void *data, struct river_window_v1 *w, uint32_t hint) {}
static void win_identifier(void *data, struct river_window_v1 *w, const char *identifier) {}
static const struct river_window_v1_listener window_listener = {
	.closed = win_closed,
	.dimensions_hint = win_dimensions_hint,
	.dimensions = win_dimensions,
	.app_id = win_app_id,
	.title = win_title,
	.parent = win_parent,
	.decoration_hint = win_decoration_hint,
	.pointer_move_requested = win_pointer_move_requested,
	.pointer_resize_requested = win_pointer_resize_requested,
	.show_window_menu_requested = win_show_window_menu_requested,
	.maximize_requested = win_maximize_requested,
	.unmaximize_requested = win_unmaximize_requested,
	.fullscreen_requested = win_fullscreen_requested,
	.exit_fullscreen_requested = win_exit_fullscreen_requested,
	.minimize_requested = win_minimize_requested,
	.unreliable_pid = win_unreliable_pid,
	.presentation_hint = win_presentation_hint,
	.identifier = win_identifier,
};

static void seat_removed(void *data, struct river_seat_v1 *s) {}
static void seat_wl_seat(void *data, struct river_seat_v1 *s, uint32_t name) {}
static void seat_pointer_enter(void *data, struct river_seat_v1 *s, struct river_window_v1 *window) {}
static void seat_pointer_leave(void *data, struct river_seat_v1 *s) {}
static void seat_window_interaction(void *data, struct river_seat_v1 *s, struct river_window_v1 *window) {}
static void seat_shell_surface_interaction(void *data, struct river_seat_v1 *s,
	struct river_shell_surface_v1 *shell_surface) {}
static void seat_op_delta(void *data, struct river_seat_v1 *s, int32_t dx, int32_t dy) {}
static void seat_op_release(void *data, struct river_seat_v1 *s) {}
static void seat_pointer_position(void *data, struct river_seat_v1 *s, int32_t x, int32_t y) {}
static const struct river_seat_v1_listener seat_listener = {
	.removed = seat_removed,
	.wl_seat = seat_wl_seat,
	.pointer_enter = seat_pointer_enter,
	.pointer_leave = seat_pointer_leave,
	.window_interaction = seat_window_interaction,
	.shell_surface_interaction = seat_shell_surface_interaction,
	.op_delta = seat_op_delta,
	.op_release = seat_op_release,
	.pointer_position = seat_pointer_position,
};

static void wm_unavailable(void *data, struct river_window_manager_v1 *m) {
	fprintf(stderr, "FAIL: window manager unavailable\n");
	exit(1);
}
static void wm_finished(void *data, struct river_window_manager_v1 *m) {}
static void wm_manage_start(void *data, struct river_window_manager_v1 *m) {
	if (wm.window != NULL && !wm.closed && wm.seat != NULL) {
		if (!wm.managed) {
			river_window_v1_propose_dimensions(wm.window, 200, 100);
			wm.managed = true;
		}
		river_seat_v1_focus_window(wm.seat, wm.window);
		wm.focused = true;
	}
	river_window_manager_v1_manage_finish(m);
}
static void wm_render_start(void *data, struct river_window_manager_v1 *m) {
	river_window_manager_v1_render_finish(m);
}
static void wm_session_locked(void *data, struct river_window_manager_v1 *m) {}
static void wm_session_unlocked(void *data, struct river_window_manager_v1 *m) {}
static void wm_window(void *data, struct river_window_manager_v1 *m, struct river_window_v1 *w) {
	wm.window = w;
	river_window_v1_add_listener(w, &window_listener, NULL);
}
static void wm_output(void *data, struct river_window_manager_v1 *m, struct river_output_v1 *o) {}
static void wm_seat(void *data, struct river_window_manager_v1 *m, struct river_seat_v1 *s) {
	if (wm.seat == NULL) {
		wm.seat = s;
		river_seat_v1_add_listener(s, &seat_listener, NULL);
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
		wm.manager = wl_registry_bind(r, name, &river_window_manager_v1_interface, 4);
		river_window_manager_v1_add_listener(wm.manager, &wm_listener, NULL);
	}
}
static void global_remove(void *data, struct wl_registry *r, uint32_t name) {}
static const struct wl_registry_listener wm_registry_listener = { .global = wm_global, .global_remove = global_remove };

// The app: it acks and commits, but never attaches a buffer, so it never maps.

static void wm_base_ping(void *data, struct xdg_wm_base *b, uint32_t serial) {
	xdg_wm_base_pong(b, serial);
}
static const struct xdg_wm_base_listener wm_base_listener = { .ping = wm_base_ping };

static void xdg_surface_configure(void *data, struct xdg_surface *s, uint32_t serial) {
	xdg_surface_ack_configure(s, serial);
	wl_surface_commit(data);
	app.configured = true;
}
static const struct xdg_surface_listener xdg_surface_listener = { .configure = xdg_surface_configure };

static void app_global(void *data, struct wl_registry *r, uint32_t name, const char *iface, uint32_t version) {
	if (strcmp(iface, wl_compositor_interface.name) == 0) {
		app.compositor = wl_registry_bind(r, name, &wl_compositor_interface, 4);
	} else if (strcmp(iface, xdg_wm_base_interface.name) == 0) {
		app.wm_base = wl_registry_bind(r, name, &xdg_wm_base_interface, 1);
		xdg_wm_base_add_listener(app.wm_base, &wm_base_listener, NULL);
	}
}
static const struct wl_registry_listener app_registry_listener = { .global = app_global, .global_remove = global_remove };

int main(void) {
	wm.display = wl_display_connect(NULL);
	app.display = wl_display_connect(NULL);
	if (wm.display == NULL || app.display == NULL) {
		fprintf(stderr, "FAIL: cannot connect to the compositor\n");
		return 1;
	}
	wl_registry_add_listener(wl_display_get_registry(wm.display), &wm_registry_listener, NULL);
	wl_display_roundtrip(wm.display);
	if (wm.manager == NULL) {
		fprintf(stderr, "FAIL: no river window manager global\n");
		return 1;
	}
	wl_registry_add_listener(wl_display_get_registry(app.display), &app_registry_listener, NULL);
	wl_display_roundtrip(app.display);
	if (app.compositor == NULL || app.wm_base == NULL) {
		fprintf(stderr, "FAIL: missing globals\n");
		return 1;
	}
	WAIT(wm.seat != NULL, "the seat");

	struct wl_surface *surface = wl_compositor_create_surface(app.compositor);
	struct xdg_surface *xdg_surface = xdg_wm_base_get_xdg_surface(app.wm_base, surface);
	xdg_surface_add_listener(xdg_surface, &xdg_surface_listener, surface);
	struct xdg_toplevel *toplevel = xdg_surface_get_toplevel(xdg_surface);
	xdg_toplevel_set_app_id(toplevel, "unmapped");
	wl_surface_commit(surface);
	WAIT(app.configured && wm.focused, "the configure and the focus");
	// One more manage sequence, so the focus the window manager asked for is applied.
	river_window_manager_v1_manage_dirty(wm.manager);
	for (int i = 0; i < 10; i++) {
		pump(10);
	}

	// The client goes away without destroying anything, as a crash does.
	wl_display_disconnect(app.display);
	app.display = NULL;
	WAIT(wm.closed, "the window's closed event");
	// A few more sequences: Window.destroy() runs after the one that sent closed.
	for (int i = 0; i < 5; i++) {
		river_window_manager_v1_manage_dirty(wm.manager);
		pump(20);
	}
	if (wl_display_roundtrip(wm.display) < 0) {
		fprintf(stderr, "FAIL: rivulet went away after the client did\n");
		return 1;
	}
	printf("ok    the window manager's connection outlives the client\n");
	printf("unmapped-focus: ok\n");
	return 0;
}
