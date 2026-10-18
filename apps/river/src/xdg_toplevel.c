// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/XdgToplevel.zig for Oath, 2026-10-18.

#include "river.h"

static void handle_destroy(struct wl_listener *listener, void *data);
static void handle_ack_configure(struct wl_listener *listener, void *data);
static void handle_map(struct wl_listener *listener, void *data);
static void handle_unmap(struct wl_listener *listener, void *data);
static void handle_commit(struct wl_listener *listener, void *data);
static void handle_new_popup(struct wl_listener *listener, void *data);
static void handle_request_show_window_menu(struct wl_listener *listener, void *data);
static void handle_request_fullscreen(struct wl_listener *listener, void *data);
static void handle_request_maximize(struct wl_listener *listener, void *data);
static void handle_request_minimize(struct wl_listener *listener, void *data);
static void handle_request_move(struct wl_listener *listener, void *data);
static void handle_request_resize(struct wl_listener *listener, void *data);
static void handle_set_parent(struct wl_listener *listener, void *data);
static void handle_set_title(struct wl_listener *listener, void *data);
static void handle_set_app_id(struct wl_listener *listener, void *data);

// Zig @intCast to u31 (safety-checked in ReleaseSafe).
static int32_t int_cast_u31(int32_t value) {
	assert(value >= 0);
	return value;
}

bool rv_xdg_toplevel_create(struct wlr_xdg_toplevel *wlr_toplevel) {
	rv_debug(RV_SCOPE_XDG, "new xdg_toplevel");

	struct rv_window_impl impl = { .tag = RV_WINDOW_IMPL_TOPLEVEL };
	impl.toplevel = (struct rv_xdg_toplevel){
		.window = NULL, // undefined until set below
		.wlr_toplevel = wlr_toplevel,
		.has_decoration = false,
		.geometry = { .x = 0, .y = 0, .width = 0, .height = 0 },
		.configure_state = { .tag = RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE },
	};
	impl.toplevel.destroy.notify = handle_destroy;
	impl.toplevel.ack_configure.notify = handle_ack_configure;
	impl.toplevel.map.notify = handle_map;
	impl.toplevel.unmap.notify = handle_unmap;
	impl.toplevel.commit.notify = handle_commit;
	impl.toplevel.new_popup.notify = handle_new_popup;
	impl.toplevel.request_show_window_menu.notify = handle_request_show_window_menu;
	impl.toplevel.request_fullscreen.notify = handle_request_fullscreen;
	impl.toplevel.request_maximize.notify = handle_request_maximize;
	impl.toplevel.request_minimize.notify = handle_request_minimize;
	impl.toplevel.request_move.notify = handle_request_move;
	impl.toplevel.request_resize.notify = handle_request_resize;
	impl.toplevel.set_parent.notify = handle_set_parent;
	impl.toplevel.set_title.notify = handle_set_title;
	impl.toplevel.set_app_id.notify = handle_set_app_id;

	struct rv_window *window = rv_window_create(impl);
	if (window == NULL) {
		return false;
	}

	struct rv_xdg_toplevel *toplevel = &window->impl.toplevel;

	// This listener must be added before the scene xdg surface is created.
	// Otherwise, the scene surface nodes will already be disabled by the unmap
	// listeners in the scene xdg surface and scene subsurface tree helpers
	// before our unmap listener is called.
	// However, we need the surface tree to be unchanged in our unmap listener
	// so that we can save the buffers for frame perfection.
	// TODO(wlroots) This is fragile, it would be good if wlroots gave us a
	// better alternative here.
	wl_signal_add(&wlr_toplevel->base->surface->events.unmap, &toplevel->unmap);

	if (wlr_scene_xdg_surface_create(window->surfaces.tree, wlr_toplevel->base) == NULL) {
		goto err;
	}
	if (wlr_scene_xdg_surface_create(&window->capture_scene->tree, wlr_toplevel->base) == NULL) {
		goto err;
	}

	toplevel->window = window;

	wlr_toplevel->base->data = toplevel;
	wlr_toplevel->base->surface->data = &window->tree->node;

	wl_signal_add(&wlr_toplevel->events.destroy, &toplevel->destroy);
	wl_signal_add(&wlr_toplevel->base->events.ack_configure, &toplevel->ack_configure);
	wl_signal_add(&wlr_toplevel->base->surface->events.map, &toplevel->map);
	wl_signal_add(&wlr_toplevel->base->surface->events.commit, &toplevel->commit);
	wl_signal_add(&wlr_toplevel->base->events.new_popup, &toplevel->new_popup);
	wl_signal_add(&wlr_toplevel->events.request_show_window_menu, &toplevel->request_show_window_menu);
	wl_signal_add(&wlr_toplevel->events.request_fullscreen, &toplevel->request_fullscreen);
	wl_signal_add(&wlr_toplevel->events.request_maximize, &toplevel->request_maximize);
	wl_signal_add(&wlr_toplevel->events.request_minimize, &toplevel->request_minimize);
	wl_signal_add(&wlr_toplevel->events.request_move, &toplevel->request_move);
	wl_signal_add(&wlr_toplevel->events.request_resize, &toplevel->request_resize);
	wl_signal_add(&wlr_toplevel->events.set_parent, &toplevel->set_parent);
	wl_signal_add(&wlr_toplevel->events.set_title, &toplevel->set_title);
	wl_signal_add(&wlr_toplevel->events.set_app_id, &toplevel->set_app_id);

	return true;

err:
	// errdefer order: toplevel.unmap.link.remove(), then window.destroy()
	// (which, as in the Zig, asserts that impl is .destroying).
	wl_list_remove(&toplevel->unmap.link);
	rv_window_destroy(window);
	return false;
}

