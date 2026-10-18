// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/XwaylandWindow.zig for Oath, 2026-10-18.

#include "rivulet.h"

#if WLR_HAS_XWAYLAND

static void handle_destroy(struct wl_listener *listener, void *data);
static void handle_request_configure(struct wl_listener *listener, void *data);
static void handle_set_override_redirect(struct wl_listener *listener, void *data);
static void handle_associate(struct wl_listener *listener, void *data);
static void handle_dissociate(struct wl_listener *listener, void *data);
static void handle_set_size_hints(struct wl_listener *listener, void *data);
static void handle_set_title(struct wl_listener *listener, void *data);
static void handle_set_class(struct wl_listener *listener, void *data);
static void handle_set_parent(struct wl_listener *listener, void *data);
static void handle_set_decorations(struct wl_listener *listener, void *data);
static void handle_request_maximize(struct wl_listener *listener, void *data);
static void handle_request_fullscreen(struct wl_listener *listener, void *data);
static void handle_request_minimize(struct wl_listener *listener, void *data);
static void handle_unmap(struct wl_listener *listener, void *data);

// Zig {?s}: a null string prints as "null".
static const char *opt_str(const char *s) {
	return s != NULL ? s : "null";
}

// std.math.lossyCast(i16, x): saturating.
static int16_t lossy_cast_i16(int32_t value) {
	if (value > INT16_MAX) {
		return INT16_MAX;
	}
	if (value < INT16_MIN) {
		return INT16_MIN;
	}
	return (int16_t)value;
}

// std.math.lossyCast(u16, x): saturating.
static uint16_t lossy_cast_u16(int32_t value) {
	if (value > UINT16_MAX) {
		return UINT16_MAX;
	}
	if (value < 0) {
		return 0;
	}
	return (uint16_t)value;
}

bool rv_xwayland_window_create(struct wlr_xwayland_surface *xsurface) {
	rv_debug(RV_SCOPE_XWAYLAND, "new xwayland window: title='%s', class='%s'",
		opt_str(xsurface->title),
		opt_str(xsurface->class));

	struct rv_window_impl impl = { .tag = RV_WINDOW_IMPL_XWAYLAND };
	impl.xwayland = (struct rv_xwayland_window){
		.window = NULL, // undefined until set below
		.xsurface = xsurface,
		.surface_tree = NULL,
	};
	impl.xwayland.destroy.notify = handle_destroy;
	impl.xwayland.request_configure.notify = handle_request_configure;
	impl.xwayland.set_override_redirect.notify = handle_set_override_redirect;
	impl.xwayland.associate.notify = handle_associate;
	impl.xwayland.dissociate.notify = handle_dissociate;
	impl.xwayland.set_size_hints.notify = handle_set_size_hints;
	impl.xwayland.set_title.notify = handle_set_title;
	impl.xwayland.set_class.notify = handle_set_class;
	impl.xwayland.set_parent.notify = handle_set_parent;
	impl.xwayland.set_decorations.notify = handle_set_decorations;
	impl.xwayland.request_maximize.notify = handle_request_maximize;
	impl.xwayland.request_fullscreen.notify = handle_request_fullscreen;
	impl.xwayland.request_minimize.notify = handle_request_minimize;
	impl.xwayland.map.notify = rv_xwayland_window_handle_map;
	impl.xwayland.unmap.notify = handle_unmap;

	struct rv_window *window = rv_window_create(impl);
	if (window == NULL) {
		return false;
	}

	struct rv_xwayland_window *xwindow = &window->impl.xwayland;
	xwindow->window = window;

	xsurface->data = xwindow;

	// Add listeners that are active over the window's entire lifetime
	wl_signal_add(&xsurface->events.destroy, &xwindow->destroy);
	wl_signal_add(&xsurface->events.associate, &xwindow->associate);
	wl_signal_add(&xsurface->events.dissociate, &xwindow->dissociate);
	wl_signal_add(&xsurface->events.request_configure, &xwindow->request_configure);
	wl_signal_add(&xsurface->events.set_override_redirect, &xwindow->set_override_redirect);
	wl_signal_add(&xsurface->events.set_size_hints, &xwindow->set_size_hints);
	wl_signal_add(&xsurface->events.set_title, &xwindow->set_title);
	wl_signal_add(&xsurface->events.set_class, &xwindow->set_class);
	wl_signal_add(&xsurface->events.set_parent, &xwindow->set_parent);
	wl_signal_add(&xsurface->events.set_decorations, &xwindow->set_decorations);
	wl_signal_add(&xsurface->events.request_maximize, &xwindow->request_maximize);
	wl_signal_add(&xsurface->events.request_fullscreen, &xwindow->request_fullscreen);
	wl_signal_add(&xsurface->events.request_minimize, &xwindow->request_minimize);

	struct wlr_surface *surface = xsurface->surface;
	if (surface != NULL) {
		handle_associate(&xwindow->associate, NULL);
		if (surface->mapped) {
			rv_xwayland_window_handle_map(&xwindow->map, NULL);
		}
	}

	return true;
}

