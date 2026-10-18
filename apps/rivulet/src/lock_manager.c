// SPDX-FileCopyrightText: © 2021 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/LockManager.zig for Oath, 2026-10-18.

#include "rivulet.h"

static int handle_lock_surfaces_timeout(void *data);
static void handle_lock(struct wl_listener *listener, void *data);
static void handle_unlock(struct wl_listener *listener, void *data);
static void handle_destroy(struct wl_listener *listener, void *data);
static void handle_surface(struct wl_listener *listener, void *data);
static void send_locked(struct rv_lock_manager *manager);

bool rv_lock_manager_init(struct rv_lock_manager *manager) {
	struct wl_event_loop *event_loop = wl_display_get_event_loop(server.wl_server);
	struct wl_event_source *timer = wl_event_loop_add_timer(event_loop, handle_lock_surfaces_timeout, manager);
	if (timer == NULL) {
		return false;
	}

	struct wlr_session_lock_manager_v1 *wlr_manager = wlr_session_lock_manager_v1_create(server.wl_server);
	if (wlr_manager == NULL) {
		wl_event_source_remove(timer);
		return false;
	}

	*manager = (struct rv_lock_manager){
		.wlr_manager = wlr_manager,
		.state = RV_LOCK_MANAGER_STATE_UNLOCKED,
		.lock = NULL,
		.lock_surfaces_timer = timer,
		.new_lock = { .notify = handle_lock },
		.unlock = { .notify = handle_unlock },
		.destroy = { .notify = handle_destroy },
		.new_surface = { .notify = handle_surface },
	};

	wl_signal_add(&manager->wlr_manager->events.new_lock, &manager->new_lock);
	return true;
}

void rv_lock_manager_deinit(struct rv_lock_manager *manager) {
	// deinit() should only be called after wl.Server.destroyClients()
	assert(manager->lock == NULL);

	wl_event_source_remove(manager->lock_surfaces_timer);

	wl_list_remove(&manager->new_lock.link);
}

static void handle_lock(struct wl_listener *listener, void *data) {
	struct rv_lock_manager *manager = wl_container_of(listener, manager, new_lock);
	struct wlr_session_lock_v1 *lock = data;

	rv_debug(RV_SCOPE_LOCK, "session lock client made lock request");

	if (manager->lock != NULL) {
		rv_info(RV_SCOPE_LOCK, "denying new session lock client, an active one already exists");
		wlr_session_lock_v1_destroy(lock);
		return;
	}

	manager->lock = lock;

	if (manager->state == RV_LOCK_MANAGER_STATE_UNLOCKED) {
		manager->state = RV_LOCK_MANAGER_STATE_WAITING_FOR_LOCK_SURFACES;

		assert(!server.scene.locked_tree->node.enabled);
		wlr_scene_node_set_enabled(&server.scene.locked_tree->node, true);

		if (wl_event_source_timer_update(manager->lock_surfaces_timer, 200) < 0) {
			rv_err(RV_SCOPE_LOCK, "error setting lock surfaces timer, imperfect frames may be shown");
			manager->state = RV_LOCK_MANAGER_STATE_WAITING_FOR_BLANK;
			// This call is necessary in the case that all outputs in the layout are disabled.
			rv_lock_manager_maybe_lock(manager);
		}

		{
			struct rv_seat *seat, *tmp;
			wl_list_for_each_safe(seat, tmp, &server.input_manager.seats, link) {
				rv_seat_focus(seat, (struct rv_seat_focus){ .tag = RV_SEAT_FOCUS_NONE });
			}
		}
	} else {
		if (manager->state == RV_LOCK_MANAGER_STATE_LOCKED) {
			wlr_session_lock_v1_send_locked(lock);
		}

		rv_info(RV_SCOPE_LOCK, "new session lock client given control of already locked session");
	}

	wl_signal_add(&lock->events.new_surface, &manager->new_surface);
	wl_signal_add(&lock->events.unlock, &manager->unlock);
	wl_signal_add(&lock->events.destroy, &manager->destroy);
}

static int handle_lock_surfaces_timeout(void *data) {
	struct rv_lock_manager *manager = data;

	rv_err(RV_SCOPE_LOCK, "waiting for lock surfaces timed out, imperfect frames may be shown");

	assert(manager->state == RV_LOCK_MANAGER_STATE_WAITING_FOR_LOCK_SURFACES);
	manager->state = RV_LOCK_MANAGER_STATE_WAITING_FOR_BLANK;

	wlr_scene_node_set_enabled(&server.scene.normal_tree->node, false);

	// This call is necessary in the case that all outputs in the layout are disabled.
	rv_lock_manager_maybe_lock(manager);

	return 0;
}

