// SPDX-FileCopyrightText: © 2025 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/LayerShellOutput.zig for Oath, 2026-10-18.

#include "rivulet.h"

// LayerShellOutput.sendConfigures() `mode: enum { exclusive, non_exclusive }`
enum send_configures_mode {
	SEND_CONFIGURES_MODE_EXCLUSIVE,
	SEND_CONFIGURES_MODE_NON_EXCLUSIVE,
};

// std.meta.eql on two wlr.Box values
static bool box_eql(struct wlr_box a, struct wlr_box b) {
	return a.x == b.x && a.y == b.y && a.width == b.width && a.height == b.height;
}

// std.meta.eql(sent.non_exclusive_area (?wlr.Box), scheduled.non_exclusive_area):
// the scheduled box coerces to a non-null ?wlr.Box, so this is false while sent is null.
static bool sent_eql_scheduled(const struct rv_layer_shell_output *shell_output) {
	return shell_output->sent.has_non_exclusive_area &&
		box_eql(shell_output->sent.non_exclusive_area, shell_output->scheduled.non_exclusive_area);
}

static void request_destroy(struct wl_client *client, struct wl_resource *layer_shell_output_v1);
static void request_set_default(struct wl_client *client, struct wl_resource *layer_shell_output_v1);

static const struct river_layer_shell_output_v1_interface layer_shell_output_impl = {
	.destroy = request_destroy,
	.set_default = request_set_default,
};

