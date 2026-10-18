// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Window.zig for Oath, 2026-10-18.

#include <signal.h>
#include <sys/types.h>

#include "river.h"

const struct rv_window_wm_requested rv_window_wm_requested_init = {
	.has_dimensions = false,
	.dimensions = { .width = 0, .height = 0 },
	.bounds = { .width = 0, .height = 0 },
	.ssd = false,
	.tiled = 0,
	.capabilities = RIVER_WINDOW_V1_CAPABILITIES_WINDOW_MENU | RIVER_WINDOW_V1_CAPABILITIES_MAXIMIZE |
		RIVER_WINDOW_V1_CAPABILITIES_FULLSCREEN | RIVER_WINDOW_V1_CAPABILITIES_MINIMIZE,
	.resizing = false,
	.maximized = false,
	.fullscreen = NULL,
	.inform_fullscreen = false,
	.close = false,
};

const struct rv_window_configure rv_window_configure_init = {
	.has_width = false,
	.width = 0,
	.has_height = false,
	.height = 0,
	.bounds = { .width = 0, .height = 0 },
	.activated = false,
	.ssd = false,
	.tiled = 0,
	.capabilities = 0,
	.maximized = false,
	.inform_fullscreen = false,
	.resizing = false,
};

const struct rv_window_rendering_requested rv_window_rendering_requested_init = {
	.x = 0,
	.y = 0,
	.hidden = false,
	.border = { 0 },
	.clip = { .x = 0, .y = 0, .width = 0, .height = 0 },
	.content_clip = { .x = 0, .y = 0, .width = 0, .height = 0 },
};

static const struct river_window_v1_interface window_v1_impl;
static const struct river_window_v1_interface window_v1_inert_impl;

static void handle_destroy(struct wl_resource *window_v1);

// Zig {?s}: a null string prints as "null".
static const char *opt_str(const char *s) {
	return s != NULL ? s : "null";
}

struct rv_window *rv_window_ref_get(struct rv_window_ref ref) {
	return rv_slotmap_get(&server.wm.windows, ref.key);
}

struct rv_window *rv_window_create(struct rv_window_impl impl) {
	assert(impl.tag != RV_WINDOW_IMPL_DESTROYING);

	struct rv_window *window = calloc(1, sizeof(*window));
	if (window == NULL) {
		return NULL;
	}

	struct rv_slotmap_key key;
	if (!rv_slotmap_put(&server.wm.windows, window, &key)) {
		goto err_put;
	}

	struct wlr_scene_tree *tree = wlr_scene_tree_create(server.scene.hidden_tree);
	if (tree == NULL) {
		goto err_tree;
	}

	struct wlr_scene_tree *popup_tree = wlr_scene_tree_create(server.scene.hidden_tree);
	if (popup_tree == NULL) {
		goto err_popup_tree;
	}

	window->ref.key = key;
	window->object = NULL;
	window->state = RV_WINDOW_STATE_INIT;
	window->impl = impl;
	window->tree = tree;
	window->fullscreen_background = wlr_scene_rect_create(tree, 0, 0, (const float[4]){ 0, 0, 0, 1 });
	if (window->fullscreen_background == NULL) {
		goto err_scene;
	}
	window->decorations_below_tree = wlr_scene_tree_create(tree);
	if (window->decorations_below_tree == NULL) {
		goto err_scene;
	}
	if (!rv_scene_saveable_surfaces_init(&window->surfaces, tree)) {
		goto err_scene;
	}
	window->border.left = wlr_scene_rect_create(tree, 0, 0, (const float[4]){ 0, 0, 0, 0 });
	if (window->border.left == NULL) {
		goto err_scene;
	}
	window->border.right = wlr_scene_rect_create(tree, 0, 0, (const float[4]){ 0, 0, 0, 0 });
	if (window->border.right == NULL) {
		goto err_scene;
	}
	window->border.top = wlr_scene_rect_create(tree, 0, 0, (const float[4]){ 0, 0, 0, 0 });
	if (window->border.top == NULL) {
		goto err_scene;
	}
	window->border.bottom = wlr_scene_rect_create(tree, 0, 0, (const float[4]){ 0, 0, 0, 0 });
	if (window->border.bottom == NULL) {
		goto err_scene;
	}
	window->decorations_above_tree = wlr_scene_tree_create(tree);
	if (window->decorations_above_tree == NULL) {
		goto err_scene;
	}
	window->popup_tree = popup_tree;
	window->capture_scene = wlr_scene_create();
	if (window->capture_scene == NULL) {
		goto err_scene;
	}
	window->capture_source = NULL;
	window->wm_scheduled = (struct rv_window_wm_scheduled){ 0 };
	window->wm_sent = (struct rv_window_wm_sent){ 0 };
	window->wm_requested = rv_window_wm_requested_init;
	window->configure_scheduled = rv_window_configure_init;
	window->configure_sent = rv_window_configure_init;
	window->rendering_scheduled = (struct rv_window_rendering_scheduled){ 0 };
	window->rendering_sent = (struct rv_window_rendering_sent){ 0 };
	window->rendering_requested = rv_window_rendering_requested_init;
	window->box = (struct wlr_box){ .x = 0, .y = 0, .width = 0, .height = 0 };
	window->foreign_toplevel_handle = NULL;
	window->wlr_toplevel_handle = NULL;

	rv_wm_node_init(&window->node, RV_WM_NODE_TAG_WINDOW);

	wl_list_init(&window->decorations_below);
	wl_list_init(&window->decorations_above);

	wlr_scene_node_set_enabled(&window->tree->node, false);
	wlr_scene_node_set_enabled(&window->popup_tree->node, false);
	wlr_scene_node_set_enabled(&window->fullscreen_background->node, false);

	window->capture_scene->restack_xwayland_surfaces = false;

	if (!rv_scene_node_data_attach(&window->tree->node,
			(struct rv_scene_node_data_data){ .tag = RV_SCENE_NODE_DATA_DATA_WINDOW, .window = window })) {
		goto err_scene;
	}
	if (!rv_scene_node_data_attach(&window->popup_tree->node,
			(struct rv_scene_node_data_data){ .tag = RV_SCENE_NODE_DATA_DATA_WINDOW, .window = window })) {
		goto err_scene;
	}

	return window;

err_scene:
	wlr_scene_node_destroy(&popup_tree->node);
err_popup_tree:
	wlr_scene_node_destroy(&tree->node);
err_tree:
	rv_slotmap_remove(&server.wm.windows, key);
err_put:
	free(window);
	return NULL;
}