void rv_lock_manager_maybe_lock(struct rv_lock_manager *manager) {
	bool all_outputs_blanked = true;
	bool all_outputs_rendered_lock_surface = true;
	{
		struct rv_output *output, *tmp;
		wl_list_for_each_safe(output, tmp, &server.om.outputs, link) {
			struct wlr_output *wlr_output = output->wlr_output;
			if (wlr_output == NULL) {
				continue;
			}
			if (!wlr_output->enabled) {
				continue;
			}

			switch (output->lock_render_state) {
			case RV_OUTPUT_LOCK_RENDER_STATE_PENDING_UNLOCK:
			case RV_OUTPUT_LOCK_RENDER_STATE_UNLOCKED:
			case RV_OUTPUT_LOCK_RENDER_STATE_PENDING_BLANK:
			case RV_OUTPUT_LOCK_RENDER_STATE_PENDING_LOCK_SURFACE:
				all_outputs_blanked = false;
				all_outputs_rendered_lock_surface = false;
				break;
			case RV_OUTPUT_LOCK_RENDER_STATE_BLANKED:
				all_outputs_rendered_lock_surface = false;
				break;
			case RV_OUTPUT_LOCK_RENDER_STATE_LOCK_SURFACE:
				break;
			}
		}
	}

	switch (manager->state) {
	case RV_LOCK_MANAGER_STATE_WAITING_FOR_LOCK_SURFACES:
		if (all_outputs_rendered_lock_surface) {
			send_locked(manager);
			wlr_scene_node_set_enabled(&server.scene.normal_tree->node, false);
			(void)wl_event_source_timer_update(manager->lock_surfaces_timer, 0);
		}
		break;
	case RV_LOCK_MANAGER_STATE_WAITING_FOR_BLANK:
		if (all_outputs_blanked) {
			send_locked(manager);
		}
		break;
	case RV_LOCK_MANAGER_STATE_UNLOCKED:
	case RV_LOCK_MANAGER_STATE_LOCKED:
		abort(); // unreachable
	}
}

static void send_locked(struct rv_lock_manager *manager) {
	rv_info(RV_SCOPE_LOCK, "session locked");
	// The lock client may have been destroyed, for example due to a protocol error.
	if (manager->lock != NULL) {
		wlr_session_lock_v1_send_locked(manager->lock);
	}
	manager->state = RV_LOCK_MANAGER_STATE_LOCKED;
	rv_window_manager_dirty_windowing(&server.wm);
}

static void handle_unlock(struct wl_listener *listener, void *data) {
	struct rv_lock_manager *manager = wl_container_of(listener, manager, unlock);

	manager->state = RV_LOCK_MANAGER_STATE_UNLOCKED;

	rv_info(RV_SCOPE_LOCK, "session unlocked");

	assert(!server.scene.normal_tree->node.enabled);
	wlr_scene_node_set_enabled(&server.scene.normal_tree->node, true);

	assert(server.scene.locked_tree->node.enabled);
	wlr_scene_node_set_enabled(&server.scene.locked_tree->node, false);

	{
		struct rv_seat *seat, *tmp;
		wl_list_for_each_safe(seat, tmp, &server.input_manager.seats, link) {
			rv_seat_focus(seat, (struct rv_seat_focus){ .tag = RV_SEAT_FOCUS_NONE });
		}
	}

	handle_destroy(&manager->destroy, NULL);

	rv_window_manager_dirty_windowing(&server.wm);
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_lock_manager *manager = wl_container_of(listener, manager, destroy);

	rv_debug(RV_SCOPE_LOCK, "ext_session_lock_v1 destroyed");

	wl_list_remove(&manager->new_surface.link);
	wl_list_remove(&manager->unlock.link);
	wl_list_remove(&manager->destroy.link);

	manager->lock = NULL;
	if (manager->state == RV_LOCK_MANAGER_STATE_WAITING_FOR_LOCK_SURFACES) {
		manager->state = RV_LOCK_MANAGER_STATE_WAITING_FOR_BLANK;
		(void)wl_event_source_timer_update(manager->lock_surfaces_timer, 0);
	}
}

static void handle_surface(struct wl_listener *listener, void *data) {
	struct rv_lock_manager *manager = wl_container_of(listener, manager, new_surface);
	struct wlr_session_lock_surface_v1 *wlr_lock_surface = data;

	rv_debug(RV_SCOPE_LOCK, "new ext_session_lock_surface_v1 created");

	assert(manager->state != RV_LOCK_MANAGER_STATE_UNLOCKED);
	assert(manager->lock != NULL);

	if (!rv_lock_surface_create(wlr_lock_surface, manager->lock)) {
		rv_err(RV_SCOPE_LOCK, "out of memory");
		wl_resource_post_no_memory(wlr_lock_surface->resource);
	}
}

struct rv_lock_surface *rv_lock_manager_lock_surface_from_output(struct rv_lock_manager *manager,
		struct rv_output *output) {
	struct wlr_session_lock_v1 *lock = manager->lock;
	if (lock == NULL) {
		return NULL;
	}

	struct wlr_session_lock_surface_v1 *wlr_lock_surface, *tmp;
	wl_list_for_each_safe(wlr_lock_surface, tmp, &lock->surfaces, link) {
		struct rv_lock_surface *lock_surface = wlr_lock_surface->data;
		if (output == rv_lock_surface_get_output(lock_surface)) {
			return lock_surface;
		}
	}

	return NULL;
}
