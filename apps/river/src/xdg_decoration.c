// SPDX-FileCopyrightText: © 2023 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/XdgDecoration.zig for Oath, 2026-10-18.

#include "river.h"

static void handle_destroy(struct wl_listener *listener, void *data);
static void handle_request_mode(struct wl_listener *listener, void *data);

void rv_xdg_decoration_init(struct wlr_xdg_toplevel_decoration_v1 *wlr_decoration) {
	struct rv_xdg_toplevel *toplevel = wlr_decoration->toplevel->base->data;

	toplevel->has_decoration = true;
	toplevel->decoration = (struct rv_xdg_decoration){ .wlr_decoration = wlr_decoration };
	struct rv_xdg_decoration *decoration = &toplevel->decoration;
	decoration->destroy.notify = handle_destroy;
	decoration->request_mode.notify = handle_request_mode;

	wl_signal_add(&wlr_decoration->events.destroy, &decoration->destroy);
	wl_signal_add(&wlr_decoration->events.request_mode, &decoration->request_mode);

	if (toplevel->wlr_toplevel->base->initialized) {
		handle_request_mode(&decoration->request_mode, wlr_decoration);
	}
}

void rv_xdg_decoration_deinit(struct rv_xdg_decoration *decoration) {
	struct rv_xdg_toplevel *toplevel = decoration->wlr_decoration->toplevel->base->data;

	wl_list_remove(&decoration->destroy.link);
	wl_list_remove(&decoration->request_mode.link);

	assert(toplevel->has_decoration);
	toplevel->has_decoration = false;
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_xdg_decoration *decoration = wl_container_of(listener, decoration, destroy);

	rv_xdg_decoration_deinit(decoration);
}

static void handle_request_mode(struct wl_listener *listener, void *data) {
	struct rv_xdg_decoration *decoration = wl_container_of(listener, decoration, request_mode);

	struct rv_xdg_toplevel *toplevel = decoration->wlr_decoration->toplevel->base->data;
	struct rv_window *window = toplevel->window;

	enum river_window_v1_decoration_hint hint;
	switch (decoration->wlr_decoration->requested_mode) {
	case WLR_XDG_TOPLEVEL_DECORATION_V1_MODE_NONE:
		hint = RIVER_WINDOW_V1_DECORATION_HINT_NO_PREFERENCE;
		break;
	case WLR_XDG_TOPLEVEL_DECORATION_V1_MODE_CLIENT_SIDE:
		hint = RIVER_WINDOW_V1_DECORATION_HINT_PREFERS_CSD;
		break;
	case WLR_XDG_TOPLEVEL_DECORATION_V1_MODE_SERVER_SIDE:
		hint = RIVER_WINDOW_V1_DECORATION_HINT_PREFERS_SSD;
		break;
	default:
		abort(); // unreachable (exhaustive enum in Zig)
	}
	rv_window_set_decoration_hint(window, hint);
}
