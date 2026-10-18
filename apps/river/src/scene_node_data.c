// SPDX-FileCopyrightText: © 2023 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/SceneNodeData.zig for Oath, 2026-10-18.

#include "river.h"

static void handle_destroy(struct wl_listener *listener, void *data);

bool rv_scene_node_data_attach(struct wlr_scene_node *node, struct rv_scene_node_data_data data) {
	struct rv_scene_node_data *scene_node_data = calloc(1, sizeof(*scene_node_data));
	if (scene_node_data == NULL) {
		return false;
	}

	*scene_node_data = (struct rv_scene_node_data){
		.node = node,
		.data = data,
	};
	scene_node_data->destroy.notify = handle_destroy;
	node->data = scene_node_data;

	wl_signal_add(&node->events.destroy, &scene_node_data->destroy);
	return true;
}

struct rv_scene_node_data *rv_scene_node_data_from_node(struct wlr_scene_node *node) {
	struct wlr_scene_node *n = node;
	while (true) {
		struct rv_scene_node_data *scene_node_data = n->data;
		if (scene_node_data != NULL) {
			return scene_node_data;
		}
		if (n->parent != NULL) {
			n = &n->parent->node;
		} else {
			return NULL;
		}
	}
}

struct rv_scene_node_data *rv_scene_node_data_from_surface(struct wlr_surface *surface) {
	struct wlr_scene_node *node = wlr_surface_get_root_surface(surface)->data;
	if (node != NULL) {
		return rv_scene_node_data_from_node(node);
	}
	return NULL;
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_scene_node_data *scene_node_data = wl_container_of(listener, scene_node_data, destroy);

	wl_list_remove(&scene_node_data->destroy.link);
	scene_node_data->node->data = NULL;

	free(scene_node_data);
}