static bool needs_configure(struct rv_xdg_toplevel *toplevel);

// Zig `defer switch (toplevel.configure_state)` at the end of configure().
static void check_configure_state_after(const struct rv_xdg_toplevel *toplevel) {
	switch (toplevel->configure_state.tag) {
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_INFLIGHT:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_ACKED:
		break;
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT_ACKED:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_COMMITTED:
		abort(); // unreachable
	}
}

// Send a configure event, return true if the configure should be tracked
// and current surfaces saved for frame perfection.
bool rv_xdg_toplevel_configure(struct rv_xdg_toplevel *toplevel) {
	switch (toplevel->configure_state.tag) {
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT_ACKED:
		break;
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_INFLIGHT:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_ACKED:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_COMMITTED:
		abort(); // unreachable
	}

	bool result;

	struct rv_window_configure *scheduled = &toplevel->window->configure_scheduled;
	// As in the Zig source: `sent` also points at configure_scheduled, so the
	// bounds comparison below never differs and setBounds() is not called here.
	struct rv_window_configure *sent = &toplevel->window->configure_scheduled;

	if (!needs_configure(toplevel)) {
		// If no new configure is required, continue to track a timed out configure
		// from the previous transaction if any.
		switch (toplevel->configure_state.tag) {
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE:
			result = false;
			goto out;
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT: {
			const uint32_t serial = toplevel->configure_state.timed_out;
			toplevel->configure_state = (struct rv_xdg_toplevel_configure_state){
				.tag = RV_XDG_TOPLEVEL_CONFIGURE_STATE_INFLIGHT,
				.inflight = serial,
			};
			result = true;
			goto out;
		}
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT_ACKED:
			toplevel->configure_state = (struct rv_xdg_toplevel_configure_state){
				.tag = RV_XDG_TOPLEVEL_CONFIGURE_STATE_ACKED,
			};
			result = true;
			goto out;
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_INFLIGHT:
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_ACKED:
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_COMMITTED:
			abort(); // unreachable
		}
	}

	struct wlr_xdg_toplevel *wlr_toplevel = toplevel->wlr_toplevel;

	(void)wlr_xdg_toplevel_set_activated(wlr_toplevel, scheduled->activated);
	{
		uint32_t tiled_edges = WLR_EDGE_NONE;
		if (scheduled->tiled & RIVER_WINDOW_V1_EDGES_TOP) {
			tiled_edges |= WLR_EDGE_TOP;
		}
		if (scheduled->tiled & RIVER_WINDOW_V1_EDGES_BOTTOM) {
			tiled_edges |= WLR_EDGE_BOTTOM;
		}
		if (scheduled->tiled & RIVER_WINDOW_V1_EDGES_LEFT) {
			tiled_edges |= WLR_EDGE_LEFT;
		}
		if (scheduled->tiled & RIVER_WINDOW_V1_EDGES_RIGHT) {
			tiled_edges |= WLR_EDGE_RIGHT;
		}
		(void)wlr_xdg_toplevel_set_tiled(wlr_toplevel, tiled_edges);
	}
	{
		uint32_t caps = 0;
		if (scheduled->capabilities & RIVER_WINDOW_V1_CAPABILITIES_WINDOW_MENU) {
			caps |= WLR_XDG_TOPLEVEL_WM_CAPABILITIES_WINDOW_MENU;
		}
		if (scheduled->capabilities & RIVER_WINDOW_V1_CAPABILITIES_MAXIMIZE) {
			caps |= WLR_XDG_TOPLEVEL_WM_CAPABILITIES_MAXIMIZE;
		}
		if (scheduled->capabilities & RIVER_WINDOW_V1_CAPABILITIES_FULLSCREEN) {
			caps |= WLR_XDG_TOPLEVEL_WM_CAPABILITIES_FULLSCREEN;
		}
		if (scheduled->capabilities & RIVER_WINDOW_V1_CAPABILITIES_MINIMIZE) {
			caps |= WLR_XDG_TOPLEVEL_WM_CAPABILITIES_MINIMIZE;
		}
		(void)wlr_xdg_toplevel_set_wm_capabilities(wlr_toplevel, caps);
	}
	(void)wlr_xdg_toplevel_set_maximized(wlr_toplevel, scheduled->maximized);
	(void)wlr_xdg_toplevel_set_fullscreen(wlr_toplevel, scheduled->inform_fullscreen);
	(void)wlr_xdg_toplevel_set_resizing(wlr_toplevel, scheduled->resizing);
	if (toplevel->has_decoration) {
		(void)wlr_xdg_toplevel_decoration_v1_set_mode(toplevel->decoration.wlr_decoration,
			scheduled->ssd ? WLR_XDG_TOPLEVEL_DECORATION_V1_MODE_SERVER_SIDE :
				WLR_XDG_TOPLEVEL_DECORATION_V1_MODE_CLIENT_SIDE);
	}
	if (scheduled->bounds.width != sent->bounds.width ||
			scheduled->bounds.height != sent->bounds.height) {
		(void)wlr_xdg_toplevel_set_bounds(wlr_toplevel, scheduled->bounds.width, scheduled->bounds.height);
	}

	int32_t width; // u31
	if (scheduled->has_width) {
		width = scheduled->width;
	} else {
		switch (toplevel->configure_state.tag) {
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE:
			width = int_cast_u31(toplevel->geometry.width);
			break;
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT:
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT_ACKED:
			assert(toplevel->window->configure_sent.has_width); // .?
			width = toplevel->window->configure_sent.width;
			break;
		default:
			abort(); // unreachable
		}
	}
	int32_t height; // u31
	if (scheduled->has_height) {
		height = scheduled->height;
	} else {
		switch (toplevel->configure_state.tag) {
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE:
			height = int_cast_u31(toplevel->geometry.height);
			break;
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT:
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT_ACKED:
			assert(toplevel->window->configure_sent.has_height); // .?
			height = toplevel->window->configure_sent.height;
			break;
		default:
			abort(); // unreachable
		}
	}
	const uint32_t configure_serial = wlr_xdg_toplevel_set_size(wlr_toplevel, width, height);

	toplevel->window->configure_sent = toplevel->window->configure_scheduled;
	toplevel->window->configure_sent.has_width = true;
	toplevel->window->configure_sent.width = width;
	toplevel->window->configure_sent.has_height = true;
	toplevel->window->configure_sent.height = height;
	toplevel->window->configure_scheduled.has_width = false;
	toplevel->window->configure_scheduled.width = 0;
	toplevel->window->configure_scheduled.has_height = false;
	toplevel->window->configure_scheduled.height = 0;

	// Generally, only track configures (and save surfaces) if there is a
	// change in size involved. If the configure state is not idle, we are
	// currently tracking a timed out configure and should instead track the
	// new one even if there is no change in size involved.
	if (width != 0 && height != 0 &&
			width == toplevel->geometry.width && height == toplevel->geometry.height &&
			toplevel->configure_state.tag == RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE) {
		result = false;
		goto out;
	}

	toplevel->configure_state = (struct rv_xdg_toplevel_configure_state){
		.tag = RV_XDG_TOPLEVEL_CONFIGURE_STATE_INFLIGHT,
		.inflight = configure_serial,
	};

	result = true;

out:
	check_configure_state_after(toplevel);
	return result;
}

