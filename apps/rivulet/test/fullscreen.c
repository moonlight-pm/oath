// SPDX-License-Identifier: GPL-3.0-only
// Oath: a window above a fullscreen window in render order (test/fullscreen.sh).
// One process, two connections to a headless rivulet:
//   wm   a minimal River window manager: it makes "game" fullscreen, puts
//        "drop" at the output's corner, and warps the pointer onto it;
//   app  the two xdg toplevels.
// With drop placed after game it stays above it, as a shell surface does, and
// the pointer is over drop; placed before game, the pointer is over game.

#define _GNU_SOURCE
#include <poll.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <wayland-client.h>

#include "river-window-management-v1-client-protocol.h"
#include "xdg-shell-client-protocol.h"

#define DROP_WIDTH 200
#define DROP_HEIGHT 100

struct window {
	struct river_window_v1 *window;
	struct river_node_v1 *node;
	char app_id[32];
	bool managed;
};

static struct {
	struct wl_display *display;
	struct river_window_manager_v1 *manager;
	struct river_output_v1 *output;
	struct river_seat_v1 *seat;
	struct window windows[4];
	int window_count;
	int32_t x, y, width, height;
	bool drop_on_top, warp;
	struct river_window_v1 *hovered;
} wm;

struct toplevel {
	const char *app_id;
	struct wl_surface *surface;
	struct xdg_surface *xdg_surface;
	struct xdg_toplevel *xdg_toplevel;
	int32_t width, height;
	bool configured;
};

static struct {
	struct wl_display *display;
	struct wl_compositor *compositor;
	struct wl_shm *shm;
	struct xdg_wm_base *wm_base;
	struct toplevel game, drop;
} app = {
	.game = { .app_id = "game" },
	.drop = { .app_id = "drop" },
};

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

static struct window *find(const char *app_id) {
	for (int i = 0; i < wm.window_count; i++) {
		if (strcmp(wm.windows[i].app_id, app_id) == 0) {
			return &wm.windows[i];
		}
	}
	return NULL;
}

static void win_closed(void *data, struct river_window_v1 *w) {}
static void win_dimensions_hint(void *data, struct river_window_v1 *w, int32_t min_width, int32_t min_height,
	int32_t max_width, int32_t max_height) {}
static void win_dimensions(void *data, struct river_window_v1 *w, int32_t width, int32_t height) {}
static void win_app_id(void *data, struct river_window_v1 *w, const char *app_id) {
	struct window *window = data;
	snprintf(window->app_id, sizeof(window->app_id), "%s", app_id != NULL ? app_id : "");
}
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

static void output_removed(void *data, struct river_output_v1 *o) {}
static void output_wl_output(void *data, struct river_output_v1 *o, uint32_t name) {}
static void output_position(void *data, struct river_output_v1 *o, int32_t x, int32_t y) {
	wm.x = x;
	wm.y = y;
}
static void output_dimensions(void *data, struct river_output_v1 *o, int32_t width, int32_t height) {
	wm.width = width;
	wm.height = height;
}
static const struct river_output_v1_listener output_listener = {
	.removed = output_removed,
	.wl_output = output_wl_output,
	.position = output_position,
	.dimensions = output_dimensions,
};

static void seat_removed(void *data, struct river_seat_v1 *s) {}
static void seat_wl_seat(void *data, struct river_seat_v1 *s, uint32_t name) {}
static void seat_pointer_enter(void *data, struct river_seat_v1 *s, struct river_window_v1 *window) {
	wm.hovered = window;
}
static void seat_pointer_leave(void *data, struct river_seat_v1 *s) {
	wm.hovered = NULL;
}
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
	for (int i = 0; i < wm.window_count; i++) {
		struct window *window = &wm.windows[i];
		if (window->managed || window->app_id[0] == '\0' || wm.output == NULL) {
			continue;
		}
		if (strcmp(window->app_id, "game") == 0) {
			river_window_v1_fullscreen(window->window, wm.output);
		} else {
			river_window_v1_propose_dimensions(window->window, DROP_WIDTH, DROP_HEIGHT);
		}
		window->managed = true;
	}
	if (wm.warp && wm.seat != NULL) {
		river_seat_v1_pointer_warp(wm.seat, wm.x + DROP_WIDTH / 2, wm.y + DROP_HEIGHT / 2);
		wm.warp = false;
	}
	river_window_manager_v1_manage_finish(m);
}
static void wm_render_start(void *data, struct river_window_manager_v1 *m) {
	struct window *game = find("game"), *drop = find("drop");
	if (drop != NULL) {
		river_node_v1_set_position(drop->node, wm.x, wm.y);
	}
	struct window *order[2] = { game, drop };
	if (!wm.drop_on_top) {
		order[0] = drop;
		order[1] = game;
	}
	for (int i = 0; i < 2; i++) {
		if (order[i] != NULL) {
			river_node_v1_place_top(order[i]->node);
		}
	}
	river_window_manager_v1_render_finish(m);
}
static void wm_session_locked(void *data, struct river_window_manager_v1 *m) {}
static void wm_session_unlocked(void *data, struct river_window_manager_v1 *m) {}
static void wm_window(void *data, struct river_window_manager_v1 *m, struct river_window_v1 *w) {
	if (wm.window_count == 4) {
		fprintf(stderr, "FAIL: too many windows\n");
		exit(1);
	}
	struct window *window = &wm.windows[wm.window_count++];
	*window = (struct window){ .window = w, .node = river_window_v1_get_node(w) };
	river_window_v1_add_listener(w, &window_listener, window);
}
static void wm_output(void *data, struct river_window_manager_v1 *m, struct river_output_v1 *o) {
	if (wm.output == NULL) {
		wm.output = o;
		river_output_v1_add_listener(o, &output_listener, NULL);
	}
}
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

