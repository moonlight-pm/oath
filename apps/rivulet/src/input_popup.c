// SPDX-FileCopyrightText: © 2024 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/InputPopup.zig for Oath, 2026-10-18.

#include "rivulet.h"

static void handle_destroy(struct wl_listener *listener, void *data);
static void handle_map(struct wl_listener *listener, void *data);
static void handle_unmap(struct wl_listener *listener, void *data);
static void handle_commit(struct wl_listener *listener, void *data);

bool rv_input_popup_create(struct wlr_input_popup_surface_v2 *wlr_popup, struct rv_input_relay *input_relay) {
	struct rv_input_popup *input_popup = calloc(1, sizeof(*input_popup));
	if (input_popup == NULL) {
		return false;
	}

	struct wlr_scene_tree *surface_tree = wlr_scene_subsurface_tree_create(server.scene.hidden_tree,
		wlr_popup->surface);
	if (surface_tree == NULL) {
		free(input_popup);
		return false;
	}

	*input_popup = (struct rv_input_popup){
		.input_relay = input_relay,
		.wlr_popup = wlr_popup,
		.surface_tree = surface_tree,
	};
	input_popup->destroy.notify = handle_destroy;
	input_popup->map.notify = handle_map;
	input_popup->unmap.notify = handle_unmap;
	input_popup->commit.notify = handle_commit;

	wl_list_insert(input_relay->input_popups.prev, &input_popup->link);

	wl_signal_add(&input_popup->wlr_popup->events.destroy, &input_popup->destroy);
	wl_signal_add(&input_popup->wlr_popup->surface->events.map, &input_popup->map);
	wl_signal_add(&input_popup->wlr_popup->surface->events.unmap, &input_popup->unmap);
	wl_signal_add(&input_popup->wlr_popup->surface->events.commit, &input_popup->commit);

	rv_input_popup_update(input_popup);

	return true;
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_input_popup *input_popup = wl_container_of(listener, input_popup, destroy);

	wl_list_remove(&input_popup->destroy.link);
	wl_list_remove(&input_popup->map.link);
	wl_list_remove(&input_popup->unmap.link);
	wl_list_remove(&input_popup->commit.link);

	wl_list_remove(&input_popup->link);

	free(input_popup);
}

static void handle_map(struct wl_listener *listener, void *data) {
	struct rv_input_popup *input_popup = wl_container_of(listener, input_popup, map);

	rv_input_popup_update(input_popup);
}

static void handle_unmap(struct wl_listener *listener, void *data) {
	struct rv_input_popup *input_popup = wl_container_of(listener, input_popup, unmap);

	wlr_scene_node_reparent(&input_popup->surface_tree->node, server.scene.hidden_tree);
}

static void handle_commit(struct wl_listener *listener, void *data) {
	struct rv_input_popup *input_popup = wl_container_of(listener, input_popup, commit);

	rv_input_popup_update(input_popup);
}

void rv_input_popup_update(struct rv_input_popup *input_popup) {
	struct rv_text_input *text_input = input_popup->input_relay->text_input;
	if (text_input == NULL) {
		wlr_scene_node_reparent(&input_popup->surface_tree->node, server.scene.hidden_tree);
		return;
	}

	if (!input_popup->wlr_popup->surface->mapped) {
		return;
	}

	// This seems like it could be null if the focused surface is destroyed
	struct wlr_surface *focused_surface = text_input->wlr_text_input->focused_surface;
	if (focused_surface == NULL) {
		return;
	}

	// Focus should never be sent to subsurfaces
	assert(wlr_surface_get_root_surface(focused_surface) == focused_surface);

	struct rv_scene_node_data *focused = rv_scene_node_data_from_surface(focused_surface);
	if (focused == NULL) {
		return;
	}

	struct wlr_scene_tree *popup_tree = NULL;
	switch (focused->data.tag) {
	case RV_SCENE_NODE_DATA_DATA_WINDOW:
		popup_tree = focused->data.window->popup_tree;
		break;
	case RV_SCENE_NODE_DATA_DATA_SHELL_SURFACE:
		popup_tree = focused->data.shell_surface->popup_tree;
		break;
	// TODO fix positioning for lock surfaces not at 0,0 in layout coords
	case RV_SCENE_NODE_DATA_DATA_LOCK_SURFACE:
		popup_tree = server.scene.layers.popups;
		break;
	case RV_SCENE_NODE_DATA_DATA_LAYER_SURFACE:
		popup_tree = focused->data.layer_surface->popup_tree;
		break;
	// Xwayland doesn't use the text-input protocol
	case RV_SCENE_NODE_DATA_DATA_OVERRIDE_REDIRECT:
		abort(); // unreachable
	}

	wlr_scene_node_reparent(&input_popup->surface_tree->node, popup_tree);

	if (!(text_input->wlr_text_input->current.features & WLR_TEXT_INPUT_V3_FEATURE_CURSOR_RECTANGLE)) {
		// If the text-input client does not inform us where in the surface
		// the active text input is there's not much we can do. Placing the
		// popup at the top left corner of the window is nice and simple
		// while not looking terrible.
		wlr_scene_node_set_position(&input_popup->surface_tree->node, 0, 0);
		return;
	}

	int focused_x;
	int focused_y;
	wlr_scene_node_coords(focused->node, &focused_x, &focused_y);

	// Relative to the surface with the active text input
	struct wlr_box cursor_box = text_input->wlr_text_input->current.cursor_rectangle;

	struct wlr_output *wlr_output = rv_output_manager_output_at(
		&server.om,
		(double)(focused_x + cursor_box.x),
		(double)(focused_y + cursor_box.y)
	);
	if (wlr_output == NULL) {
		return;
	}

	struct wlr_box output_box;
	wlr_output_layout_get_box(server.om.output_layout, wlr_output, &output_box);

	// Adjust to be relative to the output
	cursor_box.x += focused_x - output_box.x;
	cursor_box.y += focused_y - output_box.y;

	// Choose popup x/y relative to the output:

	// Align the left edge of the popup with the left edge of the cursor.
	// If the popup wouldn't fit on the output instead align the right edge
	// of the popup with the right edge of the cursor.
	int popup_x;
	{
		const int popup_width = input_popup->wlr_popup->surface->current.width;
		if (output_box.width - cursor_box.x >= popup_width) {
			popup_x = cursor_box.x;
		} else {
			popup_x = cursor_box.x + cursor_box.width - popup_width;
		}
	}

	// Align the top edge of the popup with the bottom edge of the cursor.
	// If the popup wouldn't fit on the output instead align the bottom edge
	// of the popup with the top edge of the cursor.
	int popup_y;
	{
		const int popup_height = input_popup->wlr_popup->surface->current.height;
		if (output_box.height - (cursor_box.y + cursor_box.height) >= popup_height) {
			popup_y = cursor_box.y + cursor_box.height;
		} else {
			popup_y = cursor_box.y - popup_height;
		}
	}

	// Scene node position is relative to the parent so adjust popup x/y to
	// be relative to the focused surface.
	wlr_scene_node_set_position(
		&input_popup->surface_tree->node,
		popup_x - focused_x + output_box.x,
		popup_y - focused_y + output_box.y
	);

	// The text input rectangle sent to the input method is relative to the popup.
	cursor_box.x -= popup_x;
	cursor_box.y -= popup_y;
	wlr_input_popup_surface_v2_send_text_input_rectangle(input_popup->wlr_popup, &cursor_box);
}
