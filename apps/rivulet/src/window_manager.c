// SPDX-FileCopyrightText: © 2024 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/WindowManager.zig for Oath, 2026-10-18.

#include "rivulet.h"

static void handle_server_destroy(struct wl_listener *listener, void *data);
static void bind(struct wl_client *client, void *data, uint32_t version, uint32_t id);
static int handle_timeout(void *data);
static void add_dirty_idle(struct rv_window_manager *wm);
static void remove_dirty_idle(struct rv_window_manager *wm);
static void manage_start(struct rv_window_manager *wm);
static void start_timeout_timer(struct rv_window_manager *wm, int32_t ms);
static void cancel_timeout_timer(struct rv_window_manager *wm);
static void render_start(struct rv_window_manager *wm);
static void render_finish(struct rv_window_manager *wm);

static const struct river_window_manager_v1_interface wm_impl;
static const struct river_window_manager_v1_interface wm_inert_impl;
static void handle_destroy(struct wl_resource *wm_v1);

bool rv_window_manager_init(struct rv_window_manager *wm) {
	struct wl_event_loop *event_loop = wl_display_get_event_loop(server.wl_server);
	struct wl_event_source *timeout = wl_event_loop_add_timer(event_loop, handle_timeout, wm);
	if (timeout == NULL) {
		return false;
	}

	struct wl_global *global =
		wl_global_create(server.wl_server, &river_window_manager_v1_interface, 4, wm, bind);
	if (global == NULL) {
		wl_event_source_remove(timeout);
		return false;
	}

	*wm = (struct rv_window_manager){
		.global = global,
		.object = NULL,
		.state = { .tag = RV_WINDOW_MANAGER_STATE_IDLE },
		.windows = rv_slotmap_empty,
		.scheduled = { .dirty = false, .dirty_lazy = false, .output_config = NULL },
		.sent = { .session_locked = false, .output_config = NULL },
		.rendering_scheduled = { .dirty = false },
		.rendering_requested = { .order_hash = 0 },
		.dirty_idle = NULL,
		.timeout = timeout,
	};
	wm->server_destroy.notify = handle_server_destroy;
	wl_list_init(&wm->sent.outputs);
	wl_list_init(&wm->sent.seats);
	wl_list_init(&wm->rendering_requested.list);

	wl_display_add_destroy_listener(server.wl_server, &wm->server_destroy);
	return true;
}

static void handle_server_destroy(struct wl_listener *listener, void *data) {
	struct rv_window_manager *wm = wl_container_of(listener, wm, server_destroy);

	wl_global_destroy(wm->global);
	wl_event_source_remove(wm->timeout);
}

static void bind(struct wl_client *client, void *data, uint32_t version, uint32_t id) {
	struct rv_window_manager *wm = data;
	struct wl_resource *object = wl_resource_create(client, &river_window_manager_v1_interface, (int)version, id);
	if (object == NULL) {
		wl_client_post_no_memory(client);
		rv_err(RV_SCOPE_WM, "out of memory");
		return;
	}

	if (wm->object != NULL) {
		river_window_manager_v1_send_unavailable(object);
		wl_resource_set_implementation(object, &wm_inert_impl, NULL, NULL);
		return;
	}

	wm->object = object;
	wl_resource_set_implementation(object, &wm_impl, wm, handle_destroy);
	rv_window_manager_dirty_windowing(wm);
}

// handleRequestInert: only destroy does anything.

static void inert_stop(struct wl_client *client, struct wl_resource *wm_v1) {
}

static void inert_destroy(struct wl_client *client, struct wl_resource *wm_v1) {
	wl_resource_destroy(wm_v1);
}

static void inert_manage_finish(struct wl_client *client, struct wl_resource *wm_v1) {
}

static void inert_manage_dirty(struct wl_client *client, struct wl_resource *wm_v1) {
}

static void inert_render_finish(struct wl_client *client, struct wl_resource *wm_v1) {
}

static void inert_get_shell_surface(struct wl_client *client, struct wl_resource *wm_v1, uint32_t id,
		struct wl_resource *surface) {
}

static void inert_exit_session(struct wl_client *client, struct wl_resource *wm_v1) {
}

static const struct river_window_manager_v1_interface wm_inert_impl = {
	.stop = inert_stop,
	.destroy = inert_destroy,
	.manage_finish = inert_manage_finish,
	.manage_dirty = inert_manage_dirty,
	.render_finish = inert_render_finish,
	.get_shell_surface = inert_get_shell_surface,
	.exit_session = inert_exit_session,
};

