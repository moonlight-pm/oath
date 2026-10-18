// SPDX-FileCopyrightText: © 2025 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/LayerShell.zig for Oath, 2026-10-18.

#include "river.h"

static void bind(struct wl_client *client, void *data, uint32_t version, uint32_t id);
static void handle_new_surface(struct wl_listener *listener, void *data);

bool rv_layer_shell_init(struct rv_layer_shell *layer_shell) {
	struct wl_global *global =
		wl_global_create(server.wl_server, &river_layer_shell_v1_interface, 1, layer_shell, bind);
	if (global == NULL) {
		return false;
	}
	struct wlr_layer_shell_v1 *wlr_shell = wlr_layer_shell_v1_create(server.wl_server, 4);
	if (wlr_shell == NULL) {
		return false;
	}

	layer_shell->global = global;
	layer_shell->wlr_shell = wlr_shell;
	layer_shell->surfaces = rv_slotmap_empty;
	layer_shell->new_surface.notify = handle_new_surface;

	wl_list_init(&layer_shell->objects);
	wl_signal_add(&layer_shell->wlr_shell->events.new_surface, &layer_shell->new_surface);
	return true;
}

// Use a deinit function rather than listening for the wl_server to be destroyed
// in order to avoid a signal ordering issue. The wlr.LayerShellV1 also listens
// for the wl_server to be destroyed and asserts that the new_surface event has
// no remaining listeners.
void rv_layer_shell_deinit(struct rv_layer_shell *layer_shell) {
	wl_global_destroy(layer_shell->global);
	wl_list_remove(&layer_shell->new_surface.link);
}

static void request_destroy(struct wl_client *client, struct wl_resource *object);
static void request_get_output(struct wl_client *client, struct wl_resource *object, uint32_t id,
	struct wl_resource *output_v1);
static void request_get_seat(struct wl_client *client, struct wl_resource *object, uint32_t id,
	struct wl_resource *seat_v1);

static const struct river_layer_shell_v1_interface layer_shell_impl = {
	.destroy = request_destroy,
	.get_output = request_get_output,
	.get_seat = request_get_seat,
};

static void handle_destroy(struct wl_resource *object) {
	wl_list_remove(wl_resource_get_link(object));
}

static void bind(struct wl_client *client, void *data, uint32_t version, uint32_t id) {
	struct rv_layer_shell *layer_shell = data;
	struct wl_resource *object = wl_resource_create(client, &river_layer_shell_v1_interface, (int)version, id);
	if (object == NULL) {
		wl_client_post_no_memory(client);
		rv_err(RV_SCOPE_WM, "out of memory");
		return;
	}
	wl_resource_set_implementation(object, &layer_shell_impl, NULL, handle_destroy);
	wl_list_insert(layer_shell->objects.prev, wl_resource_get_link(object));
}

static void request_destroy(struct wl_client *client, struct wl_resource *object) {
	wl_resource_destroy(object);
}

static void request_get_output(struct wl_client *client, struct wl_resource *object, uint32_t id,
		struct wl_resource *output_v1) {
	void *output_data = wl_resource_get_user_data(output_v1);
	if (output_data == NULL) {
		return;
	}
	struct rv_output *output = output_data;
	if (output->layer_shell.object != NULL) {
		wl_resource_post_error(object, RIVER_LAYER_SHELL_V1_ERROR_OBJECT_ALREADY_CREATED, "%s",
			"river_layer_shell_output_v1 already created");
		return;
	}
	rv_layer_shell_output_create_object(&output->layer_shell, wl_resource_get_client(object),
		(uint32_t)wl_resource_get_version(object), id);
}

static void request_get_seat(struct wl_client *client, struct wl_resource *object, uint32_t id,
		struct wl_resource *seat_v1) {
	void *seat_data = wl_resource_get_user_data(seat_v1);
	if (seat_data == NULL) {
		return;
	}
	struct rv_seat *seat = seat_data;
	if (seat->layer_shell.object != NULL) {
		wl_resource_post_error(object, RIVER_LAYER_SHELL_V1_ERROR_OBJECT_ALREADY_CREATED, "%s",
			"river_layer_shell_seat_v1 already created");
		return;
	}
	rv_layer_shell_seat_create_object(&seat->layer_shell, wl_resource_get_client(object),
		(uint32_t)wl_resource_get_version(object), id);
}

static bool supported(struct rv_layer_shell *layer_shell) {
	struct wl_resource *wm_v1 = server.wm.object;
	if (wm_v1 == NULL) {
		return false;
	}
	struct wl_resource *object, *tmp;
	wl_resource_for_each_safe(object, tmp, &layer_shell->objects) {
		if (wl_resource_get_client(object) == wl_resource_get_client(wm_v1)) {
			return true;
		}
	}
	return false;
}

// @tagName(zwlr.LayerShellV1.Layer)
static const char *layer_name(enum zwlr_layer_shell_v1_layer layer) {
	switch (layer) {
	case ZWLR_LAYER_SHELL_V1_LAYER_BACKGROUND:
		return "background";
	case ZWLR_LAYER_SHELL_V1_LAYER_BOTTOM:
		return "bottom";
	case ZWLR_LAYER_SHELL_V1_LAYER_TOP:
		return "top";
	case ZWLR_LAYER_SHELL_V1_LAYER_OVERLAY:
		return "overlay";
	}
	abort(); // @tagName of an unnamed enum value
}

