// SPDX-FileCopyrightText: © 2025 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/LayerSurface.zig for Oath, 2026-10-18.

#include "rivulet.h"

static void handle_destroy(struct wl_listener *listener, void *data);
static void handle_map(struct wl_listener *listener, void *data);
static void handle_unmap(struct wl_listener *listener, void *data);
static void handle_commit(struct wl_listener *listener, void *data);
static void handle_new_popup(struct wl_listener *listener, void *data);

struct rv_layer_surface *rv_layer_surface_ref_get(struct rv_layer_surface_ref ref) {
	return rv_slotmap_get(&server.layer_shell.surfaces, ref.key);
}

bool rv_layer_surface_create(struct wlr_layer_surface_v1 *wlr_layer_surface) {
	struct rv_layer_surface *layer_surface = calloc(1, sizeof(*layer_surface));
	if (layer_surface == NULL) {
		return false;
	}

	struct rv_slotmap_key key;
	if (!rv_slotmap_put(&server.layer_shell.surfaces, layer_surface, &key)) {
		goto error_put;
	}

	struct wlr_scene_tree *layer_tree = rv_scene_layer_surface_tree(&server.scene, wlr_layer_surface->current.layer);

	struct wlr_scene_layer_surface_v1 *scene_layer_surface =
		wlr_scene_layer_surface_v1_create(layer_tree, wlr_layer_surface);
	if (scene_layer_surface == NULL) {
		goto error_key;
	}
	struct wlr_scene_tree *popup_tree = wlr_scene_tree_create(server.scene.layers.popups);
	if (popup_tree == NULL) {
		goto error_key;
	}

	layer_surface->ref = (struct rv_layer_surface_ref){ .key = key };
	layer_surface->wlr_layer_surface = wlr_layer_surface;
	layer_surface->scene_layer_surface = scene_layer_surface;
	layer_surface->popup_tree = popup_tree;
	layer_surface->destroy.notify = handle_destroy;
	layer_surface->map.notify = handle_map;
	layer_surface->unmap.notify = handle_unmap;
	layer_surface->commit.notify = handle_commit;
	layer_surface->new_popup.notify = handle_new_popup;

	if (!rv_scene_node_data_attach(&layer_surface->scene_layer_surface->tree->node,
			(struct rv_scene_node_data_data){
				.tag = RV_SCENE_NODE_DATA_DATA_LAYER_SURFACE,
				.layer_surface = layer_surface,
			})) {
		goto error_key;
	}
	if (!rv_scene_node_data_attach(&layer_surface->popup_tree->node,
			(struct rv_scene_node_data_data){
				.tag = RV_SCENE_NODE_DATA_DATA_LAYER_SURFACE,
				.layer_surface = layer_surface,
			})) {
		goto error_key;
	}

	wlr_layer_surface->surface->data = &layer_surface->scene_layer_surface->tree->node;

	wl_signal_add(&wlr_layer_surface->events.destroy, &layer_surface->destroy);
	wl_signal_add(&wlr_layer_surface->surface->events.map, &layer_surface->map);
	wl_signal_add(&wlr_layer_surface->surface->events.unmap, &layer_surface->unmap);
	wl_signal_add(&wlr_layer_surface->surface->events.commit, &layer_surface->commit);
	wl_signal_add(&wlr_layer_surface->events.new_popup, &layer_surface->new_popup);

	return true;

	// The Zig errdefers: the scene nodes created above are not cleaned up on error.
error_key:
	rv_slotmap_remove(&server.layer_shell.surfaces, key);
error_put:
	free(layer_surface);
	return false;
}

void rv_layer_surface_destroy_popups(struct rv_layer_surface *layer_surface) {
	struct wlr_xdg_popup *wlr_xdg_popup, *tmp;
	wl_list_for_each_safe(wlr_xdg_popup, tmp, &layer_surface->wlr_layer_surface->popups, link) {
		wlr_xdg_popup_destroy(wlr_xdg_popup);
	}
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_layer_surface *layer_surface = wl_container_of(listener, layer_surface, destroy);

	rv_debug(RV_SCOPE_WM, "layer surface '%s' destroyed", layer_surface->wlr_layer_surface->namespace);

	wl_list_remove(&layer_surface->destroy.link);
	wl_list_remove(&layer_surface->map.link);
	wl_list_remove(&layer_surface->unmap.link);
	wl_list_remove(&layer_surface->commit.link);
	wl_list_remove(&layer_surface->new_popup.link);

	rv_layer_surface_destroy_popups(layer_surface);

	wlr_scene_node_destroy(&layer_surface->popup_tree->node);

	// The wlr_surface may outlive the wlr_layer_surface so we must clean up the user data.
	layer_surface->wlr_layer_surface->surface->data = NULL;

	rv_slotmap_remove(&server.layer_shell.surfaces, layer_surface->ref.key);
	free(layer_surface);
}

