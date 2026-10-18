// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/XwaylandOverrideRedirect.zig for Oath, 2026-10-18.

#include "river.h"

#if WLR_HAS_XWAYLAND

static void handle_request_configure(struct wl_listener *listener, void *data);
static void handle_destroy(struct wl_listener *listener, void *data);
static void handle_set_override_redirect(struct wl_listener *listener, void *data);
static void handle_associate(struct wl_listener *listener, void *data);
static void handle_dissociate(struct wl_listener *listener, void *data);
static void handle_unmap(struct wl_listener *listener, void *data);
static void handle_set_geometry(struct wl_listener *listener, void *data);

// Zig {?s}: a null string prints as "null".
static const char *opt_str(const char *s) {
	return s != NULL ? s : "null";
}

bool rv_xwayland_override_redirect_create(struct wlr_xwayland_surface *xsurface) {
	rv_debug(RV_SCOPE_XWAYLAND, "new xwayland override redirect: title='%s', class='%s'",
		opt_str(xsurface->title),
		opt_str(xsurface->class));

	struct rv_xwayland_override_redirect *override_redirect = calloc(1, sizeof(*override_redirect));
	if (override_redirect == NULL) {
		return false;
	}

	*override_redirect = (struct rv_xwayland_override_redirect){
		.xsurface = xsurface,
		.surface_tree = NULL,
	};
	override_redirect->request_configure.notify = handle_request_configure;
	override_redirect->destroy.notify = handle_destroy;
	override_redirect->set_override_redirect.notify = handle_set_override_redirect;
	override_redirect->associate.notify = handle_associate;
	override_redirect->dissociate.notify = handle_dissociate;
	override_redirect->map.notify = rv_xwayland_override_redirect_handle_map;
	override_redirect->unmap.notify = handle_unmap;
	override_redirect->set_geometry.notify = handle_set_geometry;

	wl_signal_add(&xsurface->events.request_configure, &override_redirect->request_configure);
	wl_signal_add(&xsurface->events.destroy, &override_redirect->destroy);
	wl_signal_add(&xsurface->events.set_override_redirect, &override_redirect->set_override_redirect);

	wl_signal_add(&xsurface->events.associate, &override_redirect->associate);
	wl_signal_add(&xsurface->events.dissociate, &override_redirect->dissociate);

	struct wlr_surface *surface = xsurface->surface;
	if (surface != NULL) {
		handle_associate(&override_redirect->associate, NULL);
		if (surface->mapped) {
			rv_xwayland_override_redirect_handle_map(&override_redirect->map, NULL);
		}
	}

	return true;
}

static void handle_request_configure(struct wl_listener *listener, void *data) {
	struct wlr_xwayland_surface_configure_event *event = data;
	wlr_xwayland_surface_configure(event->surface, event->x, event->y, event->width, event->height);
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_xwayland_override_redirect *override_redirect = wl_container_of(listener, override_redirect, destroy);

	wl_list_remove(&override_redirect->request_configure.link);
	wl_list_remove(&override_redirect->destroy.link);
	wl_list_remove(&override_redirect->associate.link);
	wl_list_remove(&override_redirect->dissociate.link);
	wl_list_remove(&override_redirect->set_override_redirect.link);

	free(override_redirect);
}

static void handle_associate(struct wl_listener *listener, void *data) {
	struct rv_xwayland_override_redirect *override_redirect =
		wl_container_of(listener, override_redirect, associate);

	assert(override_redirect->xsurface->surface != NULL); // .?
	wl_signal_add(&override_redirect->xsurface->surface->events.map, &override_redirect->map);
	wl_signal_add(&override_redirect->xsurface->surface->events.unmap, &override_redirect->unmap);
}

static void handle_dissociate(struct wl_listener *listener, void *data) {
	struct rv_xwayland_override_redirect *override_redirect =
		wl_container_of(listener, override_redirect, dissociate);

	wl_list_remove(&override_redirect->map.link);
	wl_list_remove(&override_redirect->unmap.link);
}

static bool map_impl(struct rv_xwayland_override_redirect *override_redirect);

void rv_xwayland_override_redirect_handle_map(struct wl_listener *listener, void *data) {
	struct rv_xwayland_override_redirect *override_redirect = wl_container_of(listener, override_redirect, map);

	if (!map_impl(override_redirect)) {
		rv_err(RV_SCOPE_XWAYLAND, "out of memory");
		assert(override_redirect->xsurface->surface != NULL); // .?
		wl_client_post_no_memory(wl_resource_get_client(override_redirect->xsurface->surface->resource));
	}
}