static void set_activated(struct rv_xwayland_window *xwindow, bool activated);

// Always returns false as we do not care about frame perfection for Xwayland windows.
bool rv_xwayland_window_configure(struct rv_xwayland_window *xwindow) {
	struct rv_window *window = xwindow->window;
	struct rv_window_configure *scheduled = &window->configure_scheduled;
	struct rv_window_configure *sent = &window->configure_sent;

	// Sending a 0 width/height to X11 clients is invalid, so fake it
	if (scheduled->has_width && scheduled->width == 0) {
		scheduled->width = xwindow->xsurface->width;
	}
	if (scheduled->has_height && scheduled->height == 0) {
		scheduled->height = xwindow->xsurface->height;
	}
	const int32_t width = scheduled->has_width ? scheduled->width : xwindow->xsurface->width;
	const int32_t height = scheduled->has_height ? scheduled->height : xwindow->xsurface->height;

	// Unlike native Wayland windows, we need to tell X11 windows about their
	// position. However, river does not necessarily know the new position
	// until after a rendering sequence is completed. Therefore, configure()
	// is called both on manageFinish() and renderFinish() for Xwayland windows.
	// Frame perfection is not achievable for Xwayland windows in any case.
	if (window->box.x != xwindow->xsurface->x ||
			window->box.y != xwindow->xsurface->y ||
			width != xwindow->xsurface->width ||
			height != xwindow->xsurface->height) {
		wlr_xwayland_surface_configure(xwindow->xsurface,
			lossy_cast_i16(window->box.x),
			lossy_cast_i16(window->box.y),
			lossy_cast_u16(width),
			lossy_cast_u16(height));
	}

	if (scheduled->activated != sent->activated) {
		set_activated(xwindow, scheduled->activated);
	}
	if (scheduled->maximized != sent->maximized) {
		wlr_xwayland_surface_set_maximized(xwindow->xsurface, scheduled->maximized, scheduled->maximized);
	}
	if (scheduled->inform_fullscreen != sent->inform_fullscreen) {
		wlr_xwayland_surface_set_fullscreen(xwindow->xsurface, scheduled->inform_fullscreen);
	}
	window->configure_sent = window->configure_scheduled;
	window->configure_sent.has_width = true;
	window->configure_sent.width = width;
	window->configure_sent.has_height = true;
	window->configure_sent.height = height;
	window->configure_scheduled.has_width = false;
	window->configure_scheduled.width = 0;
	window->configure_scheduled.has_height = false;
	window->configure_scheduled.height = 0;

	return false;
}

static void set_activated(struct rv_xwayland_window *xwindow, bool activated) {
	// See comment on handleRequestMinimize() for details
	if (activated && xwindow->xsurface->minimized) {
		wlr_xwayland_surface_set_minimized(xwindow->xsurface, false);
	}
	wlr_xwayland_surface_activate(xwindow->xsurface, activated);
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, destroy);

	// Remove listeners that are active for the entire lifetime of the window
	wl_list_remove(&xwindow->destroy.link);
	wl_list_remove(&xwindow->associate.link);
	wl_list_remove(&xwindow->dissociate.link);
	wl_list_remove(&xwindow->request_configure.link);
	wl_list_remove(&xwindow->set_override_redirect.link);
	wl_list_remove(&xwindow->set_size_hints.link);
	wl_list_remove(&xwindow->set_title.link);
	wl_list_remove(&xwindow->set_class.link);
	wl_list_remove(&xwindow->set_parent.link);
	wl_list_remove(&xwindow->set_decorations.link);
	wl_list_remove(&xwindow->request_maximize.link);
	wl_list_remove(&xwindow->request_fullscreen.link);
	wl_list_remove(&xwindow->request_minimize.link);

	xwindow->xsurface->data = NULL;

	struct rv_window *window = xwindow->window;
	// xwindow is no longer valid after this point.
	window->impl.tag = RV_WINDOW_IMPL_DESTROYING;
	// X11 does not guarantee unmap-before-destroy the way XDG does, so the
	// window may still be in .ready / .initialized / .mapped here. Mirror
	// XdgToplevel.handleDestroy() and route through .closing so the next
	// manage cycle drives Window.destroy() down its valid path instead of
	// hitting the unreachable arm.
	switch (window->state) {
	case RV_WINDOW_STATE_INIT:
	case RV_WINDOW_STATE_CLOSING:
		break;
	case RV_WINDOW_STATE_READY:
	case RV_WINDOW_STATE_INITIALIZED:
	case RV_WINDOW_STATE_MAPPED:
		window->state = RV_WINDOW_STATE_CLOSING;
		rv_window_manager_dirty_windowing(&server.wm);
		break;
	}
}

