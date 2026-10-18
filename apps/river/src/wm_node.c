// SPDX-FileCopyrightText: © 2024 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/WmNode.zig for Oath, 2026-10-18.

#include "river.h"

static const struct river_node_v1_interface node_impl;
static const struct river_node_v1_interface node_inert_impl;
static void handle_destroy(struct wl_resource *node_v1);

void rv_wm_node_init(struct rv_wm_node *node, enum rv_wm_node_tag tag) {
	*node = (struct rv_wm_node){
		.tag = tag,
		.object = NULL,
	};
	wl_list_init(&node->link);
}

void rv_wm_node_deinit(struct rv_wm_node *node) {
	assert(node->object == NULL);

	wl_list_remove(&node->link);
}

struct rv_wm_node_type rv_wm_node_get(struct rv_wm_node *node) {
	switch (node->tag) {
	case RV_WM_NODE_TAG_WINDOW: {
		struct rv_window *window = wl_container_of(node, window, node);
		return (struct rv_wm_node_type){ .tag = RV_WM_NODE_TAG_WINDOW, .window = window };
	}
	case RV_WM_NODE_TAG_SHELL_SURFACE: {
		struct rv_shell_surface *shell_surface = wl_container_of(node, shell_surface, node);
		return (struct rv_wm_node_type){ .tag = RV_WM_NODE_TAG_SHELL_SURFACE, .shell_surface = shell_surface };
	}
	}
	abort(); // unreachable
}

void rv_wm_node_create_object(struct rv_wm_node *node, struct wl_client *client, uint32_t version, uint32_t id) {
	assert(node->object == NULL);
	struct wl_resource *node_v1 = wl_resource_create(client, &river_node_v1_interface, (int)version, id);
	if (node_v1 == NULL) {
		rv_err(RV_SCOPE_DEFAULT, "out of memory");
		wl_client_post_no_memory(client);
		return;
	}
	wl_resource_set_implementation(node_v1, &node_impl, node, handle_destroy);
	node->object = node_v1;
}

void rv_wm_node_make_inert(struct rv_wm_node *node) {
	if (node->object != NULL) {
		wl_resource_set_implementation(node->object, &node_inert_impl, NULL, NULL);
		node->object = NULL;
	}
}

// handleRequestInert: only destroy does anything.

static void inert_destroy(struct wl_client *client, struct wl_resource *node_v1) {
	wl_resource_destroy(node_v1);
}

static void inert_set_position(struct wl_client *client, struct wl_resource *node_v1, int32_t x, int32_t y) {
}

static void inert_place_top(struct wl_client *client, struct wl_resource *node_v1) {
}

static void inert_place_bottom(struct wl_client *client, struct wl_resource *node_v1) {
}

static void inert_place_above(struct wl_client *client, struct wl_resource *node_v1, struct wl_resource *other) {
}

static void inert_place_below(struct wl_client *client, struct wl_resource *node_v1, struct wl_resource *other) {
}

static const struct river_node_v1_interface node_inert_impl = {
	.destroy = inert_destroy,
	.set_position = inert_set_position,
	.place_top = inert_place_top,
	.place_bottom = inert_place_bottom,
	.place_above = inert_place_above,
	.place_below = inert_place_below,
};

static void handle_destroy(struct wl_resource *node_v1) {
	struct rv_wm_node *node = wl_resource_get_user_data(node_v1);
	node->object = NULL;
}

// handleRequest: one function per request.

static struct rv_wm_node *node_from_resource(struct wl_resource *node_v1) {
	struct rv_wm_node *node = wl_resource_get_user_data(node_v1);
	assert(node->object == node_v1);
	return node;
}

static void request_destroy(struct wl_client *client, struct wl_resource *node_v1) {
	node_from_resource(node_v1);
	wl_resource_destroy(node_v1);
}

static void request_set_position(struct wl_client *client, struct wl_resource *node_v1, int32_t x, int32_t y) {
	struct rv_wm_node *node = node_from_resource(node_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}
	struct rv_wm_node_type type = rv_wm_node_get(node);
	switch (type.tag) {
	case RV_WM_NODE_TAG_WINDOW:
		type.window->rendering_requested.x = x;
		type.window->rendering_requested.y = y;
		break;
	case RV_WM_NODE_TAG_SHELL_SURFACE:
		type.shell_surface->rendering_requested.x = x;
		type.shell_surface->rendering_requested.y = y;
		break;
	}
}

static void request_place_top(struct wl_client *client, struct wl_resource *node_v1) {
	struct rv_wm_node *node = node_from_resource(node_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}
	wl_list_remove(&node->link);
	wl_list_insert(server.wm.rendering_requested.list.prev, &node->link);
}

static void request_place_bottom(struct wl_client *client, struct wl_resource *node_v1) {
	struct rv_wm_node *node = node_from_resource(node_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}
	wl_list_remove(&node->link);
	wl_list_insert(&server.wm.rendering_requested.list, &node->link);
}

static void request_place_above(struct wl_client *client, struct wl_resource *node_v1, struct wl_resource *other_v1) {
	struct rv_wm_node *node = node_from_resource(node_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}

	void *other_data = wl_resource_get_user_data(other_v1);
	if (other_data == NULL) {
		return;
	}
	struct rv_wm_node *other = other_data;

	if (other == node) {
		return;
	}

	wl_list_remove(&node->link);
	wl_list_insert(&other->link, &node->link);
}

static void request_place_below(struct wl_client *client, struct wl_resource *node_v1, struct wl_resource *other_v1) {
	struct rv_wm_node *node = node_from_resource(node_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}

	void *other_data = wl_resource_get_user_data(other_v1);
	if (other_data == NULL) {
		return;
	}
	struct rv_wm_node *other = other_data;

	if (other == node) {
		return;
	}

	wl_list_remove(&node->link);
	wl_list_insert(other->link.prev, &node->link);
}

static const struct river_node_v1_interface node_impl = {
	.destroy = request_destroy,
	.set_position = request_set_position,
	.place_top = request_place_top,
	.place_bottom = request_place_bottom,
	.place_above = request_place_above,
	.place_below = request_place_below,
};