static void handle_destroy(struct wl_resource *wm_v1) {
	struct rv_window_manager *wm = wl_resource_get_user_data(wm_v1);
	rv_debug(RV_SCOPE_WM, "active river_window_manager_v1 destroyed");
	wm->object = NULL;
	{
		struct rv_output *output, *tmp;
		wl_list_for_each_safe(output, tmp, &server.om.outputs, link) {
			rv_output_make_inert(output);
		}
	}
	{
		struct rv_seat *seat, *tmp;
		wl_list_for_each_safe(seat, tmp, &server.input_manager.seats, link) {
			rv_seat_make_inert(seat);
		}
	}
	{
		struct rv_slotmap_iterator it = rv_slotmap_iterator(&wm->windows);
		struct rv_window *window;
		while ((window = rv_slotmap_iterator_next(&it)) != NULL) {
			rv_window_make_inert(window);
		}
	}
	switch (wm->state.tag) {
	case RV_WINDOW_MANAGER_STATE_IDLE:
		break;
	case RV_WINDOW_MANAGER_STATE_INFLIGHT_CONFIGURES:
		break;
	case RV_WINDOW_MANAGER_STATE_MANAGE:
		rv_window_manager_manage_finish(wm);
		break;
	case RV_WINDOW_MANAGER_STATE_RENDER:
		render_finish(wm);
		break;
	}
}

// handleRequest: one function per request.

static struct rv_window_manager *wm_from_resource(struct wl_resource *wm_v1) {
	struct rv_window_manager *wm = wl_resource_get_user_data(wm_v1);
	assert(wm->object == wm_v1);
	return wm;
}

static void request_stop(struct wl_client *client, struct wl_resource *wm_v1) {
	struct rv_window_manager *wm = wm_from_resource(wm_v1);
	wm->object = NULL;
	river_window_manager_v1_send_finished(wm_v1);
	wl_resource_set_implementation(wm_v1, &wm_inert_impl, NULL, NULL);
}

static void request_destroy(struct wl_client *client, struct wl_resource *wm_v1) {
	wm_from_resource(wm_v1);
	// TODO send protocol error to avoid leak on race
	wl_resource_destroy(wm_v1);
}

static void request_manage_finish(struct wl_client *client, struct wl_resource *wm_v1) {
	struct rv_window_manager *wm = wm_from_resource(wm_v1);
	if (wm->state.tag != RV_WINDOW_MANAGER_STATE_MANAGE) {
		wl_resource_post_error(wm_v1, RIVER_WINDOW_MANAGER_V1_ERROR_SEQUENCE_ORDER, "%s",
			"manage_finish request does not match manage_start");
		return;
	}
	rv_window_manager_manage_finish(wm);
}

static void request_manage_dirty(struct wl_client *client, struct wl_resource *wm_v1) {
	struct rv_window_manager *wm = wm_from_resource(wm_v1);
	wm->scheduled.dirty_lazy = true;
	add_dirty_idle(wm);
}

static void request_render_finish(struct wl_client *client, struct wl_resource *wm_v1) {
	struct rv_window_manager *wm = wm_from_resource(wm_v1);
	if (wm->state.tag != RV_WINDOW_MANAGER_STATE_RENDER) {
		wl_resource_post_error(wm_v1, RIVER_WINDOW_MANAGER_V1_ERROR_SEQUENCE_ORDER, "%s",
			"render_finish request does not match render_start");
		return;
	}
	render_finish(wm);
}

static void request_get_shell_surface(struct wl_client *client, struct wl_resource *wm_v1, uint32_t id,
		struct wl_resource *surface_resource) {
	wm_from_resource(wm_v1);
	struct wlr_surface *surface = wlr_surface_from_resource(surface_resource);
	if (!rv_shell_surface_create(wl_resource_get_client(wm_v1), (uint32_t)wl_resource_get_version(wm_v1), id,
			surface)) {
		wl_client_post_no_memory(wl_resource_get_client(wm_v1));
		rv_err(RV_SCOPE_WM, "out of memory");
		return;
	}
}

static void request_exit_session(struct wl_client *client, struct wl_resource *wm_v1) {
	wm_from_resource(wm_v1);
	rv_info(RV_SCOPE_WM, "window manager requested to exit session");
	wl_display_terminate(server.wl_server);
}