static void handle_map(struct wl_listener *listener, void *data) {
	struct rv_layer_surface *layer_surface = wl_container_of(listener, layer_surface, map);
	struct wlr_layer_surface_v1 *wlr_layer_surface = layer_surface->wlr_layer_surface;

	rv_debug(RV_SCOPE_WM, "layer surface '%s' mapped", wlr_layer_surface->namespace);

	if (wlr_layer_surface->current.keyboard_interactive == ZWLR_LAYER_SURFACE_V1_KEYBOARD_INTERACTIVITY_ON_DEMAND) {
		struct rv_seat *seat, *tmp;
		wl_list_for_each_safe(seat, tmp, &server.input_manager.seats, link) {
			if (seat->layer_shell.scheduled.focus.tag != RV_LAYER_SHELL_SEAT_FOCUS_EXCLUSIVE) {
				seat->layer_shell.scheduled.focus = (struct rv_layer_shell_seat_focus){
					.tag = RV_LAYER_SHELL_SEAT_FOCUS_NON_EXCLUSIVE,
					.non_exclusive = layer_surface->ref,
				};
			}
		}
	}

	// Beware: it is possible for arrange() to destroy this LayerSurface!
	assert(layer_surface->wlr_layer_surface->output != NULL);
	struct rv_output *output = layer_surface->wlr_layer_surface->output->data;
	rv_layer_shell_output_arrange(&output->layer_shell);
	rv_layer_shell_check_exclusive_focus(&server.layer_shell);
	rv_window_manager_dirty_windowing(&server.wm);
}

static void handle_unmap(struct wl_listener *listener, void *data) {
	struct rv_layer_surface *layer_surface = wl_container_of(listener, layer_surface, unmap);

	rv_debug(RV_SCOPE_WM, "layer surface '%s' unmapped", layer_surface->wlr_layer_surface->namespace);

	{
		struct rv_seat *seat, *tmp;
		wl_list_for_each_safe(seat, tmp, &server.input_manager.seats, link) {
			if (seat->focused.tag == RV_SEAT_FOCUS_LAYER_SURFACE && seat->focused.layer_surface == layer_surface) {
				rv_seat_focus(seat, (struct rv_seat_focus){ .tag = RV_SEAT_FOCUS_NONE });
			}
			if (seat->layer_shell.scheduled.focus.tag == RV_LAYER_SHELL_SEAT_FOCUS_NON_EXCLUSIVE &&
					rv_layer_surface_ref_eql(seat->layer_shell.scheduled.focus.non_exclusive,
						layer_surface->ref)) {
				seat->layer_shell.scheduled.focus =
					(struct rv_layer_shell_seat_focus){ .tag = RV_LAYER_SHELL_SEAT_FOCUS_NONE };
			}
		}
	}

	// Beware: it is possible for arrange() to destroy this LayerSurface!
	assert(layer_surface->wlr_layer_surface->output != NULL);
	struct rv_output *output = layer_surface->wlr_layer_surface->output->data;
	rv_layer_shell_output_arrange(&output->layer_shell);
	rv_layer_shell_check_exclusive_focus(&server.layer_shell);
	rv_window_manager_dirty_windowing(&server.wm);
}

static void handle_commit(struct wl_listener *listener, void *data) {
	struct rv_layer_surface *layer_surface = wl_container_of(listener, layer_surface, commit);
	struct wlr_layer_surface_v1 *wlr_layer_surface = layer_surface->wlr_layer_surface;

	assert(wlr_layer_surface->output != NULL);

	// If the layer was changed, move the LayerSurface to the proper tree.
	if (wlr_layer_surface->current.committed & WLR_LAYER_SURFACE_V1_STATE_LAYER) {
		struct wlr_scene_tree *tree = rv_scene_layer_surface_tree(&server.scene, wlr_layer_surface->current.layer);
		wlr_scene_node_reparent(&layer_surface->scene_layer_surface->tree->node, tree);
	}

	if (wlr_layer_surface->initial_commit || wlr_layer_surface->current.committed != 0) {
		// Beware: it is possible for arrange() to destroy this LayerSurface!
		struct rv_output *output = layer_surface->wlr_layer_surface->output->data;
		rv_layer_shell_output_arrange(&output->layer_shell);
		rv_layer_shell_check_exclusive_focus(&server.layer_shell);
		rv_window_manager_dirty_windowing(&server.wm);
	}
}

static void handle_new_popup(struct wl_listener *listener, void *data) {
	struct rv_layer_surface *layer_surface = wl_container_of(listener, layer_surface, new_popup);
	struct wlr_xdg_popup *wlr_xdg_popup = data;

	if (!rv_xdg_popup_create(wlr_xdg_popup, layer_surface->popup_tree, layer_surface->popup_tree, NULL)) {
		wl_resource_post_no_memory(wlr_xdg_popup->resource);
		return;
	}
}