// It's safe to destroy the window after we no longer need the saved buffers
// for frame perfection. We no longer need the saved buffers after the manage
// sequence in which the closed event was sent is completed and the following
// render sequence is completed as well.
void rv_window_destroy(struct rv_window *window) {
	assert(window->impl.tag == RV_WINDOW_IMPL_DESTROYING);

	switch (window->state) {
	case RV_WINDOW_STATE_INIT:
		break;
	case RV_WINDOW_STATE_CLOSING:
		rv_window_manager_dirty_windowing(&server.wm);
		return;
	case RV_WINDOW_STATE_READY:
	case RV_WINDOW_STATE_INITIALIZED:
	case RV_WINDOW_STATE_MAPPED:
		abort(); // unreachable
	}
	assert(window->object == NULL);

	{
		struct rv_seat *seat;
		wl_list_for_each(seat, &server.input_manager.seats, link) {
			assert(seat->focused.tag != RV_SEAT_FOCUS_WINDOW || seat->focused.window != window);
		}
	}

	{
		struct rv_decoration *decoration, *tmp;
		wl_list_for_each_safe(decoration, tmp, &window->decorations_above, link) {
			rv_decoration_destroy(decoration);
		}
		wl_list_for_each_safe(decoration, tmp, &window->decorations_below, link) {
			rv_decoration_destroy(decoration);
		}
	}

	wlr_scene_node_destroy(&window->tree->node);
	wlr_scene_node_destroy(&window->popup_tree->node);
	wlr_scene_node_destroy(&window->capture_scene->tree.node);

	rv_wm_node_deinit(&window->node);

	rv_slotmap_remove(&server.wm.windows, window->ref.key);

	free(window);
}

void rv_window_set_dimensions_hint(struct rv_window *window, struct rv_window_dimensions_hint hint) {
	window->wm_scheduled.dimensions_hint = hint;
	const struct rv_window_dimensions_hint *sent = &window->wm_sent.dimensions_hint;
	if (sent->min_width != hint.min_width || sent->max_width != hint.max_width ||
			sent->min_height != hint.min_height || sent->max_height != hint.max_height) {
		rv_window_manager_dirty_windowing(&server.wm);
	}
}

void rv_window_set_dimensions(struct rv_window *window, int32_t width, int32_t height) {
	window->rendering_scheduled.width = width;
	window->rendering_scheduled.height = height;

	if (window->rendering_scheduled.resend_dimensions ||
			window->rendering_scheduled.width != window->rendering_sent.width ||
			window->rendering_scheduled.height != window->rendering_sent.height) {
		rv_window_manager_dirty_rendering(&server.wm);
	}
}

void rv_window_set_decoration_hint(struct rv_window *window, enum river_window_v1_decoration_hint hint) {
	window->wm_scheduled.decoration_hint = hint;
	if (hint != window->wm_sent.decoration_hint) {
		rv_window_manager_dirty_windowing(&server.wm);
	}
}

// Send dirty state as part of a manage sequence.
void rv_window_manage_start(struct rv_window *window) {
	switch (window->state) {
	case RV_WINDOW_STATE_INIT:
		break;
	case RV_WINDOW_STATE_CLOSING:
		window->state = RV_WINDOW_STATE_INIT;
		window->wm_sent = (struct rv_window_wm_sent){ 0 };
		window->wm_requested = rv_window_wm_requested_init;
		window->rendering_sent = (struct rv_window_rendering_sent){ 0 };
		window->rendering_requested = rv_window_rendering_requested_init;

		wl_list_remove(&window->node.link);
		wl_list_init(&window->node.link);

		rv_window_make_inert(window);
		break;
	case RV_WINDOW_STATE_READY:
	case RV_WINDOW_STATE_INITIALIZED:
	case RV_WINDOW_STATE_MAPPED: {
		struct wl_resource *wm_v1 = server.wm.object;
		if (wm_v1 == NULL) {
			return;
		}
		const bool new = window->object == NULL;
		struct wl_resource *window_v1 = window->object;
		if (window_v1 == NULL) {
			window_v1 = wl_resource_create(wl_resource_get_client(wm_v1), &river_window_v1_interface,
				wl_resource_get_version(wm_v1), 0);
			if (window_v1 == NULL) {
				rv_err(RV_SCOPE_WM, "out of memory");
				return; // try again next update
			}
			window->object = window_v1;
			wl_resource_set_implementation(window_v1, &window_v1_impl, window, handle_destroy);
			river_window_manager_v1_send_window(wm_v1, window_v1);

			wl_list_remove(&window->node.link);
			wl_list_insert(server.wm.rendering_requested.list.prev, &window->node.link);

			// A handle may have already been created if the window manager is restarted.
			if (window->foreign_toplevel_handle == NULL) {
				const struct wlr_ext_foreign_toplevel_handle_v1_state state = {
					.title = rv_window_get_title(window),
					.app_id = rv_window_get_app_id(window),
				};
				struct wlr_ext_foreign_toplevel_handle_v1 *handle =
					wlr_ext_foreign_toplevel_handle_v1_create(server.foreign_toplevel_list, &state);
				if (handle != NULL) {
					window->foreign_toplevel_handle = handle;
					handle->data = window;
				} else {
					rv_err(RV_SCOPE_WM, "failed to create ext foreign toplevel handle");
				}
			}

			if (window->wlr_toplevel_handle == NULL) {
				struct wlr_foreign_toplevel_handle_v1 *handle =
					wlr_foreign_toplevel_handle_v1_create(server.wlr_foreign_toplevel_manager);
				if (handle != NULL) {
					window->wlr_toplevel_handle = handle;
					const char *title = rv_window_get_title(window);
					if (title != NULL) {
						wlr_foreign_toplevel_handle_v1_set_title(handle, title);
					}
					const char *app_id = rv_window_get_app_id(window);
					if (app_id != NULL) {
						wlr_foreign_toplevel_handle_v1_set_app_id(handle, app_id);
					}
				} else {
					rv_err(RV_SCOPE_WM, "failed to create wlr foreign toplevel handle");
				}
			}
		}

		if (new) {
			if (wl_resource_get_version(window_v1) >= 2) {
				river_window_v1_send_unreliable_pid(window_v1, rv_window_unreliable_pid(window));
			}
			if (wl_resource_get_version(window_v1) >= 4) {
				if (window->foreign_toplevel_handle != NULL) {
					river_window_v1_send_identifier(window_v1, window->foreign_toplevel_handle->identifier);
				}
			}
		}

		struct rv_window_wm_scheduled *scheduled = &window->wm_scheduled;
		struct rv_window_wm_sent *sent = &window->wm_sent;

		if (new || scheduled->dimensions_hint.min_width != sent->dimensions_hint.min_width ||
				scheduled->dimensions_hint.max_width != sent->dimensions_hint.max_width ||
				scheduled->dimensions_hint.min_height != sent->dimensions_hint.min_height ||
				scheduled->dimensions_hint.max_height != sent->dimensions_hint.max_height) {
			river_window_v1_send_dimensions_hint(window_v1,
				scheduled->dimensions_hint.min_width,
				scheduled->dimensions_hint.min_height,
				scheduled->dimensions_hint.max_width,
				scheduled->dimensions_hint.max_height);
			sent->dimensions_hint = scheduled->dimensions_hint;
		}
		if (new || scheduled->decoration_hint != sent->decoration_hint) {
			river_window_v1_send_decoration_hint(window_v1, window->wm_scheduled.decoration_hint);
			sent->decoration_hint = scheduled->decoration_hint;
		}

		if (scheduled->has_show_window_menu_requested) {
			river_window_v1_send_show_window_menu_requested(window_v1,
				scheduled->show_window_menu_requested.x, scheduled->show_window_menu_requested.y);
			scheduled->has_show_window_menu_requested = false;
		}
		switch (scheduled->fullscreen_requested.tag) {
		case RV_WINDOW_FULLSCREEN_REQUEST_NO_REQUEST:
			break;
		case RV_WINDOW_FULLSCREEN_REQUEST_FULLSCREEN: {
			struct rv_output *output_hint = scheduled->fullscreen_requested.fullscreen;
			if (output_hint != NULL) {
				river_window_v1_send_fullscreen_requested(window_v1, output_hint->object);
			} else {
				river_window_v1_send_fullscreen_requested(window_v1, NULL);
			}
			break;
		}
		case RV_WINDOW_FULLSCREEN_REQUEST_EXIT:
			river_window_v1_send_exit_fullscreen_requested(window_v1);
			break;
		}
		scheduled->fullscreen_requested = (struct rv_window_fullscreen_request){
			.tag = RV_WINDOW_FULLSCREEN_REQUEST_NO_REQUEST,
		};
		switch (scheduled->maximize_requested) {
		case RV_WINDOW_WM_SCHEDULED_MAXIMIZE_REQUESTED_NO_REQUEST:
			break;
		case RV_WINDOW_WM_SCHEDULED_MAXIMIZE_REQUESTED_MAXIMIZE:
			river_window_v1_send_maximize_requested(window_v1);
			break;
		case RV_WINDOW_WM_SCHEDULED_MAXIMIZE_REQUESTED_UNMAXIMIZE:
			river_window_v1_send_unmaximize_requested(window_v1);
			break;
		}
		scheduled->maximize_requested = RV_WINDOW_WM_SCHEDULED_MAXIMIZE_REQUESTED_NO_REQUEST;
		if (scheduled->minimize_requested) {
			river_window_v1_send_minimize_requested(window_v1);
		}
		scheduled->minimize_requested = false;

		struct rv_window *parent = rv_window_get_parent(window);
		if (parent != NULL) {
			if (!sent->has_parent || rv_window_ref_get(sent->parent) != parent) {
				river_window_v1_send_parent(window_v1, parent->object);
				sent->has_parent = true;
				sent->parent = parent->ref;
			}
		} else if (sent->has_parent) {
			river_window_v1_send_parent(window_v1, NULL);
			sent->has_parent = false;
		}

		if (new || scheduled->dirty_app_id) {
			river_window_v1_send_app_id(window_v1, rv_window_get_app_id(window));
			scheduled->dirty_app_id = false;
		}
		if (new || scheduled->dirty_title) {
			river_window_v1_send_title(window_v1, rv_window_get_title(window));
			scheduled->dirty_title = false;
		}

		if (scheduled->pointer_move_requested != NULL) {
			struct rv_seat *seat = scheduled->pointer_move_requested;
			if (seat->object != NULL) {
				rv_debug(RV_SCOPE_WM, "send pointer move requested");
				river_window_v1_send_pointer_move_requested(window_v1, seat->object);
			}
		}
		scheduled->pointer_move_requested = NULL;
		if (scheduled->has_pointer_resize_requested) {
			struct rv_window_wm_scheduled_pointer_resize_requested *data = &scheduled->pointer_resize_requested;
			if (data->seat->object != NULL) {
				rv_debug(RV_SCOPE_WM, "send pointer resize requested");
				river_window_v1_send_pointer_resize_requested(window_v1, data->seat->object, data->edges);
			}
		}
		scheduled->has_pointer_resize_requested = false;
		break;
	}
	}
}