static const struct river_window_manager_v1_interface wm_impl = {
	.stop = request_stop,
	.destroy = request_destroy,
	.manage_finish = request_manage_finish,
	.manage_dirty = request_manage_dirty,
	.render_finish = request_render_finish,
	.get_shell_surface = request_get_shell_surface,
	.exit_session = request_exit_session,
};

bool rv_window_manager_ensure_windowing(struct rv_window_manager *wm) {
	switch (wm->state.tag) {
	case RV_WINDOW_MANAGER_STATE_MANAGE:
		return true;
	case RV_WINDOW_MANAGER_STATE_IDLE:
	case RV_WINDOW_MANAGER_STATE_INFLIGHT_CONFIGURES:
	case RV_WINDOW_MANAGER_STATE_RENDER:
		if (wm->object != NULL) {
			wl_resource_post_error(wm->object, RIVER_WINDOW_MANAGER_V1_ERROR_SEQUENCE_ORDER, "%s",
				"invalid modification of window management state");
		}
		return false;
	}
	abort(); // unreachable
}

bool rv_window_manager_ensure_rendering(struct rv_window_manager *wm) {
	switch (wm->state.tag) {
	case RV_WINDOW_MANAGER_STATE_MANAGE:
	case RV_WINDOW_MANAGER_STATE_INFLIGHT_CONFIGURES:
	case RV_WINDOW_MANAGER_STATE_RENDER:
		return true;
	case RV_WINDOW_MANAGER_STATE_IDLE:
		if (wm->object != NULL) {
			wl_resource_post_error(wm->object, RIVER_WINDOW_MANAGER_V1_ERROR_SEQUENCE_ORDER, "%s",
				"invalid modification of rendering state");
		}
		return false;
	}
	abort(); // unreachable
}

void rv_window_manager_dirty_windowing(struct rv_window_manager *wm) {
	wm->scheduled.dirty = true;
	add_dirty_idle(wm);
}

void rv_window_manager_dirty_windowing_lazy(struct rv_window_manager *wm) {
	wm->scheduled.dirty_lazy = true;
	add_dirty_idle(wm);
}

void rv_window_manager_clean_windowing(struct rv_window_manager *wm) {
	wm->scheduled.dirty = false;
	remove_dirty_idle(wm);
}

void rv_window_manager_dirty_rendering(struct rv_window_manager *wm) {
	wm->rendering_scheduled.dirty = true;
	add_dirty_idle(wm);
}

void rv_window_manager_clean_rendering(struct rv_window_manager *wm) {
	wm->rendering_scheduled.dirty = false;
	remove_dirty_idle(wm);
}

static void dirty_idle(void *data);

static void add_dirty_idle(struct rv_window_manager *wm) {
	assert(wm->scheduled.dirty || wm->scheduled.dirty_lazy || wm->rendering_scheduled.dirty);
	if (wm->dirty_idle == NULL) {
		struct wl_event_loop *event_loop = wl_display_get_event_loop(server.wl_server);
		wm->dirty_idle = wl_event_loop_add_idle(event_loop, dirty_idle, wm);
		if (wm->dirty_idle == NULL) {
			rv_err(RV_SCOPE_WM, "out of memory");
			return;
		}
	}
}

static void remove_dirty_idle(struct rv_window_manager *wm) {
	if (!wm->scheduled.dirty && !wm->rendering_scheduled.dirty) {
		if (wm->dirty_idle != NULL) {
			wl_event_source_remove(wm->dirty_idle);
			wm->dirty_idle = NULL;
		}
	}
}

static void dirty_idle(void *data) {
	struct rv_window_manager *wm = data;
	assert(wm->scheduled.dirty || wm->scheduled.dirty_lazy || wm->rendering_scheduled.dirty);
	wm->dirty_idle = NULL;
	switch (wm->state.tag) {
	case RV_WINDOW_MANAGER_STATE_IDLE:
		if (wm->rendering_scheduled.dirty) {
			render_start(wm);
		} else {
			assert(wm->scheduled.dirty || wm->scheduled.dirty_lazy);
			wm->scheduled.dirty = true;
			wm->scheduled.dirty_lazy = false;
			manage_start(wm);
		}
		break;
	case RV_WINDOW_MANAGER_STATE_MANAGE:
	case RV_WINDOW_MANAGER_STATE_INFLIGHT_CONFIGURES:
	case RV_WINDOW_MANAGER_STATE_RENDER:
		break;
	}
}

