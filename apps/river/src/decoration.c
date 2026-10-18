// SPDX-FileCopyrightText: © 2025 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Decoration.zig for Oath, 2026-10-18.

#include "river.h"

static void client_commit(struct wlr_surface *wlr_surface);
static void commit(struct wlr_surface *wlr_surface);

static const struct wlr_surface_role role = {
	.name = "river_decoration_v1",
	.client_commit = client_commit,
	.commit = commit,
	.unmap = NULL,
	.destroy = NULL,
};

static const struct river_decoration_v1_interface decoration_impl;
static const struct river_decoration_v1_interface decoration_inert_impl;
static void handle_destroy(struct wl_resource *decoration_v1);

struct rv_decoration *rv_decoration_create(struct wl_client *client, uint32_t version, uint32_t id,
		struct wlr_surface *surface, struct wlr_scene_tree *parent, bool *already_has_role) {
	*already_has_role = false;

	struct wl_resource *decoration_v1 = wl_resource_create(client, &river_decoration_v1_interface, (int)version, id);
	if (decoration_v1 == NULL) {
		return NULL;
	}

	if (!wlr_surface_set_role(surface, &role, decoration_v1, RIVER_WINDOW_MANAGER_V1_ERROR_ROLE)) {
		*already_has_role = true;
		return NULL;
	}
	wlr_surface_set_role_object(surface, decoration_v1);

	struct rv_decoration *decoration = calloc(1, sizeof(*decoration));
	if (decoration == NULL) {
		return NULL;
	}

	struct wlr_scene_tree *tree = wlr_scene_tree_create(parent);
	if (tree == NULL) {
		goto error_tree;
	}

	struct rv_scene_saveable_surfaces surfaces;
	if (!rv_scene_saveable_surfaces_init(&surfaces, tree)) {
		goto error;
	}
	if (wlr_scene_subsurface_tree_create(surfaces.tree, surface) == NULL) {
		goto error;
	}

	*decoration = (struct rv_decoration){
		.object = decoration_v1,
		.surface = surface,
		.tree = tree,
		.surfaces = surfaces,
	};
	// Zig leaves link undefined; the caller appends it to
	// Window.decorations_above/below right away.
	wl_list_init(&decoration->link);

	wl_resource_set_implementation(decoration_v1, &decoration_impl, decoration, handle_destroy);

	return decoration;

error:
	wlr_scene_node_destroy(&tree->node);
error_tree:
	free(decoration);
	return NULL;
}

void rv_decoration_destroy(struct rv_decoration *decoration) {
	assert(decoration->object == NULL);
	wlr_scene_node_destroy(&decoration->tree->node);
	wl_list_remove(&decoration->link);
	free(decoration);
}

void rv_decoration_make_inert(struct rv_decoration *decoration) {
	if (decoration->object != NULL) {
		wl_resource_set_implementation(decoration->object, &decoration_inert_impl, NULL, NULL);
		decoration->object = NULL;
	}
	rv_scene_saveable_surfaces_save(&decoration->surfaces);
}

// handleRequestInert: only destroy does anything.

static void inert_destroy(struct wl_client *client, struct wl_resource *decoration_v1) {
	wl_resource_destroy(decoration_v1);
}

static void inert_set_offset(struct wl_client *client, struct wl_resource *decoration_v1, int32_t x, int32_t y) {
}

static void inert_sync_next_commit(struct wl_client *client, struct wl_resource *decoration_v1) {
}

static const struct river_decoration_v1_interface decoration_inert_impl = {
	.destroy = inert_destroy,
	.set_offset = inert_set_offset,
	.sync_next_commit = inert_sync_next_commit,
};

static void handle_destroy(struct wl_resource *decoration_v1) {
	struct rv_decoration *decoration = wl_resource_get_user_data(decoration_v1);
	decoration->object = NULL;
	rv_decoration_destroy(decoration);
}

// handleRequest: one function per request.

static struct rv_decoration *decoration_from_resource(struct wl_resource *decoration_v1) {
	struct rv_decoration *decoration = wl_resource_get_user_data(decoration_v1);
	assert(decoration->object == decoration_v1);
	return decoration;
}

static void request_destroy(struct wl_client *client, struct wl_resource *decoration_v1) {
	decoration_from_resource(decoration_v1);
	wl_resource_destroy(decoration_v1);
}

static void request_set_offset(struct wl_client *client, struct wl_resource *decoration_v1, int32_t x, int32_t y) {
	struct rv_decoration *decoration = decoration_from_resource(decoration_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}
	decoration->rendering_requested.offset_x = x;
	decoration->rendering_requested.offset_y = y;
}

static void request_sync_next_commit(struct wl_client *client, struct wl_resource *decoration_v1) {
	struct rv_decoration *decoration = decoration_from_resource(decoration_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}
	decoration->rendering_requested.sync_next_commit = true;
}

static const struct river_decoration_v1_interface decoration_impl = {
	.destroy = request_destroy,
	.set_offset = request_set_offset,
	.sync_next_commit = request_sync_next_commit,
};

static void client_commit(struct wlr_surface *wlr_surface) {
	if (wlr_surface->role != &role) {
		return;
	}
	struct wl_resource *resource = wlr_surface->role_resource;
	if (resource == NULL) {
		return;
	}
	struct rv_decoration *decoration = wl_resource_get_user_data(resource);
	// Zig casts the user data to a non-optional *Decoration here, which is a
	// safety-checked panic if the decoration object was made inert.
	assert(decoration != NULL);
	if (decoration->rendering_requested.sync_next_commit) {
		rv_scene_saveable_surfaces_save(&decoration->surfaces);
	}
}

static void commit(struct wlr_surface *wlr_surface) {
	if (wlr_surface_has_buffer(wlr_surface)) {
		wlr_surface_map(wlr_surface);
	}
}

void rv_decoration_render_finish(struct rv_decoration *decoration, const struct wlr_box *window_clip) {
	struct rv_decoration_rendering_requested *rendering_requested = &decoration->rendering_requested;
	if (rendering_requested->sync_next_commit) {
		rendering_requested->sync_next_commit = false;

		if (!decoration->surfaces.saved) {
			if (decoration->object != NULL) {
				wl_resource_post_error(decoration->object, RIVER_DECORATION_V1_ERROR_NO_COMMIT, "%s",
					"no wl_surface.commit after sync_next_commit and before update_rendering_finish");
			}
		}
	}

	rv_scene_saveable_surfaces_drop_saved(&decoration->surfaces);

	wlr_scene_node_set_position(&decoration->tree->node, rendering_requested->offset_x, rendering_requested->offset_y);

	// wlroots asserts that a subsurface tree is present.
	if (!wl_list_empty(&decoration->surfaces.tree->children)) {
		struct wlr_box clip = *window_clip;
		clip.x -= rendering_requested->offset_x;
		clip.y -= rendering_requested->offset_y;
		wlr_scene_subsurface_tree_set_clip(&decoration->surfaces.tree->node, &clip);
	}
}