static void handle_destroy_window(struct rv_window *window);

void rv_window_make_inert(struct rv_window *window) {
	struct wl_resource *window_v1 = window->object;
	if (window_v1 != NULL) {
		river_window_v1_send_closed(window_v1);
		wl_resource_set_implementation(window_v1, &window_v1_inert_impl, NULL, NULL);
		handle_destroy_window(window);
	} else {
		assert(window->node.object == NULL);
	}
}

// handleRequestInert: destroy destroys the resource, every other request is ignored.

static void inert_destroy(struct wl_client *client, struct wl_resource *window_v1) {
	wl_resource_destroy(window_v1);
}

static void inert_noop(struct wl_client *client, struct wl_resource *window_v1) {
}

static void inert_noop_u32(struct wl_client *client, struct wl_resource *window_v1, uint32_t a) {
}

static void inert_noop_i32_i32(struct wl_client *client, struct wl_resource *window_v1, int32_t a, int32_t b) {
}

static void inert_set_borders(struct wl_client *client, struct wl_resource *window_v1, uint32_t edges,
		int32_t width, uint32_t r, uint32_t g, uint32_t b, uint32_t a) {
}

static void inert_get_decoration(struct wl_client *client, struct wl_resource *window_v1, uint32_t id,
		struct wl_resource *surface) {
}

static void inert_fullscreen(struct wl_client *client, struct wl_resource *window_v1, struct wl_resource *output) {
}

static void inert_noop_box(struct wl_client *client, struct wl_resource *window_v1, int32_t x, int32_t y,
		int32_t width, int32_t height) {
}

static const struct river_window_v1_interface window_v1_inert_impl = {
	.destroy = inert_destroy,
	.close = inert_noop,
	.get_node = inert_noop_u32,
	.propose_dimensions = inert_noop_i32_i32,
	.hide = inert_noop,
	.show = inert_noop,
	.use_csd = inert_noop,
	.use_ssd = inert_noop,
	.set_borders = inert_set_borders,
	.set_tiled = inert_noop_u32,
	.get_decoration_above = inert_get_decoration,
	.get_decoration_below = inert_get_decoration,
	.inform_resize_start = inert_noop,
	.inform_resize_end = inert_noop,
	.set_capabilities = inert_noop_u32,
	.inform_maximized = inert_noop,
	.inform_unmaximized = inert_noop,
	.inform_fullscreen = inert_noop,
	.inform_not_fullscreen = inert_noop,
	.fullscreen = inert_fullscreen,
	.exit_fullscreen = inert_noop,
	.set_clip_box = inert_noop_box,
	.set_content_clip_box = inert_noop_box,
	.set_dimension_bounds = inert_noop_i32_i32,
};

// Zig handleDestroy(window_v1, window): also called directly by makeInert()
// after the handler was swapped to the inert one (whose user data is null).
static void handle_destroy_window(struct rv_window *window) {
	window->object = NULL;
	window->wm_requested = rv_window_wm_requested_init;
	const int32_t x = window->rendering_requested.x;
	const int32_t y = window->rendering_requested.y;
	window->rendering_requested = (struct rv_window_rendering_requested){
		.x = x,
		.y = y,
		.hidden = false,
		.border = { 0 },
		.clip = { .x = 0, .y = 0, .width = 0, .height = 0 },
		.content_clip = { .x = 0, .y = 0, .width = 0, .height = 0 },
	};
	rv_window_manager_dirty_windowing(&server.wm);
	rv_wm_node_make_inert(&window->node);
	struct rv_decoration *decoration;
	wl_list_for_each(decoration, &window->decorations_above, link) {
		rv_decoration_make_inert(decoration);
	}
	wl_list_for_each(decoration, &window->decorations_below, link) {
		rv_decoration_make_inert(decoration);
	}
}

