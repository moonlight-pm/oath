// SPDX-FileCopyrightText: © 2024 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Scene.zig for Oath, 2026-10-18.

#include "river.h"

bool rv_scene_init(struct rv_scene *scene) {
	struct wlr_scene *wlr_scene = wlr_scene_create();
	if (wlr_scene == NULL) {
		return false;
	}

	if (server.linux_dmabuf != NULL) {
		wlr_scene_set_linux_dmabuf_v1(wlr_scene, server.linux_dmabuf);
	}
	if (server.color_manager != NULL) {
		wlr_scene_set_color_manager_v1(wlr_scene, server.color_manager);
	}

	struct wlr_scene_tree *interactive_tree = wlr_scene_tree_create(&wlr_scene->tree);
	if (interactive_tree == NULL) {
		goto error;
	}
	struct wlr_scene_tree *drag_icons = wlr_scene_tree_create(&wlr_scene->tree);
	if (drag_icons == NULL) {
		goto error;
	}
	struct wlr_scene_tree *hidden_tree = wlr_scene_tree_create(&wlr_scene->tree);
	if (hidden_tree == NULL) {
		goto error;
	}
	wlr_scene_node_set_enabled(&hidden_tree->node, false);

	struct wlr_scene_tree *normal_tree = wlr_scene_tree_create(interactive_tree);
	if (normal_tree == NULL) {
		goto error;
	}
	struct wlr_scene_tree *locked_tree = wlr_scene_tree_create(interactive_tree);
	if (locked_tree == NULL) {
		goto error;
	}
	wlr_scene_node_set_enabled(&locked_tree->node, false);

	struct rv_scene new = {
		.wlr_scene = wlr_scene,
		.interactive_tree = interactive_tree,
		.drag_icons = drag_icons,
		.hidden_tree = hidden_tree,
		.normal_tree = normal_tree,
		.locked_tree = locked_tree,
	};
	// Same creation order as the Zig struct literal.
	struct wlr_scene_tree **layers[] = {
		&new.layers.background,
		&new.layers.bottom,
		&new.layers.wm,
		&new.layers.top,
		&new.layers.fullscreen,
		&new.layers.overlay,
		&new.layers.popups,
#if WLR_HAS_XWAYLAND
		&new.layers.override_redirect,
#endif
	};
	for (size_t i = 0; i < sizeof(layers) / sizeof(layers[0]); i++) {
		*layers[i] = wlr_scene_tree_create(normal_tree);
		if (*layers[i] == NULL) {
			goto error;
		}
	}
	*scene = new;
	return true;

error:
	wlr_scene_node_destroy(&wlr_scene->tree.node);
	return false;
}

// Return information about what is currently rendered in the interactive_tree
// tree at the given layout coordinates, taking surface input regions into account.
bool rv_scene_at(const struct rv_scene *scene, double lx, double ly, struct rv_scene_at_result *result) {
	double sx;
	double sy;
	struct wlr_scene_node *node = wlr_scene_node_at(&scene->interactive_tree->node, lx, ly, &sx, &sy);
	if (node == NULL) {
		return false;
	}

	struct wlr_surface *surface = NULL;
	if (node->type == WLR_SCENE_NODE_BUFFER) {
		struct wlr_scene_buffer *scene_buffer = wlr_scene_buffer_from_node(node);
		struct wlr_scene_surface *scene_surface = wlr_scene_surface_try_from_buffer(scene_buffer);
		if (scene_surface != NULL) {
			surface = scene_surface->surface;
		}
	}

	struct rv_scene_node_data *scene_node_data = rv_scene_node_data_from_node(node);
	if (scene_node_data != NULL) {
		*result = (struct rv_scene_at_result){
			.node = node,
			.surface = surface,
			.sx = sx,
			.sy = sy,
			.data = scene_node_data->data,
		};
		return true;
	} else {
		return false;
	}
}

