// SPDX-FileCopyrightText: © 2024 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/ShellSurface.zig for Oath, 2026-10-18.

#include "river.h"

static void client_commit(struct wlr_surface *wlr_surface);
static void commit(struct wlr_surface *wlr_surface);
static void role_destroy(struct wlr_surface *wlr_surface);

static const struct wlr_surface_role role = {
	.name = "river_shell_surface_v1",
	.client_commit = client_commit,
	.commit = commit,
	.unmap = NULL,
	.destroy = role_destroy,
};

static const struct river_shell_surface_v1_interface shell_surface_impl;

bool rv_shell_surface_create(struct wl_client *client, uint32_t version, uint32_t id, struct wlr_surface *surface) {
	rv_debug(RV_SCOPE_WM, "new river_shell_surface_v1");

	struct wl_resource *shell_surface_v1 =
		wl_resource_create(client, &river_shell_surface_v1_interface, (int)version, id);
	if (shell_surface_v1 == NULL) {
		return false;
	}

	if (!wlr_surface_set_role(surface, &role, shell_surface_v1, RIVER_WINDOW_MANAGER_V1_ERROR_ROLE)) {
		return true;
	}
	wlr_surface_set_role_object(surface, shell_surface_v1);

	struct rv_shell_surface *shell_surface = calloc(1, sizeof(*shell_surface));
	if (shell_surface == NULL) {
		return false;
	}

	struct wlr_scene_tree *tree = wlr_scene_tree_create(server.scene.hidden_tree);
	if (tree == NULL) {
		goto error_tree;
	}

	struct wlr_scene_tree *popup_tree = wlr_scene_tree_create(server.scene.hidden_tree);
	if (popup_tree == NULL) {
		goto error_popup_tree;
	}

	struct rv_scene_saveable_surfaces surfaces;
	if (!rv_scene_saveable_surfaces_init(&surfaces, tree)) {
		goto error;
	}
	if (wlr_scene_subsurface_tree_create(surfaces.tree, surface) == NULL) {
		goto error;
	}

	if (!rv_scene_node_data_attach(&tree->node, (struct rv_scene_node_data_data){
			.tag = RV_SCENE_NODE_DATA_DATA_SHELL_SURFACE,
			.shell_surface = shell_surface,
		})) {
		goto error;
	}
	if (!rv_scene_node_data_attach(&popup_tree->node, (struct rv_scene_node_data_data){
			.tag = RV_SCENE_NODE_DATA_DATA_SHELL_SURFACE,
			.shell_surface = shell_surface,
		})) {
		goto error;
	}

	*shell_surface = (struct rv_shell_surface){
		.object = shell_surface_v1,
		.surface = surface,
		.tree = tree,
		.surfaces = surfaces,
		.popup_tree = popup_tree,
	};
	rv_wm_node_init(&shell_surface->node, RV_WM_NODE_TAG_SHELL_SURFACE);
	wl_list_insert(server.wm.rendering_requested.list.prev, &shell_surface->node.link);

	wl_resource_set_implementation(shell_surface_v1, &shell_surface_impl, shell_surface, NULL);
	return true;

error:
	wlr_scene_node_destroy(&popup_tree->node);
error_popup_tree:
	wlr_scene_node_destroy(&tree->node);
error_tree:
	free(shell_surface);
	return false;
}

static struct rv_shell_surface *from_wlr_surface(struct wlr_surface *wlr_surface) {
	if (wlr_surface->role != &role) {
		return NULL;
	}
	struct wl_resource *resource = wlr_surface->role_resource;
	if (resource == NULL) {
		return NULL;
	}
	return wl_resource_get_user_data(resource);
}

static void role_destroy(struct wlr_surface *wlr_surface) {
	struct rv_shell_surface *shell_surface = from_wlr_surface(wlr_surface);
	if (shell_surface == NULL) {
		return;
	}

	wlr_surface_unmap(shell_surface->surface);

	rv_wm_node_make_inert(&shell_surface->node);
	rv_wm_node_deinit(&shell_surface->node);

	wlr_scene_node_destroy(&shell_surface->tree->node);
	wlr_scene_node_destroy(&shell_surface->popup_tree->node);

	free(shell_surface);
}

// handleRequest: one function per request.

static struct rv_shell_surface *shell_surface_from_resource(struct wl_resource *shell_surface_v1) {
	struct rv_shell_surface *shell_surface = wl_resource_get_user_data(shell_surface_v1);
	assert(shell_surface->object == shell_surface_v1);
	return shell_surface;
}

static void request_destroy(struct wl_client *client, struct wl_resource *shell_surface_v1) {
	shell_surface_from_resource(shell_surface_v1);
	wl_resource_destroy(shell_surface_v1);
}

static void request_get_node(struct wl_client *client, struct wl_resource *shell_surface_v1, uint32_t id) {
	struct rv_shell_surface *shell_surface = shell_surface_from_resource(shell_surface_v1);
	if (shell_surface->node.object != NULL) {
		wl_resource_post_error(shell_surface_v1, RIVER_SHELL_SURFACE_V1_ERROR_NODE_EXISTS, "%s",
			"shell surface already has a node object");
		return;
	}
	rv_wm_node_create_object(&shell_surface->node, wl_resource_get_client(shell_surface_v1),
		(uint32_t)wl_resource_get_version(shell_surface_v1), id);
}

static void request_sync_next_commit(struct wl_client *client, struct wl_resource *shell_surface_v1) {
	struct rv_shell_surface *shell_surface = shell_surface_from_resource(shell_surface_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}
	shell_surface->rendering_requested.sync_next_commit = true;
}

static const struct river_shell_surface_v1_interface shell_surface_impl = {
	.destroy = request_destroy,
	.get_node = request_get_node,
	.sync_next_commit = request_sync_next_commit,
};

static void client_commit(struct wlr_surface *wlr_surface) {
	struct rv_shell_surface *shell_surface = from_wlr_surface(wlr_surface);
	if (shell_surface == NULL) {
		return;
	}
	if (shell_surface->rendering_requested.sync_next_commit) {
		rv_scene_saveable_surfaces_save(&shell_surface->surfaces);
	}
}

static void commit(struct wlr_surface *wlr_surface) {
	if (wlr_surface_has_buffer(wlr_surface)) {
		wlr_surface_map(wlr_surface);
	}
}

void rv_shell_surface_render_finish(struct rv_shell_surface *shell_surface) {
	struct rv_shell_surface_rendering_requested *rendering_requested = &shell_surface->rendering_requested;
	if (rendering_requested->sync_next_commit) {
		rendering_requested->sync_next_commit = false;

		if (!shell_surface->surfaces.saved) {
			wl_resource_post_error(shell_surface->object, RIVER_SHELL_SURFACE_V1_ERROR_NO_COMMIT, "%s",
				"no wl_surface.commit after sync_next_commit and before update_rendering_finish");
		}
	}

	rv_scene_saveable_surfaces_drop_saved(&shell_surface->surfaces);

	wlr_scene_node_set_position(&shell_surface->tree->node, rendering_requested->x, rendering_requested->y);
	wlr_scene_node_set_position(&shell_surface->popup_tree->node, rendering_requested->x, rendering_requested->y);
}