// The app.

static struct wl_buffer *make_buffer(int32_t w, int32_t h) {
	size_t stride = w * 4, size = stride * h;
	int fd = memfd_create("rivulet-fullscreen", MFD_CLOEXEC);
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

static void wm_base_ping(void *data, struct xdg_wm_base *b, uint32_t serial) {
	xdg_wm_base_pong(b, serial);
}
static const struct xdg_wm_base_listener wm_base_listener = { .ping = wm_base_ping };

// Each configure gets a buffer of its size, so the window takes the size it is given.
static void xdg_surface_configure(void *data, struct xdg_surface *s, uint32_t serial) {
	struct toplevel *t = data;
	xdg_surface_ack_configure(s, serial);
	if (t->width > 0 && t->height > 0) {
		wl_surface_attach(t->surface, make_buffer(t->width, t->height), 0, 0);
		t->configured = true;
	}
	wl_surface_commit(t->surface);
}
static const struct xdg_surface_listener xdg_surface_listener = { .configure = xdg_surface_configure };

static void toplevel_configure(void *data, struct xdg_toplevel *xt, int32_t width, int32_t height,
	struct wl_array *states) {
	struct toplevel *t = data;
	if (width > 0 && height > 0) {
		t->width = width;
		t->height = height;
	}
}
static void toplevel_close(void *data, struct xdg_toplevel *xt) {}
static const struct xdg_toplevel_listener toplevel_listener = {
	.configure = toplevel_configure,
	.close = toplevel_close,
};

static void app_global(void *data, struct wl_registry *r, uint32_t name, const char *iface, uint32_t version) {
	if (strcmp(iface, wl_compositor_interface.name) == 0) {
		app.compositor = wl_registry_bind(r, name, &wl_compositor_interface, 4);
	} else if (strcmp(iface, wl_shm_interface.name) == 0) {
		app.shm = wl_registry_bind(r, name, &wl_shm_interface, 1);
	} else if (strcmp(iface, xdg_wm_base_interface.name) == 0) {
		app.wm_base = wl_registry_bind(r, name, &xdg_wm_base_interface, 1);
		xdg_wm_base_add_listener(app.wm_base, &wm_base_listener, NULL);
	}
}
static const struct wl_registry_listener app_registry_listener = { .global = app_global, .global_remove = global_remove };

static void map(struct toplevel *t) {
	t->surface = wl_compositor_create_surface(app.compositor);
	t->xdg_surface = xdg_wm_base_get_xdg_surface(app.wm_base, t->surface);
	xdg_surface_add_listener(t->xdg_surface, &xdg_surface_listener, t);
	t->xdg_toplevel = xdg_surface_get_toplevel(t->xdg_surface);
	xdg_toplevel_add_listener(t->xdg_toplevel, &toplevel_listener, t);
	xdg_toplevel_set_app_id(t->xdg_toplevel, t->app_id);
	wl_surface_commit(t->surface);
}

static struct river_window_v1 *window_of(const char *app_id) {
	struct window *window = find(app_id);
	return window != NULL ? window->window : NULL;
}

// Order the two windows, warp the pointer onto drop, and wait for the window
// manager to hear that the pointer is over the window it should be.
static void restack(bool drop_on_top, const char *over, const char *what) {
	wm.drop_on_top = drop_on_top;
	wm.warp = true;
	river_window_manager_v1_manage_dirty(wm.manager);
	struct river_window_v1 *want = window_of(over);
	for (int i = 0; i < 500 && wm.hovered != want; i++) {
		pump(10);
	}
	expect(wm.hovered == want, what);
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
	if (wm.manager == NULL) {
		fprintf(stderr, "FAIL: no river window manager global\n");
		return 1;
	}
	wl_registry_add_listener(wl_display_get_registry(app.display), &app_registry_listener, NULL);
	wl_display_roundtrip(app.display);
	if (app.compositor == NULL || app.shm == NULL || app.wm_base == NULL) {
		fprintf(stderr, "FAIL: missing globals\n");
		return 1;
	}
	WAIT(wm.output != NULL && wm.width > 0 && wm.seat != NULL, "the output and the seat");

	map(&app.game);
	WAIT(app.game.configured, "game's configure");
	map(&app.drop);
	WAIT(app.drop.configured, "drop's configure");
	expect(app.game.width == wm.width && app.game.height == wm.height, "game fills the output");

	restack(true, "drop", "drop after the fullscreen game: the pointer is over drop");
	restack(false, "game", "drop before the fullscreen game: the pointer is over game");
	restack(true, "drop", "drop after the game again: the pointer is over drop");

	printf("%s\n", failures == 0 ? "fullscreen: ok" : "fullscreen: FAILED");
	return failures == 0 ? 0 : 1;
}