static void handle_destroy(struct wl_resource *window_v1) {
	struct rv_window *window = wl_resource_get_user_data(window_v1);
	handle_destroy_window(window);
}

// handleRequest: one function per request.

static struct rv_window *window_from_resource(struct wl_resource *window_v1) {
	struct rv_window *window = wl_resource_get_user_data(window_v1);
	assert(window->object == window_v1);
	return window;
}

static void request_destroy(struct wl_client *client, struct wl_resource *window_v1) {
	(void)window_from_resource(window_v1);
	wl_resource_destroy(window_v1);
}

static void request_close(struct wl_client *client, struct wl_resource *window_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	window->wm_requested.close = true;
}

static void request_get_node(struct wl_client *client, struct wl_resource *window_v1, uint32_t id) {
	struct rv_window *window = window_from_resource(window_v1);
	if (window->node.object != NULL) {
		wl_resource_post_error(window_v1, RIVER_WINDOW_V1_ERROR_NODE_EXISTS, "%s",
			"window already has a node object");
		return;
	}
	rv_wm_node_create_object(&window->node, wl_resource_get_client(window_v1),
		wl_resource_get_version(window_v1), id);
}

static void request_propose_dimensions(struct wl_client *client, struct wl_resource *window_v1, int32_t width,
		int32_t height) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	if (width < 0 || height < 0) {
		wl_resource_post_error(window_v1, RIVER_WINDOW_V1_ERROR_INVALID_DIMENSIONS, "%s",
			"dimensions must be greater than or equal to 0 ");
		return;
	}
	window->wm_requested.has_dimensions = true;
	window->wm_requested.dimensions = (struct rv_window_dimensions){
		.width = width,
		.height = height,
	};
}

static void request_hide(struct wl_client *client, struct wl_resource *window_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}
	window->rendering_requested.hidden = true;
}

static void request_show(struct wl_client *client, struct wl_resource *window_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}
	window->rendering_requested.hidden = false;
}

static void request_use_ssd(struct wl_client *client, struct wl_resource *window_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	window->wm_requested.ssd = true;
}

static void request_use_csd(struct wl_client *client, struct wl_resource *window_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	window->wm_requested.ssd = false;
}

static void request_set_borders(struct wl_client *client, struct wl_resource *window_v1, uint32_t edges,
		int32_t width, uint32_t r, uint32_t g, uint32_t b, uint32_t a) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}
	if (width < 0) {
		wl_resource_post_error(window_v1, RIVER_WINDOW_V1_ERROR_INVALID_BORDER, "%s",
			"border width must be greater than or equal to 0 ");
		return;
	}
	window->rendering_requested.border = (struct rv_window_border){
		.edges = edges,
		.width = width,
		.r = r,
		.g = g,
		.b = b,
		.a = a,
	};
}

static void request_set_tiled(struct wl_client *client, struct wl_resource *window_v1, uint32_t edges) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	window->wm_requested.tiled = edges;
}

// inline .get_decoration_above, .get_decoration_below
static void get_decoration(struct wl_resource *window_v1, uint32_t id, struct wl_resource *surface_resource,
		bool above) {
	struct rv_window *window = window_from_resource(window_v1);
	struct wlr_surface *surface = wlr_surface_from_resource(surface_resource);
	bool already_has_role = false;
	struct rv_decoration *decoration = rv_decoration_create(
		wl_resource_get_client(window_v1),
		wl_resource_get_version(window_v1),
		id,
		surface,
		above ? window->decorations_above_tree : window->decorations_below_tree,
		&already_has_role);
	if (decoration == NULL) {
		if (already_has_role) {
			return;
		}
		// error.OutOfMemory, error.ResourceCreateFailed
		wl_client_post_no_memory(wl_resource_get_client(window_v1));
		rv_err(RV_SCOPE_WM, "out of memory");
		return;
	}
	if (above) {
		wl_list_insert(window->decorations_above.prev, &decoration->link);
	} else {
		wl_list_insert(window->decorations_below.prev, &decoration->link);
	}
}

static void request_get_decoration_above(struct wl_client *client, struct wl_resource *window_v1, uint32_t id,
		struct wl_resource *surface) {
	get_decoration(window_v1, id, surface, true);
}

static void request_get_decoration_below(struct wl_client *client, struct wl_resource *window_v1, uint32_t id,
		struct wl_resource *surface) {
	get_decoration(window_v1, id, surface, false);
}

static void request_inform_resize_start(struct wl_client *client, struct wl_resource *window_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	window->wm_requested.resizing = true;
}

static void request_inform_resize_end(struct wl_client *client, struct wl_resource *window_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	window->wm_requested.resizing = false;
}

static void request_set_capabilities(struct wl_client *client, struct wl_resource *window_v1, uint32_t caps) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	window->wm_requested.capabilities = caps;
}

static void request_inform_maximized(struct wl_client *client, struct wl_resource *window_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	window->wm_requested.maximized = true;
}

static void request_inform_unmaximized(struct wl_client *client, struct wl_resource *window_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	window->wm_requested.maximized = false;
}

static void request_inform_fullscreen(struct wl_client *client, struct wl_resource *window_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	window->wm_requested.inform_fullscreen = true;
}

static void request_inform_not_fullscreen(struct wl_client *client, struct wl_resource *window_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	window->wm_requested.inform_fullscreen = false;
}

static void request_fullscreen(struct wl_client *client, struct wl_resource *window_v1,
		struct wl_resource *output_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	void *data = wl_resource_get_user_data(output_v1);
	if (data == NULL) {
		return;
	}
	struct rv_output *output = data;
	window->wm_requested.fullscreen = output;
}

static void request_exit_fullscreen(struct wl_client *client, struct wl_resource *window_v1) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	window->wm_requested.fullscreen = NULL;
}

static void request_set_clip_box(struct wl_client *client, struct wl_resource *window_v1, int32_t x, int32_t y,
		int32_t width, int32_t height) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}
	if (width < 0 || height < 0) {
		wl_resource_post_error(window_v1, RIVER_WINDOW_V1_ERROR_INVALID_CLIP_BOX, "%s",
			"width/height must be greater than or equal to 0 ");
		return;
	}
	window->rendering_requested.clip = (struct wlr_box){
		.x = x,
		.y = y,
		.width = width,
		.height = height,
	};
}

static void request_set_content_clip_box(struct wl_client *client, struct wl_resource *window_v1, int32_t x,
		int32_t y, int32_t width, int32_t height) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}
	if (width < 0 || height < 0) {
		wl_resource_post_error(window_v1, RIVER_WINDOW_V1_ERROR_INVALID_CLIP_BOX, "%s",
			"width/height must be greater than or equal to 0 ");
		return;
	}
	window->rendering_requested.content_clip = (struct wlr_box){
		.x = x,
		.y = y,
		.width = width,
		.height = height,
	};
}