static void handle_associate(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, associate);

	assert(xwindow->xsurface->surface != NULL); // .?
	wl_signal_add(&xwindow->xsurface->surface->events.map, &xwindow->map);
	wl_signal_add(&xwindow->xsurface->surface->events.unmap, &xwindow->unmap);
}

static void handle_dissociate(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, dissociate);
	wl_list_remove(&xwindow->map.link);
	wl_list_remove(&xwindow->unmap.link);
}

void rv_xwayland_window_handle_map(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, map);
	struct rv_window *window = xwindow->window;
	struct wlr_surface *surface = xwindow->xsurface->surface;
	assert(surface != NULL); // .?

	struct wlr_scene_tree *surface_tree = wlr_scene_subsurface_tree_create(window->surfaces.tree, surface);
	if (surface_tree == NULL) {
		rv_err(RV_SCOPE_XWAYLAND, "out of memory");
		wl_client_post_no_memory(wl_resource_get_client(surface->resource));
		return;
	}
	xwindow->surface_tree = surface_tree;
	surface->data = &window->tree->node;

	if (wlr_scene_surface_create(&window->capture_scene->tree, surface) == NULL) {
		rv_err(RV_SCOPE_XWAYLAND, "out of memory");
		wl_client_post_no_memory(wl_resource_get_client(surface->resource));
		return;
	}

	if (xwindow->xsurface->fullscreen) {
		window->wm_scheduled.fullscreen_requested = (struct rv_window_fullscreen_request){
			.tag = RV_WINDOW_FULLSCREEN_REQUEST_FULLSCREEN,
			.fullscreen = NULL,
		};
	}

	window->state = RV_WINDOW_STATE_INITIALIZED;
	if (!rv_window_map(window)) {
		rv_err(RV_SCOPE_XWAYLAND, "out of memory");
		wl_client_post_no_memory(wl_resource_get_client(surface->resource));
	}
	rv_window_manager_dirty_windowing(&server.wm);
}

static void handle_unmap(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, unmap);

	assert(xwindow->xsurface->surface != NULL); // .?
	xwindow->xsurface->surface->data = NULL;

	rv_window_unmap(xwindow->window);

	// Don't destroy the surface tree until after Window.unmap() has a chance
	// to save buffers for frame perfection.
	assert(xwindow->surface_tree != NULL); // .?
	wlr_scene_node_destroy(&xwindow->surface_tree->node);
	xwindow->surface_tree = NULL;
}

static void handle_request_configure(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, request_configure);
	struct wlr_xwayland_surface_configure_event *event = data;

	// If unmapped, let the client do whatever it wants
	if (xwindow->xsurface->surface == NULL || !xwindow->xsurface->surface->mapped) {
		wlr_xwayland_surface_configure(xwindow->xsurface, event->x, event->y, event->width, event->height);
		return;
	}

	wlr_xwayland_surface_configure(xwindow->xsurface,
		lossy_cast_i16(xwindow->window->box.x),
		lossy_cast_i16(xwindow->window->box.y),
		event->width,
		event->height);
	rv_window_set_dimensions(xwindow->window, event->width, event->height);
}