static bool needs_configure(struct rv_xdg_toplevel *toplevel) {
	const struct rv_window_configure *scheduled = &toplevel->window->configure_scheduled;
	const struct rv_window_configure *sent = &toplevel->window->configure_sent;

	// ?u31 != ?u31: true if exactly one is null or both are set and differ.
	if (scheduled->has_width && (!sent->has_width || scheduled->width != sent->width)) {
		return true;
	}
	if (scheduled->has_height && (!sent->has_height || scheduled->height != sent->height)) {
		return true;
	}
	if (scheduled->bounds.width != sent->bounds.width ||
			scheduled->bounds.height != sent->bounds.height) {
		return true;
	}
	if (scheduled->activated != sent->activated) {
		return true;
	}
	if (scheduled->ssd != sent->ssd) {
		return true;
	}
	if (scheduled->tiled != sent->tiled) {
		return true;
	}
	if (scheduled->capabilities != sent->capabilities) {
		return true;
	}
	if (scheduled->maximized != sent->maximized) {
		return true;
	}
	if (scheduled->inform_fullscreen != sent->inform_fullscreen) {
		return true;
	}
	if ((scheduled->resizing) != (sent->resizing)) {
		return true;
	}

	return false;
}

void rv_xdg_toplevel_destroy_popups(struct rv_xdg_toplevel *toplevel) {
	struct wlr_xdg_popup *wlr_xdg_popup, *tmp;
	wl_list_for_each_safe(wlr_xdg_popup, tmp, &toplevel->wlr_toplevel->base->popups, link) {
		wlr_xdg_popup_destroy(wlr_xdg_popup);
	}
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, destroy);

	// This can be be non-null here if the client commits a protocol error or
	// if it exits without destroying its wayland objects.
	if (toplevel->has_decoration) {
		rv_xdg_decoration_deinit(&toplevel->decoration);
	}
	assert(!toplevel->has_decoration);

	wl_list_remove(&toplevel->destroy.link);
	wl_list_remove(&toplevel->ack_configure.link);
	wl_list_remove(&toplevel->map.link);
	wl_list_remove(&toplevel->unmap.link);
	wl_list_remove(&toplevel->commit.link);
	wl_list_remove(&toplevel->new_popup.link);
	wl_list_remove(&toplevel->request_show_window_menu.link);
	wl_list_remove(&toplevel->request_fullscreen.link);
	wl_list_remove(&toplevel->request_maximize.link);
	wl_list_remove(&toplevel->request_minimize.link);
	wl_list_remove(&toplevel->request_move.link);
	wl_list_remove(&toplevel->request_resize.link);
	wl_list_remove(&toplevel->set_parent.link);
	wl_list_remove(&toplevel->set_title.link);
	wl_list_remove(&toplevel->set_app_id.link);

	// The wlr_surface may outlive the wlr_xdg_toplevel so we must clean up the user data.
	toplevel->wlr_toplevel->base->surface->data = NULL;

	struct rv_window *window = toplevel->window;
	// toplevel is no longer valid after this point.
	window->impl.tag = RV_WINDOW_IMPL_DESTROYING;
	switch (window->state) {
	case RV_WINDOW_STATE_INIT:
	case RV_WINDOW_STATE_CLOSING:
		break;
	// This can happen if the xdg toplevel is destroyed after the initial
	// commit but before the window is mapped.
	case RV_WINDOW_STATE_READY:
	case RV_WINDOW_STATE_INITIALIZED:
		window->state = RV_WINDOW_STATE_CLOSING;
		rv_window_manager_dirty_windowing(&server.wm);
		break;
	// State must have been set to closing in Window.unmap()
	case RV_WINDOW_STATE_MAPPED:
		abort(); // unreachable
	}
}

