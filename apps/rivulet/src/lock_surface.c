// SPDX-FileCopyrightText: © 2021 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/LockSurface.zig for Oath, 2026-10-18.

#include "rivulet.h"

static void handle_map(struct wl_listener *listener, void *data);
static void handle_destroy(struct wl_listener *listener, void *data);

bool rv_lock_surface_create(struct wlr_session_lock_surface_v1 *wlr_lock_surface, struct wlr_session_lock_v1 *lock) {
	struct rv_lock_surface *lock_surface = calloc(1, sizeof(*lock_surface));
	if (lock_surface == NULL) {
		return false;
	}

	struct wlr_scene_tree *tree = wlr_scene_subsurface_tree_create(server.scene.locked_tree, wlr_lock_surface->surface);
	if (tree == NULL) {
		free(lock_surface);
		return false;
	}

	lock_surface->wlr_lock_surface = wlr_lock_surface;
	lock_surface->lock = lock;
	lock_surface->tree = tree;
	lock_surface->idle_update_focus = NULL;
	lock_surface->map.notify = handle_map;
	lock_surface->surface_destroy.notify = handle_destroy;
	wlr_lock_surface->data = lock_surface;

	if (!rv_scene_node_data_attach(&tree->node, (struct rv_scene_node_data_data){
			.tag = RV_SCENE_NODE_DATA_DATA_LOCK_SURFACE,
			.lock_surface = lock_surface,
		})) {
		wlr_scene_node_destroy(&tree->node);
		free(lock_surface);
		return false;
	}
	wlr_lock_surface->surface->data = &tree->node;

	wl_signal_add(&wlr_lock_surface->surface->events.map, &lock_surface->map);
	wl_signal_add(&wlr_lock_surface->events.destroy, &lock_surface->surface_destroy);

	rv_lock_surface_configure(lock_surface);

	return true;
}

static void destroy(struct rv_lock_surface *lock_surface) {
	{
		struct rv_seat_focus new_focus = { .tag = RV_SEAT_FOCUS_NONE };
		struct wlr_session_lock_surface_v1 *surface, *surface_tmp;
		wl_list_for_each_safe(surface, surface_tmp, &lock_surface->lock->surfaces, link) {
			if (surface != lock_surface->wlr_lock_surface) {
				new_focus = (struct rv_seat_focus){
					.tag = RV_SEAT_FOCUS_LOCK_SURFACE,
					.lock_surface = surface->data,
				};
				break;
			}
		}

		struct rv_seat *seat, *seat_tmp;
		wl_list_for_each_safe(seat, seat_tmp, &server.input_manager.seats, link) {
			if (seat->focused.tag == RV_SEAT_FOCUS_LOCK_SURFACE && seat->focused.lock_surface == lock_surface) {
				rv_seat_focus(seat, new_focus);
			}
			rv_cursor_update_state(&seat->cursor);
		}
	}

	if (lock_surface->idle_update_focus != NULL) {
		wl_event_source_remove(lock_surface->idle_update_focus);
	}

	wl_list_remove(&lock_surface->map.link);
	wl_list_remove(&lock_surface->surface_destroy.link);

	// The wlr_surface may outlive the wlr_lock_surface so we must clean up the user data.
	lock_surface->wlr_lock_surface->surface->data = NULL;

	free(lock_surface);
}

struct rv_output *rv_lock_surface_get_output(struct rv_lock_surface *lock_surface) {
	return lock_surface->wlr_lock_surface->output->data;
}

void rv_lock_surface_configure(struct rv_lock_surface *lock_surface) {
	int output_width;
	int output_height;
	wlr_output_effective_resolution(lock_surface->wlr_lock_surface->output, &output_width, &output_height);
	(void)wlr_session_lock_surface_v1_configure(lock_surface->wlr_lock_surface, (uint32_t)output_width,
		(uint32_t)output_height);
}

static void update_focus(void *data);

static void handle_map(struct wl_listener *listener, void *data) {
	struct rv_lock_surface *lock_surface = wl_container_of(listener, lock_surface, map);
	struct rv_output *output = rv_lock_surface_get_output(lock_surface);
	wlr_scene_node_set_position(&lock_surface->tree->node, output->sent.x, output->sent.y);

	// Unfortunately the surface commit handlers for the scene subsurface tree corresponding to
	// this lock surface won't be called until after this function returns, which means that we cannot
	// update pointer focus yet as the nodes in the scene graph representing this lock surface are still
	// 0x0 in size. To work around this, use an idle callback.
	struct wl_event_loop *event_loop = wl_display_get_event_loop(server.wl_server);
	assert(lock_surface->idle_update_focus == NULL);
	lock_surface->idle_update_focus = wl_event_loop_add_idle(event_loop, update_focus, lock_surface);
	if (lock_surface->idle_update_focus == NULL) {
		rv_err(RV_SCOPE_DEFAULT, "out of memory");
		return;
	}
}

static void update_focus(void *data) {
	struct rv_lock_surface *lock_surface = data;

	struct rv_seat *seat, *tmp;
	wl_list_for_each_safe(seat, tmp, &server.input_manager.seats, link) {
		if (seat->focused.tag != RV_SEAT_FOCUS_LOCK_SURFACE) {
			rv_seat_focus(seat, (struct rv_seat_focus){
				.tag = RV_SEAT_FOCUS_LOCK_SURFACE,
				.lock_surface = lock_surface,
			});
		}
		rv_cursor_update_state(&seat->cursor);
	}

	lock_surface->idle_update_focus = NULL;
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_lock_surface *lock_surface = wl_container_of(listener, lock_surface, surface_destroy);

	destroy(lock_surface);
}