static void handle_set_override_redirect(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, set_override_redirect);
	struct wlr_xwayland_surface *xsurface = xwindow->xsurface;

	rv_debug(RV_SCOPE_XWAYLAND, "xwayland surface set override redirect");

	assert(xsurface->override_redirect);

	struct wlr_surface *surface = xsurface->surface;
	if (surface != NULL) {
		if (surface->mapped) {
			handle_unmap(&xwindow->unmap, NULL);
		}
		handle_dissociate(&xwindow->dissociate, NULL);
	}
	handle_destroy(&xwindow->destroy, NULL);

	if (!rv_xwayland_override_redirect_create(xsurface)) {
		rv_err(RV_SCOPE_XWAYLAND, "out of memory");
		return;
	}
}

static void handle_set_size_hints(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, set_size_hints);
	xcb_size_hints_t *size_hints = xwindow->xsurface->size_hints;
	if (size_hints != NULL) {
		const int32_t min_width = size_hints->min_width > 0 ? size_hints->min_width : 0; // u31
		const int32_t min_height = size_hints->min_height > 0 ? size_hints->min_height : 0; // u31
		// Don't trust X11 clients not to set a min_width greater than their max_width.
		const int32_t max_width = // u31
			size_hints->max_width <= 0 ? 0 :
				(min_width > size_hints->max_width ? min_width : size_hints->max_width);
		const int32_t max_height = // u31
			size_hints->max_height <= 0 ? 0 :
				(min_height > size_hints->max_height ? min_height : size_hints->max_height);
		rv_window_set_dimensions_hint(xwindow->window, (struct rv_window_dimensions_hint){
			.min_width = min_width,
			.max_width = max_width,
			.min_height = min_height,
			.max_height = max_height,
		});
	}
}

static void handle_set_title(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, set_title);
	rv_window_notify_title(xwindow->window);
}

static void handle_set_class(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, set_class);
	rv_window_notify_app_id(xwindow->window);
}

static void handle_set_parent(struct wl_listener *listener, void *data) {
	rv_window_manager_dirty_windowing(&server.wm);
}

static void handle_set_decorations(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, set_decorations);

	if ((xwindow->xsurface->decorations & WLR_XWAYLAND_SURFACE_DECORATIONS_NO_BORDER) ||
			(xwindow->xsurface->decorations & WLR_XWAYLAND_SURFACE_DECORATIONS_NO_TITLE)) {
		rv_window_set_decoration_hint(xwindow->window, RIVER_WINDOW_V1_DECORATION_HINT_PREFERS_CSD);
	} else {
		rv_window_set_decoration_hint(xwindow->window, RIVER_WINDOW_V1_DECORATION_HINT_PREFERS_SSD);
	}
}

static void handle_request_maximize(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, request_maximize);
	if (xwindow->xsurface->maximized_vert || xwindow->xsurface->maximized_horz) {
		xwindow->window->wm_scheduled.maximize_requested = RV_WINDOW_WM_SCHEDULED_MAXIMIZE_REQUESTED_MAXIMIZE;
	} else {
		xwindow->window->wm_scheduled.maximize_requested = RV_WINDOW_WM_SCHEDULED_MAXIMIZE_REQUESTED_UNMAXIMIZE;
	}
	rv_window_manager_dirty_windowing(&server.wm);
}

static void handle_request_fullscreen(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, request_fullscreen);
	if (xwindow->xsurface->fullscreen) {
		xwindow->window->wm_scheduled.fullscreen_requested = (struct rv_window_fullscreen_request){
			.tag = RV_WINDOW_FULLSCREEN_REQUEST_FULLSCREEN,
			.fullscreen = NULL,
		};
	} else {
		xwindow->window->wm_scheduled.fullscreen_requested = (struct rv_window_fullscreen_request){
			.tag = RV_WINDOW_FULLSCREEN_REQUEST_EXIT,
		};
	}
	rv_window_manager_dirty_windowing(&server.wm);
}

// Some X11 clients will minimize themselves regardless of how we respond.
// Therefore to ensure they don't get stuck in this minimized state we tell
// them their request has been honored without actually doing anything and
// unminimize them if they gain focus while minimized.
static void handle_request_minimize(struct wl_listener *listener, void *data) {
	struct rv_xwayland_window *xwindow = wl_container_of(listener, xwindow, request_minimize);
	struct wlr_xwayland_minimize_event *event = data;
	wlr_xwayland_surface_set_minimized(xwindow->xsurface, event->minimize);
	xwindow->window->wm_scheduled.minimize_requested = true;
	rv_window_manager_dirty_windowing(&server.wm);
}

#endif
