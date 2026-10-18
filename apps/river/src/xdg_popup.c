// SPDX-FileCopyrightText: © 2023 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/XdgPopup.zig for Oath, 2026-10-18.

#include "river.h"

static void handle_destroy(struct wl_listener *listener, void *data);
static void handle_commit(struct wl_listener *listener, void *data);
static void handle_new_popup(struct wl_listener *listener, void *data);
static void handle_reposition(struct wl_listener *listener, void *data);

// TODO check if popup is set_reactive and reposition on parent movement.
bool rv_xdg_popup_create(struct wlr_xdg_popup *wlr_popup, struct wlr_scene_tree *root,
		struct wlr_scene_tree *parent, struct wlr_scene_tree *capture_parent) {
	struct rv_xdg_popup *xdg_popup = calloc(1, sizeof(*xdg_popup));
	if (xdg_popup == NULL) {
		return false;
	}

	struct wlr_scene_tree *tree = wlr_scene_xdg_surface_create(parent, wlr_popup->base);
	if (tree == NULL) {
		free(xdg_popup);
		return false;
	}
	*xdg_popup = (struct rv_xdg_popup){
		.wlr_popup = wlr_popup,
		.root = root,
		.tree = tree,
		.capture_tree = NULL,
	};
	xdg_popup->destroy.notify = handle_destroy;
	xdg_popup->commit.notify = handle_commit;
	xdg_popup->new_popup.notify = handle_new_popup;
	xdg_popup->reposition.notify = handle_reposition;

	if (capture_parent != NULL) {
		xdg_popup->capture_tree = wlr_scene_xdg_surface_create(capture_parent, wlr_popup->base);
		if (xdg_popup->capture_tree == NULL) {
			free(xdg_popup);
			return false;
		}
	}

	wl_signal_add(&wlr_popup->events.destroy, &xdg_popup->destroy);
	wl_signal_add(&wlr_popup->base->surface->events.commit, &xdg_popup->commit);
	wl_signal_add(&wlr_popup->base->events.new_popup, &xdg_popup->new_popup);
	wl_signal_add(&wlr_popup->events.reposition, &xdg_popup->reposition);

	return true;
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_xdg_popup *xdg_popup = wl_container_of(listener, xdg_popup, destroy);

	wl_list_remove(&xdg_popup->destroy.link);
	wl_list_remove(&xdg_popup->commit.link);
	wl_list_remove(&xdg_popup->new_popup.link);
	wl_list_remove(&xdg_popup->reposition.link);

	free(xdg_popup);
}

static void handle_commit(struct wl_listener *listener, void *data) {
	struct rv_xdg_popup *xdg_popup = wl_container_of(listener, xdg_popup, commit);

	if (xdg_popup->wlr_popup->base->initial_commit) {
		handle_reposition(&xdg_popup->reposition, NULL);
	}
}

static void handle_new_popup(struct wl_listener *listener, void *data) {
	struct rv_xdg_popup *xdg_popup = wl_container_of(listener, xdg_popup, new_popup);
	struct wlr_xdg_popup *wlr_popup = data;

	if (!rv_xdg_popup_create(
			wlr_popup,
			xdg_popup->root,
			xdg_popup->tree,
			xdg_popup->capture_tree)) {
		wl_resource_post_no_memory(wlr_popup->resource);
		return;
	}
}

static void handle_reposition(struct wl_listener *listener, void *data) {
	struct rv_xdg_popup *xdg_popup = wl_container_of(listener, xdg_popup, reposition);

	int root_lx;
	int root_ly;
	(void)wlr_scene_node_coords(&xdg_popup->root->node, &root_lx, &root_ly);

	struct wlr_output *wlr_output = rv_output_manager_output_at(&server.om,
		(double)(root_lx + xdg_popup->wlr_popup->scheduled.geometry.x),
		(double)(root_ly + xdg_popup->wlr_popup->scheduled.geometry.y));
	if (wlr_output == NULL) {
		return;
	}

	struct wlr_box box;
	wlr_output_layout_get_box(server.om.output_layout, wlr_output, &box);

	box.x -= root_lx;
	box.y -= root_ly;

	wlr_xdg_popup_unconstrain_from_box(xdg_popup->wlr_popup, &box);
}