static void handle_map(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, map);

	if (!rv_window_map(toplevel->window)) {
		rv_err(RV_SCOPE_XDG, "out of memory");
		wl_client_post_no_memory(wl_resource_get_client(toplevel->wlr_toplevel->resource));
	}
}

// Called when the surface is unmapped and will no longer be displayed.
static void handle_unmap(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, unmap);

	rv_window_unmap(toplevel->window);
}

static void handle_new_popup(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, new_popup);
	struct wlr_xdg_popup *wlr_xdg_popup = data;

	if (!rv_xdg_popup_create(
			wlr_xdg_popup,
			toplevel->window->popup_tree,
			toplevel->window->popup_tree,
			&toplevel->window->capture_scene->tree)) {
		wl_resource_post_no_memory(wlr_xdg_popup->resource);
		return;
	}
}

static void handle_ack_configure(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, ack_configure);
	struct wlr_xdg_surface_configure *acked_configure = data;
	switch (toplevel->configure_state.tag) {
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_INFLIGHT:
		if (acked_configure->serial == toplevel->configure_state.inflight) {
			toplevel->configure_state = (struct rv_xdg_toplevel_configure_state){
				.tag = RV_XDG_TOPLEVEL_CONFIGURE_STATE_ACKED,
			};
		}
		break;
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT:
		if (acked_configure->serial == toplevel->configure_state.timed_out) {
			toplevel->configure_state = (struct rv_xdg_toplevel_configure_state){
				.tag = RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT_ACKED,
			};
		}
		break;
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_ACKED:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_COMMITTED:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT_ACKED:
		break;
	}
}