static void manage_start(struct rv_window_manager *wm) {
	assert(wm->state.tag == RV_WINDOW_MANAGER_STATE_IDLE);
	assert(wm->scheduled.dirty);
	rv_window_manager_clean_windowing(wm);
	wm->state = (struct rv_window_manager_state){ .tag = RV_WINDOW_MANAGER_STATE_MANAGE };

	rv_debug(RV_SCOPE_WM, "manage sequence start");

	const bool session_locked = server.lock_manager.state == RV_LOCK_MANAGER_STATE_LOCKED;
	if (session_locked != wm->sent.session_locked) {
		if (wm->object != NULL) {
			if (session_locked) {
				river_window_manager_v1_send_session_locked(wm->object);
			} else {
				river_window_manager_v1_send_session_unlocked(wm->object);
			}
		}
		wm->sent.session_locked = session_locked;
	}

	rv_output_manager_auto_layout(&server.om);
	{
		struct rv_output *output, *tmp;
		wl_list_for_each_safe(output, tmp, &server.om.outputs, link) {
			rv_output_manage_start(output);
		}
	}

	assert(wm->sent.output_config == NULL);
	wm->sent.output_config = wm->scheduled.output_config;
	wm->scheduled.output_config = NULL;

	{
		struct rv_slotmap_iterator it = rv_slotmap_iterator(&wm->windows);
		struct rv_window *window;
		while ((window = rv_slotmap_iterator_next(&it)) != NULL) {
			rv_window_manage_start(window);
		}
	}

	{
		struct rv_seat *seat, *tmp;
		wl_list_for_each_safe(seat, tmp, &server.input_manager.seats, link) {
			rv_seat_manage_start(seat);
		}
	}

	if (wm->object != NULL) {
		river_window_manager_v1_send_manage_start(wm->object);
		start_timeout_timer(wm, 3000);
	} else {
		rv_window_manager_manage_finish(wm);
	}
}

void rv_window_manager_manage_finish(struct rv_window_manager *wm) {
	assert(wm->state.tag == RV_WINDOW_MANAGER_STATE_MANAGE);
	cancel_timeout_timer(wm);

	rv_debug(RV_SCOPE_WM, "manage sequence finish");

	{
		// Order is important here, Seat.manageFinish() must be called
		// before Window.manageFinish().
		struct rv_seat *seat, *tmp;
		wl_list_for_each_safe(seat, tmp, &wm->sent.seats, link_sent) {
			rv_seat_manage_finish(seat);
		}
	}

	wm->state = (struct rv_window_manager_state){
		.tag = RV_WINDOW_MANAGER_STATE_INFLIGHT_CONFIGURES,
		.inflight_configures = 0,
	};
	{
		struct rv_wm_node *node, *tmp;
		wl_list_for_each_safe(node, tmp, &wm->rendering_requested.list, link) {
			struct rv_wm_node_type type = rv_wm_node_get(node);
			switch (type.tag) {
			case RV_WM_NODE_TAG_WINDOW:
				if (rv_window_manage_finish(type.window)) {
					wm->state.inflight_configures += 1;
				}
				break;
			case RV_WM_NODE_TAG_SHELL_SURFACE:
				break;
			}
		}
	}

	rv_debug(RV_SCOPE_WM, "sent %u tracked configure(s)", wm->state.inflight_configures);

	if (wm->state.inflight_configures > 0) {
		start_timeout_timer(wm, 100);
	} else {
		render_start(wm);
	}
}

static void start_timeout_timer(struct rv_window_manager *wm, int32_t ms) {
	if (wl_event_source_timer_update(wm->timeout, ms) < 0) {
		rv_err(RV_SCOPE_WM, "failed to start timer");
		(void)handle_timeout(wm);
	}
}

static void cancel_timeout_timer(struct rv_window_manager *wm) {
	if (wl_event_source_timer_update(wm->timeout, 0) < 0) {
		rv_err(RV_SCOPE_WM, "error disarming timer");
	}
}