struct wlr_scene_tree *rv_scene_layer_surface_tree(struct rv_scene *scene, enum zwlr_layer_shell_v1_layer layer) {
	switch (layer) {
	case ZWLR_LAYER_SHELL_V1_LAYER_BACKGROUND:
		return scene->layers.background;
	case ZWLR_LAYER_SHELL_V1_LAYER_BOTTOM:
		return scene->layers.bottom;
	case ZWLR_LAYER_SHELL_V1_LAYER_TOP:
		return scene->layers.top;
	case ZWLR_LAYER_SHELL_V1_LAYER_OVERLAY:
		return scene->layers.overlay;
	}
	abort(); // unreachable
}

static void saveable_surfaces_sync_enabled(const struct rv_scene_saveable_surfaces *surfaces) {
	wlr_scene_node_set_enabled(&surfaces->tree->node, surfaces->enabled && !surfaces->saved);
	wlr_scene_node_set_enabled(&surfaces->saved_tree->node, surfaces->enabled && surfaces->saved);
}

bool rv_scene_saveable_surfaces_init(struct rv_scene_saveable_surfaces *surfaces, struct wlr_scene_tree *parent) {
	struct wlr_scene_tree *tree = wlr_scene_tree_create(parent);
	if (tree == NULL) {
		return false;
	}
	struct wlr_scene_tree *saved_tree = wlr_scene_tree_create(parent);
	if (saved_tree == NULL) {
		return false;
	}
	*surfaces = (struct rv_scene_saveable_surfaces){
		.enabled = true,
		.saved = false,
		.tree = tree,
		.saved_tree = saved_tree,
	};
	saveable_surfaces_sync_enabled(surfaces);
	return true;
}

void rv_scene_saveable_surfaces_set_enabled(struct rv_scene_saveable_surfaces *surfaces, bool enabled) {
	if (enabled == surfaces->enabled) {
		return;
	}
	surfaces->enabled = enabled;
	saveable_surfaces_sync_enabled(surfaces);
}

static void save_surface_tree_iter(struct wlr_scene_buffer *buffer, int sx, int sy, void *data) {
	struct wlr_scene_tree *saved_tree = data;
	struct wlr_scene_buffer *scene_buffer = wlr_scene_buffer_create(saved_tree, buffer->buffer);
	if (scene_buffer == NULL) {
		rv_err(RV_SCOPE_DEFAULT, "out of memory");
		return;
	}
	wlr_scene_node_set_position(&scene_buffer->node, sx, sy);
	wlr_scene_buffer_set_dest_size(scene_buffer, buffer->dst_width, buffer->dst_height);
	wlr_scene_buffer_set_source_box(scene_buffer, &buffer->src_box);
	wlr_scene_buffer_set_transform(scene_buffer, buffer->transform);
}

void rv_scene_saveable_surfaces_save(struct rv_scene_saveable_surfaces *surfaces) {
	if (surfaces->saved) {
		return;
	}
	assert(surfaces->tree->node.enabled == surfaces->enabled);
	assert(!surfaces->saved_tree->node.enabled);
	assert(wl_list_empty(&surfaces->saved_tree->children));
	wlr_scene_node_for_each_buffer(&surfaces->tree->node, save_surface_tree_iter, surfaces->saved_tree);
	surfaces->saved = true;
	saveable_surfaces_sync_enabled(surfaces);
}

void rv_scene_saveable_surfaces_drop_saved(struct rv_scene_saveable_surfaces *surfaces) {
	if (!surfaces->saved) {
		return;
	}
	assert(!surfaces->tree->node.enabled);
	assert(surfaces->saved_tree->node.enabled == surfaces->enabled);
	{
		struct wlr_scene_node *node, *tmp;
		wl_list_for_each_safe(node, tmp, &surfaces->saved_tree->children, link) {
			wlr_scene_node_destroy(node);
		}
	}
	surfaces->saved = false;
	saveable_surfaces_sync_enabled(surfaces);
}