// Zig error{OutOfMemory}!void: returns false on OOM.
static bool map_impl(struct rv_xwayland_override_redirect *override_redirect) {
	struct wlr_surface *surface = override_redirect->xsurface->surface;
	assert(surface != NULL); // .?
	struct wlr_scene_tree *surface_tree =
		wlr_scene_subsurface_tree_create(server.scene.layers.override_redirect, surface);
	if (surface_tree == NULL) {
		return false;
	}
	override_redirect->surface_tree = surface_tree;
	if (!rv_scene_node_data_attach(&override_redirect->surface_tree->node, (struct rv_scene_node_data_data){
			.tag = RV_SCENE_NODE_DATA_DATA_OVERRIDE_REDIRECT,
			.override_redirect = override_redirect,
		})) {
		return false;
	}

	surface->data = &override_redirect->surface_tree->node;

	wlr_scene_node_set_position(&override_redirect->surface_tree->node,
		override_redirect->xsurface->x,
		override_redirect->xsurface->y);

	wl_signal_add(&override_redirect->xsurface->events.set_geometry, &override_redirect->set_geometry);

	rv_xwayland_override_redirect_focus_if_desired(override_redirect);

	return true;
}

void rv_xwayland_override_redirect_focus_if_desired(struct rv_xwayland_override_redirect *override_redirect) {
	if (server.lock_manager.state != RV_LOCK_MANAGER_STATE_UNLOCKED) {
		return;
	}

	if (wlr_xwayland_surface_override_redirect_wants_focus(override_redirect->xsurface) &&
			wlr_xwayland_surface_icccm_input_model(override_redirect->xsurface) != WLR_ICCCM_INPUT_MODEL_NONE) {
		struct rv_seat *seat = rv_input_manager_default_seat(&server.input_manager);
		// Keep the parent top-level Xwayland window of any override redirect surface
		// activated while that override redirect surface is focused. This ensures
		// override redirect menus do not disappear as a result of deactivating
		// their parent window.
		if (seat->focused.tag == RV_SEAT_FOCUS_WINDOW &&
				seat->focused.window->impl.tag == RV_WINDOW_IMPL_XWAYLAND &&
				seat->focused.window->impl.xwayland.xsurface->pid == override_redirect->xsurface->pid) {
			rv_seat_keyboard_enter_or_leave(seat, override_redirect->xsurface->surface);
		} else {
			rv_seat_focus(seat, (struct rv_seat_focus){
				.tag = RV_SEAT_FOCUS_OVERRIDE_REDIRECT,
				.override_redirect = override_redirect,
			});
		}
	}
}

static void handle_unmap(struct wl_listener *listener, void *data) {
	struct rv_xwayland_override_redirect *override_redirect = wl_container_of(listener, override_redirect, unmap);

	wl_list_remove(&override_redirect->set_geometry.link);

	assert(override_redirect->xsurface->surface != NULL); // .?
	override_redirect->xsurface->surface->data = NULL;
	assert(override_redirect->surface_tree != NULL); // .?
	wlr_scene_node_destroy(&override_redirect->surface_tree->node);
	override_redirect->surface_tree = NULL;

	// If the unmapped surface is currently focused, pass keyboard focus
	// to the most appropriate surface.
	struct rv_seat *seat;
	wl_list_for_each(seat, &server.input_manager.seats, link) {
		if (seat->focused.tag == RV_SEAT_FOCUS_WINDOW &&
				seat->focused.window->impl.tag == RV_WINDOW_IMPL_XWAYLAND &&
				seat->focused.window->impl.xwayland.xsurface->pid == override_redirect->xsurface->pid &&
				seat->wlr_seat->keyboard_state.focused_surface == override_redirect->xsurface->surface) {
			rv_seat_keyboard_enter_or_leave(seat, rv_window_root_surface(seat->focused.window));
		}
	}

	rv_window_manager_dirty_windowing(&server.wm);
}

static void handle_set_geometry(struct wl_listener *listener, void *data) {
	struct rv_xwayland_override_redirect *override_redirect =
		wl_container_of(listener, override_redirect, set_geometry);

	assert(override_redirect->surface_tree != NULL); // .?
	wlr_scene_node_set_position(&override_redirect->surface_tree->node,
		override_redirect->xsurface->x,
		override_redirect->xsurface->y);
}

static void handle_set_override_redirect(struct wl_listener *listener, void *data) {
	struct rv_xwayland_override_redirect *override_redirect =
		wl_container_of(listener, override_redirect, set_override_redirect);
	struct wlr_xwayland_surface *xsurface = override_redirect->xsurface;

	rv_debug(RV_SCOPE_XWAYLAND, "xwayland surface unset override redirect");

	assert(!xsurface->override_redirect);

	struct wlr_surface *surface = xsurface->surface;
	if (surface != NULL) {
		if (surface->mapped) {
			handle_unmap(&override_redirect->unmap, NULL);
		}
		handle_dissociate(&override_redirect->dissociate, NULL);
	}
	handle_destroy(&override_redirect->destroy, NULL);

	if (!rv_xwayland_window_create(xsurface)) {
		rv_err(RV_SCOPE_XWAYLAND, "out of memory");
		return;
	}
}

#endif