static void handle_commit(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, commit);
	struct rv_window *window = toplevel->window;

	// NB: the subsurface tree is never empty here
	wlr_scene_subsurface_tree_set_clip(&window->capture_scene->tree.node, &toplevel->wlr_toplevel->base->geometry);

	rv_window_set_dimensions_hint(window, (struct rv_window_dimensions_hint){
		.min_width = int_cast_u31(toplevel->wlr_toplevel->current.min_width),
		.min_height = int_cast_u31(toplevel->wlr_toplevel->current.min_height),
		.max_width = int_cast_u31(toplevel->wlr_toplevel->current.max_width),
		.max_height = int_cast_u31(toplevel->wlr_toplevel->current.max_height),
	});

	if (toplevel->wlr_toplevel->base->initial_commit) {
		assert(window->state != RV_WINDOW_STATE_READY);
		window->state = RV_WINDOW_STATE_READY;
		rv_window_manager_dirty_windowing(&server.wm);
		return;
	}

	if (window->state != RV_WINDOW_STATE_MAPPED) {
		return;
	}

	switch (toplevel->configure_state.tag) {
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_COMMITTED:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT: {
		const struct wlr_box old_geometry = toplevel->geometry;
		toplevel->geometry = toplevel->wlr_toplevel->base->geometry;

		const bool size_changed = toplevel->geometry.width != old_geometry.width ||
			toplevel->geometry.height != old_geometry.height;

		if (size_changed) {
			rv_debug(RV_SCOPE_XDG,
				"client initiated size change: %dx%d -> %dx%d",
				old_geometry.width, old_geometry.height, toplevel->geometry.width, toplevel->geometry.height);

			rv_window_set_dimensions(window, int_cast_u31(toplevel->geometry.width),
				int_cast_u31(toplevel->geometry.height));
		} else if (old_geometry.x != toplevel->geometry.x ||
				old_geometry.y != toplevel->geometry.y) {
			// We need to update the surface clip box to reflect the geometry change.
			rv_window_render_finish(window);
		}
		break;
	}
	// If the client has not yet acked our configure, we need to send a
	// frame done event so that it commits another buffer. These
	// buffers won't be rendered since we are still rendering our
	// stashed buffer from when the transaction started.
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_INFLIGHT:
		rv_window_send_frame_done(window);
		break;
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_ACKED:
	case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT_ACKED:
		toplevel->geometry = toplevel->wlr_toplevel->base->geometry;

		window->rendering_scheduled.width = int_cast_u31(toplevel->geometry.width);
		window->rendering_scheduled.height = int_cast_u31(toplevel->geometry.height);

		switch (toplevel->configure_state.tag) {
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_ACKED:
			toplevel->configure_state = (struct rv_xdg_toplevel_configure_state){
				.tag = RV_XDG_TOPLEVEL_CONFIGURE_STATE_COMMITTED,
			};
			rv_window_manager_notify_configured(&server.wm);
			break;
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT_ACKED:
			toplevel->configure_state = (struct rv_xdg_toplevel_configure_state){
				.tag = RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE,
			};
			rv_window_manager_dirty_rendering(&server.wm);
			break;
		default:
			abort(); // unreachable
		}
		break;
	}
}