static int handle_timeout(void *data) {
	struct rv_window_manager *wm = data;
	switch (wm->state.tag) {
	case RV_WINDOW_MANAGER_STATE_INFLIGHT_CONFIGURES:
		rv_err(RV_SCOPE_WM, "timeout occurred, some imperfect frames may be shown");
		assert(wm->state.inflight_configures > 0);
		wm->state.inflight_configures = 0;

		render_start(wm);
		break;
	case RV_WINDOW_MANAGER_STATE_MANAGE:
	case RV_WINDOW_MANAGER_STATE_RENDER:
		if (wm->object != NULL) {
			struct wl_resource *wm_v1 = wm->object;
			rv_err(RV_SCOPE_WM, "window manager unresponsive for more than 3 seconds, disconnecting");
			wl_resource_post_error(wm_v1, RIVER_WINDOW_MANAGER_V1_ERROR_UNRESPONSIVE, "%s",
				"unresponsive for more than 3 seconds");
			// Don't wait for the frozen client to receive the protocol error
			// and exit of its own accord.
			wl_client_destroy(wl_resource_get_client(wm_v1));
		}
		break;
	case RV_WINDOW_MANAGER_STATE_IDLE:
		abort(); // unreachable
	}

	return 0;
}

void rv_window_manager_notify_configured(struct rv_window_manager *wm) {
	// Zig checks the active union field and integer underflow here.
	assert(wm->state.tag == RV_WINDOW_MANAGER_STATE_INFLIGHT_CONFIGURES);
	assert(wm->state.inflight_configures > 0);
	wm->state.inflight_configures -= 1;
	if (wm->state.inflight_configures == 0) {
		cancel_timeout_timer(wm);
		render_start(wm);
	}
}

static void render_start(struct rv_window_manager *wm) {
	// Zig reads state.inflight_configures here, which is a safety-checked
	// panic unless the state is .inflight_configures.
	assert((wm->state.tag == RV_WINDOW_MANAGER_STATE_IDLE && wm->rendering_scheduled.dirty) ||
		(wm->state.tag == RV_WINDOW_MANAGER_STATE_INFLIGHT_CONFIGURES && wm->state.inflight_configures == 0));
	wm->state = (struct rv_window_manager_state){ .tag = RV_WINDOW_MANAGER_STATE_RENDER };
	rv_window_manager_clean_rendering(wm);

	rv_debug(RV_SCOPE_WM, "render sequence start");

	{
		struct rv_wm_node *node, *tmp;
		wl_list_for_each_safe(node, tmp, &wm->rendering_requested.list, link) {
			struct rv_wm_node_type type = rv_wm_node_get(node);
			switch (type.tag) {
			case RV_WM_NODE_TAG_WINDOW:
				rv_window_render_start(type.window);
				break;
			case RV_WM_NODE_TAG_SHELL_SURFACE:
				break;
			}
		}
	}

	if (wm->object != NULL) {
		river_window_manager_v1_send_render_start(wm->object);
		start_timeout_timer(wm, 3000);
	} else {
		render_finish(wm);
	}
}

// FNV-1a 64-bit. River hashes the render order with Blake3 only to compare it
// against the previous value; nothing else depends on the hash value, so the
// C port hashes exactly the same input bytes with FNV-1a instead.
#define FNV1A_64_OFFSET_BASIS UINT64_C(0xcbf29ce484222325)
#define FNV1A_64_PRIME UINT64_C(0x100000001b3)

static void fnv1a_64_update(uint64_t *hash, const void *bytes, size_t len) {
	const uint8_t *p = bytes;
	for (size_t i = 0; i < len; i++) {
		*hash ^= p[i];
		*hash *= FNV1A_64_PRIME;
	}
}