// Zig {b:0>4}: binary, padded on the left with '0' to a width of at least 4.
static const char *format_binary_0_4(char buf[static 33], uint32_t value) {
	char digits[32];
	int n = 0;
	do {
		digits[n++] = (char)('0' + (value & 1));
		value >>= 1;
	} while (value != 0);
	int len = 0;
	for (int pad = 4 - n; pad > 0; pad--) {
		buf[len++] = '0';
	}
	while (n > 0) {
		buf[len++] = digits[--n];
	}
	buf[len] = '\0';
	return buf;
}

static void handle_new_surface(struct wl_listener *listener, void *data) {
	struct wlr_layer_surface_v1 *wlr_layer_surface = data;

	char anchor[33];
	rv_debug(RV_SCOPE_WM,
		"new layer surface: namespace %s, layer %s, anchor %s, size %u,%u, margin %d,%d,%d,%d, exclusive_zone %d",
		wlr_layer_surface->namespace,
		layer_name(wlr_layer_surface->current.layer),
		format_binary_0_4(anchor, wlr_layer_surface->current.anchor),
		wlr_layer_surface->current.desired_width,
		wlr_layer_surface->current.desired_height,
		wlr_layer_surface->current.margin.top,
		wlr_layer_surface->current.margin.right,
		wlr_layer_surface->current.margin.bottom,
		wlr_layer_surface->current.margin.left,
		wlr_layer_surface->current.exclusive_zone);

	if (!supported(&server.layer_shell)) {
		rv_info(RV_SCOPE_WM, "window manager did not bind river_layer_shell_v1, closing layer surface");
		wlr_layer_surface_v1_destroy(wlr_layer_surface);
		return;
	}

	if (wlr_layer_surface->output == NULL) {
		bool found = false;
		struct rv_output *output, *tmp;
		wl_list_for_each_safe(output, tmp, &server.om.outputs, link) {
			if (output->layer_shell.requested.default_) {
				wlr_layer_surface->output = output->wlr_output;
				found = true;
				break;
			}
		}
		if (!found) {
			if (!wl_list_empty(&server.om.outputs)) {
				struct rv_output *first = wl_container_of(server.om.outputs.next, first, link);
				rv_info(RV_SCOPE_WM,
					"window manager did not set default layer surface output, choosing arbitrary output");
				wlr_layer_surface->output = first->wlr_output;
			} else {
				rv_err(RV_SCOPE_WM, "no output available for layer surface '%s'", wlr_layer_surface->namespace);
				wlr_layer_surface_v1_destroy(wlr_layer_surface);
				return;
			}
		}
	}

	if (!rv_layer_surface_create(wlr_layer_surface)) {
		wl_resource_post_no_memory(wlr_layer_surface->resource);
		return;
	}
}

void rv_layer_shell_check_exclusive_focus(struct rv_layer_shell *layer_shell) {
	// Find the topmost layer surface (if any) in the top or overlay layers which
	// requests exclusive keyboard interactivity.
	struct rv_layer_surface *to_focus = NULL;
	{
		const enum zwlr_layer_shell_v1_layer layers[] = {
			ZWLR_LAYER_SHELL_V1_LAYER_OVERLAY,
			ZWLR_LAYER_SHELL_V1_LAYER_TOP,
		};
		for (size_t i = 0; i < sizeof(layers) / sizeof(layers[0]); i++) {
			struct wlr_scene_tree *tree = rv_scene_layer_surface_tree(&server.scene, layers[i]);
			// Iterate in reverse to match rendering order.
			struct wlr_scene_node *node, *tmp;
			wl_list_for_each_reverse_safe(node, tmp, &tree->children, link) {
				assert(node->type == WLR_SCENE_NODE_TREE);
				struct rv_scene_node_data *node_data = node->data;
				if (node_data == NULL) {
					continue;
				}
				assert(node_data->data.tag == RV_SCENE_NODE_DATA_DATA_LAYER_SURFACE);
				struct rv_layer_surface *layer_surface = node_data->data.layer_surface;
				struct wlr_layer_surface_v1 *wlr_layer_surface = layer_surface->wlr_layer_surface;
				if (wlr_layer_surface->surface->mapped &&
						wlr_layer_surface->current.keyboard_interactive ==
							ZWLR_LAYER_SURFACE_V1_KEYBOARD_INTERACTIVITY_EXCLUSIVE) {
					to_focus = layer_surface;
					goto found;
				}
			}
		}
	}
found:;

	struct rv_seat *seat, *tmp;
	wl_list_for_each_safe(seat, tmp, &server.input_manager.seats, link) {
		if (to_focus != NULL) {
			seat->layer_shell.scheduled.focus = (struct rv_layer_shell_seat_focus){
				.tag = RV_LAYER_SHELL_SEAT_FOCUS_EXCLUSIVE,
				.exclusive = to_focus->ref,
			};
		} else if (seat->layer_shell.scheduled.focus.tag == RV_LAYER_SHELL_SEAT_FOCUS_EXCLUSIVE) {
			seat->layer_shell.scheduled.focus =
				(struct rv_layer_shell_seat_focus){ .tag = RV_LAYER_SHELL_SEAT_FOCUS_NONE };
		}
	}
}