static void request_set_dimension_bounds(struct wl_client *client, struct wl_resource *window_v1,
		int32_t max_width, int32_t max_height) {
	struct rv_window *window = window_from_resource(window_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	if (max_width < 0 || max_height < 0) {
		wl_resource_post_error(window_v1, RIVER_WINDOW_V1_ERROR_INVALID_DIMENSIONS, "%s",
			"dimensions must be greater than or equal to 0 ");
		return;
	}
	window->wm_requested.bounds = (struct rv_window_dimensions){
		.width = max_width,
		.height = max_height,
	};
}

static const struct river_window_v1_interface window_v1_impl = {
	.destroy = request_destroy,
	.close = request_close,
	.get_node = request_get_node,
	.propose_dimensions = request_propose_dimensions,
	.hide = request_hide,
	.show = request_show,
	.use_csd = request_use_csd,
	.use_ssd = request_use_ssd,
	.set_borders = request_set_borders,
	.set_tiled = request_set_tiled,
	.get_decoration_above = request_get_decoration_above,
	.get_decoration_below = request_get_decoration_below,
	.inform_resize_start = request_inform_resize_start,
	.inform_resize_end = request_inform_resize_end,
	.set_capabilities = request_set_capabilities,
	.inform_maximized = request_inform_maximized,
	.inform_unmaximized = request_inform_unmaximized,
	.inform_fullscreen = request_inform_fullscreen,
	.inform_not_fullscreen = request_inform_not_fullscreen,
	.fullscreen = request_fullscreen,
	.exit_fullscreen = request_exit_fullscreen,
	.set_clip_box = request_set_clip_box,
	.set_content_clip_box = request_set_content_clip_box,
	.set_dimension_bounds = request_set_dimension_bounds,
};

// Applies window management state from the window manager and sends a configure
// to the window if necessary.
// Returns true if the configure should be waited for by the transaction system.
bool rv_window_manage_finish(struct rv_window *window) {
	struct rv_window_wm_requested *wm_requested = &window->wm_requested;

	// This can happen if the window is destroyed after being sent to the wm but
	// before being mapped.
	if (window->impl.tag == RV_WINDOW_IMPL_DESTROYING) {
		assert(window->state == RV_WINDOW_STATE_CLOSING);
		return false;
	}

	switch (window->state) {
	case RV_WINDOW_STATE_INIT:
		abort(); // unreachable
	case RV_WINDOW_STATE_READY:
		if (!wm_requested->has_dimensions && wm_requested->fullscreen == NULL) {
			// A connected WM is allowed to defer the first size until
			// propose_dimensions / fullscreen in a later manage
			// sequence. With no WM, xdg-shell still requires a
			// configure before the client may attach a buffer; 0×0
			// means the client picks its own size.
			if (server.wm.object != NULL) {
				return false;
			}
			rv_info(RV_SCOPE_WM, "no window manager; configuring 0x0 so xdg clients may map");
			wm_requested->has_dimensions = true;
			wm_requested->dimensions = (struct rv_window_dimensions){ .width = 0, .height = 0 };
		}
		window->state = RV_WINDOW_STATE_INITIALIZED;
		break;
	case RV_WINDOW_STATE_INITIALIZED:
	case RV_WINDOW_STATE_MAPPED:
		break;
	case RV_WINDOW_STATE_CLOSING:
		return false;
	}

	if (wm_requested->close) {
		rv_window_close(window);
		wm_requested->close = false;
	}

	bool activated = false;
	{
		struct rv_seat *seat;
		wl_list_for_each(seat, &server.wm.sent.seats, link_sent) {
			if (seat->focused.tag == RV_SEAT_FOCUS_WINDOW && seat->focused.window == window) {
				activated = true;
				break;
			}
		}
	}

	if (window->wlr_toplevel_handle != NULL) {
		wlr_foreign_toplevel_handle_v1_set_activated(window->wlr_toplevel_handle, activated);
	}

	// const width, const height: ?u31 each
	bool has_size = false;
	int32_t width = 0;
	int32_t height = 0;
	if (wm_requested->fullscreen != NULL) {
		struct rv_output *output = wm_requested->fullscreen;
		int32_t output_width, output_height;
		rv_output_state_dimensions(&output->sent, &output_width, &output_height);
		if (!window->configure_sent.has_width || window->configure_sent.width != output_width ||
				!window->configure_sent.has_height || window->configure_sent.height != output_height) {
			window->configure_scheduled.has_width = true;
			window->configure_scheduled.width = output_width;
			window->configure_scheduled.has_height = true;
			window->configure_scheduled.height = output_height;
			window->rendering_scheduled.resend_dimensions = true;
			has_size = true;
			width = output_width;
			height = output_height;
		}
	} else if (wm_requested->has_dimensions) {
		window->rendering_scheduled.resend_dimensions = true;
		has_size = true;
		width = wm_requested->dimensions.width;
		height = wm_requested->dimensions.height;
	}
	wm_requested->has_dimensions = false;

	window->configure_scheduled = (struct rv_window_configure){
		.has_width = has_size,
		.width = width,
		.has_height = has_size,
		.height = height,
		.bounds = wm_requested->bounds,
		.activated = activated,
		.ssd = wm_requested->ssd,
		.tiled = wm_requested->tiled,
		.capabilities = wm_requested->capabilities,
		.resizing = wm_requested->resizing,
		.maximized = wm_requested->maximized,
		.inform_fullscreen = wm_requested->inform_fullscreen,
	};

	bool track_configure;
	switch (window->impl.tag) {
	case RV_WINDOW_IMPL_TOPLEVEL:
		track_configure = rv_xdg_toplevel_configure(&window->impl.toplevel);
		break;
	case RV_WINDOW_IMPL_XWAYLAND:
#if WLR_HAS_XWAYLAND
		track_configure = rv_xwayland_window_configure(&window->impl.xwayland);
		break;
#else
		abort(); // noreturn payload
#endif
	case RV_WINDOW_IMPL_DESTROYING:
	default:
		abort(); // unreachable
	}

	if (track_configure && window->state == RV_WINDOW_STATE_MAPPED) {
		rv_scene_saveable_surfaces_save(&window->surfaces);
		rv_window_send_frame_done(window);
	}

	return track_configure;
}

// Zig @intCast to u31 (safety-checked in ReleaseSafe).
static int32_t int_cast_u31(int32_t value) {
	assert(value >= 0);
	return value;
}

static enum river_output_v1_presentation_mode presentation_hint(struct rv_window *window);

void rv_window_render_start(struct rv_window *window) {
	switch (window->impl.tag) {
	case RV_WINDOW_IMPL_TOPLEVEL: {
		struct rv_xdg_toplevel *toplevel = &window->impl.toplevel;
		switch (toplevel->configure_state.tag) {
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_INFLIGHT:
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_ACKED:
			// The transaction has timed out for the xdg toplevel, which means a commit
			// in response to the configure with the inflight width/height has not yet
			// been made. It may seem that we should therefore leave the current.box
			// width/height unchanged. However, this would in fact cause visual glitches.
			//
			// We must update the dimensions to the current geometry of the
			// xdg toplevel here in order to handle the following series of events:
			//
			// 0. initial state: client has dimensions X
			// 1. transaction A sends a configure of size Y
			// 2. transaction A times out - saved surfaces are dropped
			// 3. transaction B sends a configure of size Z
			// 4. client commits buffer of size Y
			// 5. transaction B times out - saved surfaces are dropped
			//
			// If we did not use the current geometry of the toplevel at this point
			// we would be rendering the SSD border at initial size X but the surface
			// would be rendered at size Y.
			switch (toplevel->configure_state.tag) {
			case RV_XDG_TOPLEVEL_CONFIGURE_STATE_INFLIGHT: {
				const uint32_t serial = toplevel->configure_state.inflight;
				toplevel->configure_state = (struct rv_xdg_toplevel_configure_state){
					.tag = RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT,
					.timed_out = serial,
				};
				break;
			}
			case RV_XDG_TOPLEVEL_CONFIGURE_STATE_ACKED:
				toplevel->configure_state = (struct rv_xdg_toplevel_configure_state){
					.tag = RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT_ACKED,
				};
				break;
			default:
				abort(); // unreachable
			}
			break;
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_COMMITTED:
			toplevel->configure_state = (struct rv_xdg_toplevel_configure_state){
				.tag = RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE,
			};
			break;
		// A timed_out or timed_out_acked value is possible in the case of a
		// manage sequence followed by two render sequences for example.
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE:
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT:
		case RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT_ACKED:
			break;
		}
		window->rendering_scheduled.width = int_cast_u31(toplevel->geometry.width);
		window->rendering_scheduled.height = int_cast_u31(toplevel->geometry.height);
		break;
	}
	case RV_WINDOW_IMPL_XWAYLAND: {
#if WLR_HAS_XWAYLAND
		struct rv_xwayland_window *xwindow = &window->impl.xwayland;
		window->rendering_scheduled.width = xwindow->xsurface->width;
		window->rendering_scheduled.height = xwindow->xsurface->height;
		break;
#else
		abort(); // noreturn payload
#endif
	}
	case RV_WINDOW_IMPL_DESTROYING:
		break;
	}

	struct rv_window_rendering_sent *sent = &window->rendering_sent;
	struct rv_window_rendering_scheduled *scheduled = &window->rendering_scheduled;

	// Check if mapped to handle timeout of the first configure sent.
	if (window->state == RV_WINDOW_STATE_MAPPED &&
			(scheduled->resend_dimensions ||
				scheduled->width != sent->width || scheduled->height != sent->height)) {
		if (window->object != NULL) {
			river_window_v1_send_dimensions(window->object, scheduled->width, scheduled->height);
			window->rendering_scheduled.resend_dimensions = false;
		}
	}
	sent->width = scheduled->width;
	sent->height = scheduled->height;

	const enum river_output_v1_presentation_mode hint = presentation_hint(window);
	if (sent->presentation_hint != hint) {
		if (window->object != NULL) {
			if (wl_resource_get_version(window->object) >= 4) {
				river_window_v1_send_presentation_hint(window->object, hint);
			}
		}
		sent->presentation_hint = hint;
	}
}

static enum river_output_v1_presentation_mode presentation_hint(struct rv_window *window) {
	struct wlr_surface *root_surface = rv_window_root_surface(window);
	if (root_surface == NULL) {
		return RIVER_OUTPUT_V1_PRESENTATION_MODE_VSYNC;
	}
	switch (wlr_tearing_control_manager_v1_surface_hint_from_surface(server.tearing_control_manager,
			root_surface)) {
	case WP_TEARING_CONTROL_V1_PRESENTATION_HINT_ASYNC:
		return RIVER_OUTPUT_V1_PRESENTATION_MODE_ASYNC;
	case WP_TEARING_CONTROL_V1_PRESENTATION_HINT_VSYNC:
		return RIVER_OUTPUT_V1_PRESENTATION_MODE_VSYNC;
	default:
		abort(); // unreachable
	}
}

static void draw_borders(struct rv_window *window);
static void apply_surface_clip(struct rv_window *window, const struct wlr_box *a, const struct wlr_box *b);

void rv_window_render_finish(struct rv_window *window) {
	struct rv_window_rendering_requested *requested = &window->rendering_requested;

	// Keep the scene nodes disabled until the render sequence in which the first
	// dimensions event was sent is completed. If we enable the nodes before the
	// window is mapped, there may be an imperfect frame rendered after the window
	// commits its initial buffer and before the render sequence with the first
	// dimensions event is completed.
	// Keeping the nodes enabled while closing is necessary for frame perfection.
	const bool enabled = !requested->hidden &&
		(window->state == RV_WINDOW_STATE_MAPPED || window->state == RV_WINDOW_STATE_CLOSING);
	wlr_scene_node_set_enabled(&window->tree->node, enabled);
	wlr_scene_node_set_enabled(&window->popup_tree->node, enabled);

	window->box.width = window->rendering_sent.width;
	window->box.height = window->rendering_sent.height;

	struct wlr_box clip = requested->clip;
	struct wlr_box content_clip = requested->content_clip;
	if (window->wm_requested.fullscreen != NULL) {
		struct rv_output *output = window->wm_requested.fullscreen;
		window->box.x = output->sent.x;
		window->box.y = output->sent.y;
		wlr_scene_node_set_enabled(&window->fullscreen_background->node, true);
		int32_t width, height;
		rv_output_state_dimensions(&output->sent, &width, &height);
		wlr_scene_rect_set_size(window->fullscreen_background, width, height);
		clip = (struct wlr_box){ .x = 0, .y = 0, .width = width, .height = height };
		content_clip = (struct wlr_box){ .x = 0, .y = 0, .width = 0, .height = 0 };
		wlr_scene_node_set_enabled(&window->border.left->node, false);
		wlr_scene_node_set_enabled(&window->border.right->node, false);
		wlr_scene_node_set_enabled(&window->border.top->node, false);
		wlr_scene_node_set_enabled(&window->border.bottom->node, false);
	} else {
		window->box.x = requested->x;
		window->box.y = requested->y;
		wlr_scene_node_set_enabled(&window->fullscreen_background->node, false);
		draw_borders(window);
	}
	wlr_scene_node_set_position(&window->tree->node, window->box.x, window->box.y);
	wlr_scene_node_set_position(&window->popup_tree->node, window->box.x, window->box.y);

	switch (window->impl.tag) {
	case RV_WINDOW_IMPL_XWAYLAND:
#if WLR_HAS_XWAYLAND
		(void)rv_xwayland_window_configure(&window->impl.xwayland);
		break;
#else
		abort(); // noreturn payload
#endif
	case RV_WINDOW_IMPL_TOPLEVEL:
	case RV_WINDOW_IMPL_DESTROYING:
		break;
	}

	apply_surface_clip(window, &clip, &content_clip);
	struct rv_decoration *decoration;
	wl_list_for_each(decoration, &window->decorations_above, link) {
		rv_decoration_render_finish(decoration, &clip);
	}
	wl_list_for_each(decoration, &window->decorations_below, link) {
		rv_decoration_render_finish(decoration, &clip);
	}
}

static void draw_borders(struct rv_window *window) {
	struct rv_window_rendering_requested *requested = &window->rendering_requested;
	struct wlr_box content = {
		.x = 0,
		.y = 0,
		.width = window->box.width,
		.height = window->box.height,
	};
	if (wlr_box_empty(&requested->content_clip) ||
			wlr_box_intersection(&content, &content, &requested->content_clip)) {
		// f32 cannot represent all u32 values exactly, therefore we must initially use f64
		// (which can) and then cast to f32, potentially losing precision.
		const struct rv_window_border *border = &requested->border;
		const float color[4] = {
			(float)((double)border->r / (double)UINT32_MAX),
			(float)((double)border->g / (double)UINT32_MAX),
			(float)((double)border->b / (double)UINT32_MAX),
			(float)((double)border->a / (double)UINT32_MAX),
		};
		struct wlr_box left = {
			.x = -border->width,
			.y = 0,
			.width = border->width,
			.height = content.height,
		};
		struct wlr_box right = {
			.x = content.width,
			.y = 0,
			.width = border->width,
			.height = content.height,
		};
		struct wlr_box top = {
			.x = 0,
			.y = -border->width,
			.width = content.width,
			.height = border->width,
		};
		struct wlr_box bottom = {
			.x = 0,
			.y = content.height,
			.width = content.width,
			.height = border->width,
		};
		// Use left and right scene rects to draw the corners if needed
		if (border->edges & RIVER_WINDOW_V1_EDGES_TOP) {
			left.y -= border->width;
			left.height += border->width;
			right.y -= border->width;
			right.height += border->width;
		}
		if (border->edges & RIVER_WINDOW_V1_EDGES_BOTTOM) {
			left.height += border->width;
			right.height += border->width;
		}
		const struct {
			struct wlr_scene_rect *rect;
			struct wlr_box *box;
			uint32_t edge;
		} edges[] = {
			{ window->border.left, &left, RIVER_WINDOW_V1_EDGES_LEFT },
			{ window->border.right, &right, RIVER_WINDOW_V1_EDGES_RIGHT },
			{ window->border.top, &top, RIVER_WINDOW_V1_EDGES_TOP },
			{ window->border.bottom, &bottom, RIVER_WINDOW_V1_EDGES_BOTTOM },
		};
		for (size_t i = 0; i < sizeof(edges) / sizeof(edges[0]); i++) {
			struct wlr_box *box = edges[i].box;
			if (!wlr_box_empty(&requested->clip)) {
				(void)wlr_box_intersection(box, box, &requested->clip);
			}
			struct wlr_scene_rect *rect = edges[i].rect;
			wlr_scene_node_set_enabled(&rect->node, (border->edges & edges[i].edge) != 0);
			wlr_scene_node_set_position(&rect->node, box->x, box->y);
			wlr_scene_rect_set_size(rect, box->width, box->height);
			wlr_scene_rect_set_color(rect, color);
		}
	}
}

static void apply_surface_clip(struct rv_window *window, const struct wlr_box *a, const struct wlr_box *b) {
	struct wlr_box surface_clip;
	if (!wlr_box_empty(a) && !wlr_box_empty(b)) {
		if (!wlr_box_intersection(&surface_clip, a, b)) {
			// Clip boxes are both non-empty but don't intersect, all window
			// content is clipped away.
			rv_scene_saveable_surfaces_set_enabled(&window->surfaces, false);
			return;
		}
	} else if (!wlr_box_empty(a)) {
		surface_clip = *a;
	} else {
		surface_clip = *b;
	}
	rv_scene_saveable_surfaces_set_enabled(&window->surfaces, true);
	switch (window->impl.tag) {
	case RV_WINDOW_IMPL_TOPLEVEL:
		surface_clip.x += window->impl.toplevel.geometry.x;
		surface_clip.y += window->impl.toplevel.geometry.y;
		break;
	case RV_WINDOW_IMPL_XWAYLAND:
	case RV_WINDOW_IMPL_DESTROYING:
		break;
	}
	// wlroots asserts that a subsurface tree is present.
	if (!wl_list_empty(&window->surfaces.tree->children)) {
		wlr_scene_subsurface_tree_set_clip(&window->surfaces.tree->node, &surface_clip);
	}
}

// Returns null if the window is currently being destroyed and no longer has
// an associated surface.
// May also return null for Xwayland windows that are not currently mapped.
struct wlr_surface *rv_window_root_surface(struct rv_window *window) {
	switch (window->impl.tag) {
	case RV_WINDOW_IMPL_TOPLEVEL:
		return window->impl.toplevel.wlr_toplevel->base->surface;
	case RV_WINDOW_IMPL_XWAYLAND:
#if WLR_HAS_XWAYLAND
		return window->impl.xwayland.xsurface->surface;
#else
		abort(); // noreturn payload
#endif
	case RV_WINDOW_IMPL_DESTROYING:
		return NULL;
	}
	abort(); // unreachable
}

void rv_window_send_frame_done(struct rv_window *window) {
	assert(window->state == RV_WINDOW_STATE_MAPPED);
	assert(window->impl.tag != RV_WINDOW_IMPL_DESTROYING);

	struct timespec now = rv_util_timestamp();
	struct wlr_surface *root_surface = rv_window_root_surface(window);
	assert(root_surface != NULL); // .?
	wlr_surface_send_frame_done(root_surface, &now);
}

#if WLR_HAS_XWAYLAND
// std.ascii.eqlIgnoreCase
static bool ascii_eql_ignore_case(const char *a, const char *b) {
	for (;; a++, b++) {
		unsigned char ca = (unsigned char)*a;
		unsigned char cb = (unsigned char)*b;
		if (ca >= 'A' && ca <= 'Z') {
			ca = ca - 'A' + 'a';
		}
		if (cb >= 'A' && cb <= 'Z') {
			cb = cb - 'A' + 'a';
		}
		if (ca != cb) {
			return false;
		}
		if (ca == '\0') {
			return true;
		}
	}
}

static bool steam_x11_class(const char *class) {
	if (class == NULL) {
		return false;
	}
	return ascii_eql_ignore_case(class, "steam");
}
#endif

void rv_window_close(struct rv_window *window) {
	switch (window->impl.tag) {
	case RV_WINDOW_IMPL_TOPLEVEL:
		wlr_xdg_toplevel_send_close(window->impl.toplevel.wlr_toplevel);
		break;
	case RV_WINDOW_IMPL_XWAYLAND: {
#if WLR_HAS_XWAYLAND
		struct rv_xwayland_window *xwindow = &window->impl.xwayland;
		wlr_xwayland_surface_close(xwindow->xsurface);
		// Steam treats WM_DELETE as hide-to-tray. There is no tray
		// here; Super+Q / Sola close must SIGTERM the client or
		// ubuntu12_32/steam stays up and Arcade refuses Play.
		if (steam_x11_class(xwindow->xsurface->class)) {
			const pid_t pid = xwindow->xsurface->pid;
			if (pid > 1) {
				(void)kill(pid, SIGTERM);
			}
		}
		break;
#else
		abort(); // noreturn payload
#endif
	}
	case RV_WINDOW_IMPL_DESTROYING:
		break;
	}
}

void rv_window_destroy_popups(struct rv_window *window) {
	switch (window->impl.tag) {
	case RV_WINDOW_IMPL_TOPLEVEL:
		rv_xdg_toplevel_destroy_popups(&window->impl.toplevel);
		break;
	case RV_WINDOW_IMPL_XWAYLAND:
	case RV_WINDOW_IMPL_DESTROYING:
		break;
	}
}

struct rv_window *rv_window_get_parent(struct rv_window *window) {
	switch (window->impl.tag) {
	case RV_WINDOW_IMPL_TOPLEVEL: {
		struct wlr_xdg_toplevel *wlr_parent = window->impl.toplevel.wlr_toplevel->parent;
		if (wlr_parent == NULL) {
			return NULL;
		}
		struct rv_xdg_toplevel *parent = wlr_parent->base->data;
		return parent->window;
	}
	case RV_WINDOW_IMPL_XWAYLAND: {
#if WLR_HAS_XWAYLAND
		struct wlr_xwayland_surface *parent_xsurface = window->impl.xwayland.xsurface->parent;
		if (parent_xsurface == NULL) {
			return NULL;
		}
		// It seems that the parent may be an Override Redirect window, which
		// have null data.
		void *parent_data = parent_xsurface->data;
		if (parent_data == NULL) {
			return NULL;
		}
		struct rv_xwayland_window *parent_xwindow = parent_data;
		return parent_xwindow->window;
#else
		abort(); // noreturn payload
#endif
	}
	case RV_WINDOW_IMPL_DESTROYING:
		return NULL;
	}
	abort(); // unreachable
}

int32_t rv_window_unreliable_pid(struct rv_window *window) {
	switch (window->impl.tag) {
	case RV_WINDOW_IMPL_TOPLEVEL: {
		struct wl_client *client =
			wl_resource_get_client(window->impl.toplevel.wlr_toplevel->base->surface->resource);
		pid_t pid;
		uid_t uid;
		gid_t gid;
		wl_client_get_credentials(client, &pid, &uid, &gid);
		return pid;
	}
	case RV_WINDOW_IMPL_XWAYLAND:
#if WLR_HAS_XWAYLAND
		return window->impl.xwayland.xsurface->pid;
#else
		abort(); // noreturn payload
#endif
	case RV_WINDOW_IMPL_DESTROYING:
		abort(); // unreachable
	}
	abort(); // unreachable
}

// Return the current title of the window if any.
const char *rv_window_get_title(struct rv_window *window) {
	switch (window->impl.tag) {
	case RV_WINDOW_IMPL_TOPLEVEL:
		return window->impl.toplevel.wlr_toplevel->title;
	case RV_WINDOW_IMPL_XWAYLAND:
#if WLR_HAS_XWAYLAND
		return window->impl.xwayland.xsurface->title;
#else
		abort(); // noreturn payload
#endif
	case RV_WINDOW_IMPL_DESTROYING:
		abort(); // unreachable
	}
	abort(); // unreachable
}

// Return the current app_id of the window if any.
const char *rv_window_get_app_id(struct rv_window *window) {
	switch (window->impl.tag) {
	case RV_WINDOW_IMPL_TOPLEVEL:
		return window->impl.toplevel.wlr_toplevel->app_id;
	case RV_WINDOW_IMPL_XWAYLAND:
#if WLR_HAS_XWAYLAND
		// X11 clients don't have an app_id but the class serves a similar role.
		return window->impl.xwayland.xsurface->class;
#else
		abort(); // noreturn payload
#endif
	case RV_WINDOW_IMPL_DESTROYING:
		abort(); // unreachable
	}
	abort(); // unreachable
}

// Called by the impl when the surface is ready to be displayed
bool rv_window_map(struct rv_window *window) {
	rv_debug(RV_SCOPE_WM, "window '%s' mapped", opt_str(rv_window_get_title(window)));
	assert(window->impl.tag != RV_WINDOW_IMPL_DESTROYING);
	assert(window->state == RV_WINDOW_STATE_INITIALIZED);
	window->state = RV_WINDOW_STATE_MAPPED;
	return true;
}

// Called by the impl when the surface will no longer be displayed
void rv_window_unmap(struct rv_window *window) {
	rv_debug(RV_SCOPE_WM, "window '%s' unmapped", opt_str(rv_window_get_title(window)));

	rv_scene_saveable_surfaces_save(&window->surfaces);

	assert(window->impl.tag != RV_WINDOW_IMPL_DESTROYING);
	assert(window->state == RV_WINDOW_STATE_MAPPED);
	window->state = RV_WINDOW_STATE_CLOSING;

	rv_window_manager_dirty_windowing(&server.wm);

	if (window->foreign_toplevel_handle != NULL) {
		wlr_ext_foreign_toplevel_handle_v1_destroy(window->foreign_toplevel_handle);
		window->foreign_toplevel_handle = NULL;
	}

	if (window->wlr_toplevel_handle != NULL) {
		wlr_foreign_toplevel_handle_v1_destroy(window->wlr_toplevel_handle);
		window->wlr_toplevel_handle = NULL;
	}

	{
		struct rv_seat *seat;
		wl_list_for_each(seat, &server.input_manager.seats, link) {
			if (seat->focused.tag == RV_SEAT_FOCUS_WINDOW && seat->focused.window == window) {
				rv_seat_focus(seat, (struct rv_seat_focus){ .tag = RV_SEAT_FOCUS_NONE });
			}
		}
	}
}

void rv_window_notify_title(struct rv_window *window) {
	window->wm_scheduled.dirty_title = true;
	rv_window_manager_dirty_windowing(&server.wm);

	if (window->foreign_toplevel_handle != NULL) {
		const struct wlr_ext_foreign_toplevel_handle_v1_state state = {
			.title = rv_window_get_title(window),
			.app_id = rv_window_get_app_id(window),
		};
		wlr_ext_foreign_toplevel_handle_v1_update_state(window->foreign_toplevel_handle, &state);
	}
	if (window->wlr_toplevel_handle != NULL) {
		const char *title = rv_window_get_title(window);
		if (title != NULL) {
			wlr_foreign_toplevel_handle_v1_set_title(window->wlr_toplevel_handle, title);
		}
	}
}

void rv_window_notify_app_id(struct rv_window *window) {
	window->wm_scheduled.dirty_app_id = true;
	rv_window_manager_dirty_windowing(&server.wm);

	if (window->foreign_toplevel_handle != NULL) {
		const struct wlr_ext_foreign_toplevel_handle_v1_state state = {
			.title = rv_window_get_title(window),
			.app_id = rv_window_get_app_id(window),
		};
		wlr_ext_foreign_toplevel_handle_v1_update_state(window->foreign_toplevel_handle, &state);
	}
	if (window->wlr_toplevel_handle != NULL) {
		const char *app_id = rv_window_get_app_id(window);
		if (app_id != NULL) {
			wlr_foreign_toplevel_handle_v1_set_app_id(window->wlr_toplevel_handle, app_id);
		}
	}
}