// Finish the update sequence and drop stashed buffers. This means that
// the next frame drawn will be the post-transaction state.
static void render_finish(struct rv_window_manager *wm) {
	assert(wm->state.tag == RV_WINDOW_MANAGER_STATE_RENDER);
	wm->state = (struct rv_window_manager_state){ .tag = RV_WINDOW_MANAGER_STATE_IDLE };

	cancel_timeout_timer(wm);

	rv_debug(RV_SCOPE_WM, "render sequence finish");

	{
		struct rv_slotmap_iterator it = rv_slotmap_iterator(&wm->windows);
		struct rv_window *window;
		while ((window = rv_slotmap_iterator_next(&it)) != NULL) {
			// If a window is unmapped during a render sequence, we need to retain the saved
			// buffers until after the next manage sequence (in which the closed event will
			// be sent) for frame perfection.
			if (window->state != RV_WINDOW_STATE_CLOSING) {
				rv_scene_saveable_surfaces_drop_saved(&window->surfaces);
			}
			// Ensure windows that are closed but not yet destroyed don't have
			// their borders/decorations rendered.
			if (window->state == RV_WINDOW_STATE_INIT) {
				wlr_scene_node_reparent(&window->tree->node, server.scene.hidden_tree);
			}
			if (window->impl.tag == RV_WINDOW_IMPL_DESTROYING) {
				rv_window_destroy(window);
			}
		}
	}

	// This is a hack to avoid excessive modification of the wlroots scene graph.
	// There is currently no way to atomically apply multiple changes to the
	// scene graph, which means that damage and visibility are re-calculated
	// every API call, resulting in redundant events being sent to clients.
	//
	// TODO(wlroots) provide a way to batch changes to the scene graph.
	uint64_t new_order_hash = FNV1A_64_OFFSET_BASIS;
	{
		struct rv_wm_node *node, *tmp;
		wl_list_for_each_safe(node, tmp, &wm->rendering_requested.list, link) {
			struct rv_wm_node_type type = rv_wm_node_get(node);
			switch (type.tag) {
			case RV_WM_NODE_TAG_WINDOW: {
				struct rv_window *window = type.window;
				// The bytes of the packed Window.Ref (generation, then index).
				fnv1a_64_update(&new_order_hash, &window->ref, sizeof(window->ref));
				const uint8_t fullscreen = window->wm_requested.fullscreen != NULL;
				fnv1a_64_update(&new_order_hash, &fullscreen, 1);
				break;
			}
			case RV_WM_NODE_TAG_SHELL_SURFACE: {
				// The bytes of the *ShellSurface pointer.
				struct rv_shell_surface *shell_surface = type.shell_surface;
				fnv1a_64_update(&new_order_hash, &shell_surface, sizeof(shell_surface));
				break;
			}
			}
		}
	}

	{
		const bool reorder = wm->rendering_requested.order_hash != new_order_hash;
		wm->rendering_requested.order_hash = new_order_hash;

		bool found_fullscreen = false;
		struct rv_wm_node *node, *tmp;
		wl_list_for_each_safe(node, tmp, &wm->rendering_requested.list, link) {
			struct rv_wm_node_type type = rv_wm_node_get(node);
			switch (type.tag) {
			case RV_WM_NODE_TAG_WINDOW: {
				struct rv_window *window = type.window;
				rv_window_render_finish(window);
				if (!reorder) {
					continue;
				}
				wlr_scene_node_reparent(&window->popup_tree->node, server.scene.layers.popups);
				if (window->wm_requested.fullscreen != NULL) {
					wlr_scene_node_reparent(&window->tree->node, server.scene.layers.fullscreen);
					wlr_scene_node_raise_to_top(&window->tree->node);
					found_fullscreen = true;
				} else {
					wlr_scene_node_reparent(&window->tree->node, server.scene.layers.wm);
					wlr_scene_node_raise_to_top(&window->tree->node);
				}
				break;
			}
			case RV_WM_NODE_TAG_SHELL_SURFACE: {
				struct rv_shell_surface *shell_surface = type.shell_surface;
				rv_shell_surface_render_finish(shell_surface);
				if (!reorder) {
					continue;
				}
				wlr_scene_node_reparent(&shell_surface->popup_tree->node, server.scene.layers.popups);
				if (found_fullscreen) {
					wlr_scene_node_reparent(&shell_surface->tree->node, server.scene.layers.fullscreen);
				} else {
					wlr_scene_node_reparent(&shell_surface->tree->node, server.scene.layers.wm);
				}
				wlr_scene_node_raise_to_top(&shell_surface->tree->node);
				break;
			}
			}
		}
	}

	rv_output_manager_commit_output_state(&server.om);

	{
		struct rv_seat *seat, *tmp;
		wl_list_for_each_safe(seat, tmp, &server.input_manager.seats, link) {
			rv_cursor_update_state(&seat->cursor);
		}
	}

	rv_idle_inhibit_manager_check_active(&server.idle_inhibit_manager);

	rv_debug(RV_SCOPE_WM, "finished committing transaction");

	if (wm->scheduled.dirty || wm->scheduled.dirty_lazy || wm->rendering_scheduled.dirty) {
		add_dirty_idle(wm);
	}

	rv_input_manager_process_events(&server.input_manager);
}