static void request_destroy_inert(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

static void request_set_default_inert(struct wl_client *client, struct wl_resource *object) {
}

// handleRequestInert
static const struct river_layer_shell_output_v1_interface layer_shell_output_inert_impl = {
	.destroy = request_destroy_inert,
	.set_default = request_set_default_inert,
};

// Zig handleDestroy(_: *river.LayerShellOutputV1, shell_output: *LayerShellOutput),
// also called directly by makeInert().
static void handle_destroy_data(struct rv_layer_shell_output *shell_output) {
	shell_output->object = NULL;
	shell_output->sent = (struct rv_layer_shell_output_sent){ .has_non_exclusive_area = false };
	shell_output->requested = (struct rv_layer_shell_output_requested){ .default_ = false };
}

// The resource destructor installed with layer_shell_output_impl.
static void handle_destroy(struct wl_resource *object) {
	struct rv_layer_shell_output *shell_output = wl_resource_get_user_data(object);
	handle_destroy_data(shell_output);
}

void rv_layer_shell_output_create_object(struct rv_layer_shell_output *shell_output, struct wl_client *client,
		uint32_t version, uint32_t id) {
	assert(shell_output->object == NULL);
	shell_output->object = wl_resource_create(client, &river_layer_shell_output_v1_interface, (int)version, id);
	if (shell_output->object == NULL) {
		wl_client_post_no_memory(client);
		rv_err(RV_SCOPE_WM, "out of memory");
		return;
	}
	wl_resource_set_implementation(shell_output->object, &layer_shell_output_impl, shell_output, handle_destroy);
	rv_window_manager_dirty_windowing(&server.wm);
}

void rv_layer_shell_output_make_inert(struct rv_layer_shell_output *shell_output) {
	struct wl_resource *object = shell_output->object;
	if (object != NULL) {
		wl_resource_set_implementation(object, &layer_shell_output_inert_impl, NULL, NULL);
		handle_destroy_data(shell_output);
	}
}

static void request_destroy(struct wl_client *client, struct wl_resource *layer_shell_output_v1) {
	struct rv_layer_shell_output *shell_output = wl_resource_get_user_data(layer_shell_output_v1);
	assert(shell_output->object == layer_shell_output_v1);
	wl_resource_destroy(layer_shell_output_v1);
}

static void request_set_default(struct wl_client *client, struct wl_resource *layer_shell_output_v1) {
	struct rv_layer_shell_output *shell_output = wl_resource_get_user_data(layer_shell_output_v1);
	assert(shell_output->object == layer_shell_output_v1);
	struct rv_output *output, *tmp;
	wl_list_for_each_safe(output, tmp, &server.om.outputs, link) {
		output->layer_shell.requested.default_ = false;
	}
	shell_output->requested.default_ = true;
}

static void send_configures(struct rv_output *output, enum send_configures_mode mode);

void rv_layer_shell_output_arrange(struct rv_layer_shell_output *shell_output) {
	struct rv_output *output = wl_container_of(shell_output, output, layer_shell);
	shell_output->scheduled.non_exclusive_area = rv_output_state_box(&output->scheduled);
	send_configures(output, SEND_CONFIGURES_MODE_EXCLUSIVE);
	send_configures(output, SEND_CONFIGURES_MODE_NON_EXCLUSIVE);
	if (!sent_eql_scheduled(shell_output)) {
		rv_window_manager_dirty_windowing(&server.wm);
	}
}

static void send_configures(struct rv_output *output, enum send_configures_mode mode) {
	int32_t output_width, output_height;
	rv_output_state_dimensions(&output->scheduled, &output_width, &output_height);
	const struct wlr_box output_box = rv_output_state_box(&output->scheduled);
	const enum zwlr_layer_shell_v1_layer layers[] = {
		ZWLR_LAYER_SHELL_V1_LAYER_BACKGROUND,
		ZWLR_LAYER_SHELL_V1_LAYER_BOTTOM,
		ZWLR_LAYER_SHELL_V1_LAYER_TOP,
		ZWLR_LAYER_SHELL_V1_LAYER_OVERLAY,
	};
	for (size_t i = 0; i < sizeof(layers) / sizeof(layers[0]); i++) {
		struct wlr_scene_tree *tree = rv_scene_layer_surface_tree(&server.scene, layers[i]);
		struct wlr_scene_node *node, *tmp;
		wl_list_for_each_safe(node, tmp, &tree->children, link) {
			assert(node->type == WLR_SCENE_NODE_TREE);
			struct rv_scene_node_data *node_data = node->data;
			if (node_data == NULL) {
				continue;
			}
			assert(node_data->data.tag == RV_SCENE_NODE_DATA_DATA_LAYER_SURFACE);
			struct rv_layer_surface *layer_surface = node_data->data.layer_surface;
			if (!layer_surface->wlr_layer_surface->surface->mapped &&
					!layer_surface->wlr_layer_surface->initial_commit) {
				continue;
			}
			if (layer_surface->wlr_layer_surface->output != output->wlr_output) {
				continue;
			}
			const struct wlr_layer_surface_v1_state current = layer_surface->wlr_layer_surface->current;
			const bool exclusive = current.exclusive_zone > 0;
			if (exclusive != (mode == SEND_CONFIGURES_MODE_EXCLUSIVE)) {
				continue;
			}
			{
				struct wlr_box new_area = output->layer_shell.scheduled.non_exclusive_area;
				wlr_scene_layer_surface_v1_configure(layer_surface->scene_layer_surface, &output_box, &new_area);
				// Clients can request bogus exclusive zones larger than the output
				// dimensions and river must handle this gracefully. It seems reasonable
				// to close layer shell clients that would cause the usable area of the
				// output to become less than half the width/height of its full dimensions.
				if (new_area.width < output_width / 2 || new_area.height < output_height / 2) {
					wlr_layer_surface_v1_destroy(layer_surface->wlr_layer_surface);
					continue;
				}
				output->layer_shell.scheduled.non_exclusive_area = new_area;
			}
			const int x = layer_surface->scene_layer_surface->tree->node.x;
			const int y = layer_surface->scene_layer_surface->tree->node.y;
			wlr_scene_node_set_position(&layer_surface->popup_tree->node, x, y);
			wlr_scene_subsurface_tree_set_clip(&layer_surface->scene_layer_surface->tree->node,
				&(struct wlr_box){
					.x = -(x - output->scheduled.x),
					.y = -(y - output->scheduled.y),
					.width = output_width,
					.height = output_height,
				});
		}
	}
}

void rv_layer_shell_output_manage_start(struct rv_layer_shell_output *shell_output) {
	struct rv_output *output = wl_container_of(shell_output, output, layer_shell);
	assert(output->scheduled.state == RV_OUTPUT_STATE_STATE_ENABLED ||
		output->scheduled.state == RV_OUTPUT_STATE_STATE_DISABLED_SOFT);

	const struct wlr_box scheduled_box = rv_output_state_box(&output->scheduled);
	const struct wlr_box sent_box = rv_output_state_box(&output->sent);

	if (!box_eql(scheduled_box, sent_box)) {
		shell_output->scheduled.non_exclusive_area = scheduled_box;
		send_configures(output, SEND_CONFIGURES_MODE_EXCLUSIVE);
		send_configures(output, SEND_CONFIGURES_MODE_NON_EXCLUSIVE);
	}
	if (!sent_eql_scheduled(shell_output)) {
		struct wl_resource *layer_shell_output_v1 = shell_output->object;
		if (layer_shell_output_v1 != NULL) {
			river_layer_shell_output_v1_send_non_exclusive_area(layer_shell_output_v1,
				shell_output->scheduled.non_exclusive_area.x,
				shell_output->scheduled.non_exclusive_area.y,
				shell_output->scheduled.non_exclusive_area.width,
				shell_output->scheduled.non_exclusive_area.height);
			shell_output->sent.has_non_exclusive_area = true;
			shell_output->sent.non_exclusive_area = shell_output->scheduled.non_exclusive_area;
		}
	}
}