static void handle_request_show_window_menu(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, request_show_window_menu);
	struct wlr_xdg_toplevel_show_window_menu_event *event = data;
	toplevel->window->wm_scheduled.has_show_window_menu_requested = true;
	toplevel->window->wm_scheduled.show_window_menu_requested =
		(struct rv_window_wm_scheduled_show_window_menu_requested){
			.x = event->x - toplevel->geometry.x,
			.y = event->y - toplevel->geometry.y,
		};
	rv_window_manager_dirty_windowing(&server.wm);
}

static void handle_request_fullscreen(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, request_fullscreen);
	if (toplevel->wlr_toplevel->requested.fullscreen) {
		struct wlr_output *wlr_output = toplevel->wlr_toplevel->requested.fullscreen_output;
		if (wlr_output != NULL) {
			struct rv_output *output = wlr_output->data;
			toplevel->window->wm_scheduled.fullscreen_requested = (struct rv_window_fullscreen_request){
				.tag = RV_WINDOW_FULLSCREEN_REQUEST_FULLSCREEN,
				.fullscreen = output,
			};
		} else {
			toplevel->window->wm_scheduled.fullscreen_requested = (struct rv_window_fullscreen_request){
				.tag = RV_WINDOW_FULLSCREEN_REQUEST_FULLSCREEN,
				.fullscreen = NULL,
			};
		}
	} else {
		toplevel->window->wm_scheduled.fullscreen_requested = (struct rv_window_fullscreen_request){
			.tag = RV_WINDOW_FULLSCREEN_REQUEST_EXIT,
		};
	}
	rv_window_manager_dirty_windowing(&server.wm);
}

static void handle_request_maximize(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, request_maximize);
	if (toplevel->wlr_toplevel->requested.maximized) {
		toplevel->window->wm_scheduled.maximize_requested = RV_WINDOW_WM_SCHEDULED_MAXIMIZE_REQUESTED_MAXIMIZE;
	} else {
		toplevel->window->wm_scheduled.maximize_requested = RV_WINDOW_WM_SCHEDULED_MAXIMIZE_REQUESTED_UNMAXIMIZE;
	}
	rv_window_manager_dirty_windowing(&server.wm);
}

static void handle_request_minimize(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, request_minimize);
	toplevel->window->wm_scheduled.minimize_requested = true;
	rv_window_manager_dirty_windowing(&server.wm);
}

static void handle_request_move(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, request_move);
	struct wlr_xdg_toplevel_move_event *event = data;
	struct rv_seat *seat = event->seat->seat->data;

	// Moving windows with touch or tablet tool is not yet supported.
	if (wlr_seat_validate_pointer_grab_serial(seat->wlr_seat, NULL, event->serial)) {
		toplevel->window->wm_scheduled.pointer_move_requested = seat;
		rv_window_manager_dirty_windowing(&server.wm);
	}
}

static void handle_request_resize(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, request_resize);
	struct wlr_xdg_toplevel_resize_event *event = data;
	struct rv_seat *seat = event->seat->seat->data;

	// Resizing windows with touch or tablet tool is not yet supported.
	if (wlr_seat_validate_pointer_grab_serial(seat->wlr_seat, NULL, event->serial)) {
		toplevel->window->wm_scheduled.has_pointer_resize_requested = true;
		toplevel->window->wm_scheduled.pointer_resize_requested =
			(struct rv_window_wm_scheduled_pointer_resize_requested){
				.seat = seat,
				// @bitCast(wlr.Edges) -> river.WindowV1.Edges: same bits
				.edges = event->edges,
			};
		rv_window_manager_dirty_windowing(&server.wm);
	}
}

static void handle_set_parent(struct wl_listener *listener, void *data) {
	rv_window_manager_dirty_windowing(&server.wm);
}

// Called when the client sets / updates its title
static void handle_set_title(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, set_title);
	rv_window_notify_title(toplevel->window);
}

// Called when the client sets / updates its app_id
static void handle_set_app_id(struct wl_listener *listener, void *data) {
	struct rv_xdg_toplevel *toplevel = wl_container_of(listener, toplevel, set_app_id);
	rv_window_notify_app_id(toplevel->window);
}
