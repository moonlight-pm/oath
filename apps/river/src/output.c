// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Output.zig for Oath, 2026-10-18.

#include "river.h"

#include <math.h>

static void handle_destroy(struct wl_listener *listener, void *data);
static void handle_request_state(struct wl_listener *listener, void *data);
static void handle_frame(struct wl_listener *listener, void *data);
static void handle_present(struct wl_listener *listener, void *data);

// ---------------------------------------------------------------------------
// Output.State
// ---------------------------------------------------------------------------

struct rv_output_state rv_output_state_from_head_state(const struct wlr_output_head_v1_state *state) {
	assert(state->enabled);
	struct rv_output_state result = {
		.state = RV_OUTPUT_STATE_STATE_ENABLED,
		.x = state->x,
		.y = state->y,
		// Round to nearest 1/120 to ensure the scale is exactly represented
		// in the fractional-scale-v1 protocol.
		.scale = roundf(state->scale * 120) / 120,
		.transform = state->transform,
		.adaptive_sync = state->adaptive_sync_enabled,
		.auto_layout = false,
	};
	if (state->mode != NULL) {
		result.mode.tag = RV_OUTPUT_STATE_MODE_STANDARD;
		result.mode.standard = state->mode;
	} else {
		result.mode.tag = RV_OUTPUT_STATE_MODE_CUSTOM;
		result.mode.custom = (struct rv_output_state_mode_custom){
			.width = state->custom_mode.width,
			.height = state->custom_mode.height,
			.refresh = state->custom_mode.refresh,
		};
	}
	return result;
}

// Width/height in the logical coordinate space
void rv_output_state_dimensions(const struct rv_output_state *state, int32_t *width, int32_t *height) {
	int32_t w, h;
	switch (state->mode.tag) {
	case RV_OUTPUT_STATE_MODE_STANDARD:
		w = state->mode.standard->width;
		h = state->mode.standard->height;
		break;
	case RV_OUTPUT_STATE_MODE_CUSTOM:
		w = state->mode.custom.width;
		h = state->mode.custom.height;
		break;
	case RV_OUTPUT_STATE_MODE_NONE:
	default:
		w = 0;
		h = 0;
		break;
	}
	if ((int)state->transform % 2 != 0) {
		int32_t tmp = w;
		w = h;
		h = tmp;
	}
	*width = (int32_t)((float)w / state->scale);
	*height = (int32_t)((float)h / state->scale);
}

struct wlr_box rv_output_state_box(const struct rv_output_state *state) {
	int32_t w, h;
	rv_output_state_dimensions(state, &w, &h);
	return (struct wlr_box){ .x = state->x, .y = state->y, .width = w, .height = h };
}

void rv_output_state_apply_no_modeset(const struct rv_output_state *state, struct wlr_output_state *wlr_state) {
	wlr_output_state_set_scale(wlr_state, state->scale);
	wlr_output_state_set_transform(wlr_state, state->transform);
}

void rv_output_state_apply_modeset(const struct rv_output_state *state, struct wlr_output_state *wlr_state) {
	const bool enabled = state->state == RV_OUTPUT_STATE_STATE_ENABLED;
	wlr_output_state_set_enabled(wlr_state, enabled);
	if (!enabled) {
		return;
	}
	rv_output_state_apply_no_modeset(state, wlr_state);
	switch (state->mode.tag) {
	case RV_OUTPUT_STATE_MODE_STANDARD:
		wlr_output_state_set_mode(wlr_state, state->mode.standard);
		break;
	case RV_OUTPUT_STATE_MODE_CUSTOM:
		wlr_output_state_set_custom_mode(wlr_state, state->mode.custom.width, state->mode.custom.height,
			state->mode.custom.refresh);
		break;
	case RV_OUTPUT_STATE_MODE_NONE:
		break;
	}
	wlr_output_state_set_adaptive_sync_enabled(wlr_state, state->adaptive_sync);
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

bool rv_output_create(struct wlr_output *wlr_output, bool *init_render_failed) {
	*init_render_failed = false;

	struct rv_output *output = calloc(1, sizeof(*output));
	if (output == NULL) {
		return false;
	}

	{
		const char *prefix = "river - ";
		size_t len = strlen(prefix) + strlen(wlr_output->name) + 1;
		char *title = malloc(len);
		if (title == NULL) {
			free(output);
			return false;
		}
		snprintf(title, len, "%s%s", prefix, wlr_output->name);
		if (wlr_output_is_wl(wlr_output)) {
			wlr_wl_output_set_app_id(wlr_output, "river");
			wlr_wl_output_set_title(wlr_output, title);
		}
#if WLR_HAS_X11_BACKEND
		else if (wlr_output_is_x11(wlr_output)) {
			wlr_x11_output_set_title(wlr_output, title);
		}
#endif
		free(title);
	}

	if (!wlr_output_init_render(wlr_output, server.allocator, server.renderer)) {
		free(output);
		*init_render_failed = true;
		return false;
	}

	struct wlr_scene_output *scene_output = wlr_scene_output_create(server.scene.wlr_scene, wlr_output);
	if (scene_output == NULL) {
		free(output);
		return false;
	}

	const struct rv_output_state initial = {
		.state = RV_OUTPUT_STATE_STATE_DISABLED_HARD,
		.x = 0,
		.y = 0,
		.mode = { .tag = RV_OUTPUT_STATE_MODE_NONE },
		.scale = 1,
		.transform = WL_OUTPUT_TRANSFORM_NORMAL,
		.adaptive_sync = wlr_output->adaptive_sync_status == WLR_OUTPUT_ADAPTIVE_SYNC_ENABLED,
		.auto_layout = true,
	};

	output->wlr_output = wlr_output;
	output->scene_output = scene_output;
	output->object = NULL;
	// layer_shell: .{} is all-zero (object null, sent.non_exclusive_area null,
	// requested.default false).
	output->lock_render_state = RV_OUTPUT_LOCK_RENDER_STATE_BLANKED;
	output->scheduled = initial;
	output->sent = initial;
	output->sent_wl_output = false;
	output->rendering_requested = (struct rv_output_rendering_state){ .tearing = false };
	output->current = initial;
	output->rendering_current = (struct rv_output_rendering_state){ .tearing = false };
	output->destroy.notify = handle_destroy;
	output->request_state.notify = handle_request_state;
	output->frame.notify = handle_frame;
	output->present.notify = handle_present;

	wlr_output->data = output;

	wl_list_insert(server.om.outputs.prev, &output->link);
	wl_list_init(&output->link_sent);

	wl_signal_add(&wlr_output->events.destroy, &output->destroy);
	wl_signal_add(&wlr_output->events.request_state, &output->request_state);
	wl_signal_add(&wlr_output->events.frame, &output->frame);
	wl_signal_add(&wlr_output->events.present, &output->present);

	output->scheduled.state = RV_OUTPUT_STATE_STATE_ENABLED;
	struct wlr_output_mode *preferred_mode = wlr_output_preferred_mode(wlr_output);
	if (preferred_mode != NULL) {
		output->scheduled.mode.tag = RV_OUTPUT_STATE_MODE_STANDARD;
		output->scheduled.mode.standard = preferred_mode;
	} else {
		// The output does not support modes (i.e. we are not using the DRM backend)
		// Currently, wlroots does not make it possible for us know the dimensions
		// requested by the host compositor in the first configure event. Therefore,
		// we can't do anything but guess until the second configure is sent and
		// wlroots emits the wlr_output request_state event.
		// TODO(wlroots): fix this API limitation, for context see
		// https://gitlab.freedesktop.org/wlroots/wlroots/-/merge_requests/4963
		output->scheduled.mode.tag = RV_OUTPUT_STATE_MODE_CUSTOM;
		output->scheduled.mode.custom = (struct rv_output_state_mode_custom){
			.width = 1280,
			.height = 720,
			.refresh = 0,
		};
	}

	rv_window_manager_dirty_windowing(&server.wm);

	return true;
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_output *output = wl_container_of(listener, output, destroy);
	struct wlr_output *wlr_output = data;

	rv_debug(RV_SCOPE_OUTPUT, "wlr_output '%s' destroyed", wlr_output->name);

	{
		struct rv_slotmap_iterator it = rv_slotmap_iterator(&server.layer_shell.surfaces);
		struct rv_layer_surface *surface;
		while ((surface = rv_slotmap_iterator_next(&it)) != NULL) {
			if (surface->wlr_layer_surface->output == wlr_output) {
				wlr_layer_surface_v1_destroy(surface->wlr_layer_surface);
			}
		}
	}
	{
		struct rv_input_device *device, *tmp;
		wl_list_for_each_safe(device, tmp, &server.input_manager.devices, link) {
			if (device->config.map_to_output == wlr_output) {
				device->config.map_to_output = NULL;
				wlr_cursor_map_input_to_output(device->seat->cursor.wlr_cursor, device->wlr_device, NULL);
			}
		}
	}

	wl_list_remove(&output->destroy.link);
	wl_list_remove(&output->request_state.link);
	wl_list_remove(&output->frame.link);
	wl_list_remove(&output->present.link);

	wlr_output->data = NULL;

	output->wlr_output = NULL;
	output->scene_output = NULL;
	output->scheduled.mode.tag = RV_OUTPUT_STATE_MODE_NONE;
	output->sent.mode.tag = RV_OUTPUT_STATE_MODE_NONE;
	output->current.mode.tag = RV_OUTPUT_STATE_MODE_NONE;
	output->scheduled.state = RV_OUTPUT_STATE_STATE_DESTROYING;

	rv_window_manager_dirty_windowing(&server.wm);
}

static void request_destroy(struct wl_client *client, struct wl_resource *output_v1);
static void request_set_presentation_mode(struct wl_client *client, struct wl_resource *output_v1, uint32_t mode);

static const struct river_output_v1_interface output_impl = {
	.destroy = request_destroy,
	.set_presentation_mode = request_set_presentation_mode,
};

static void request_destroy_inert(struct wl_client *client, struct wl_resource *output_v1) {
	wl_resource_destroy(output_v1);
}

static void request_set_presentation_mode_inert(struct wl_client *client, struct wl_resource *output_v1,
		uint32_t mode) {
}

// handleRequestInert
static const struct river_output_v1_interface output_inert_impl = {
	.destroy = request_destroy_inert,
	.set_presentation_mode = request_set_presentation_mode_inert,
};

// Zig handleObjectDestroy(_: *river.OutputV1, output: *Output), also called directly by makeInert().
static void handle_object_destroy_data(struct rv_output *output) {
	output->object = NULL;
	output->sent_wl_output = false;
}

// The resource destructor installed with output_impl.
static void handle_object_destroy(struct wl_resource *output_v1) {
	struct rv_output *output = wl_resource_get_user_data(output_v1);
	handle_object_destroy_data(output);
}

void rv_output_manage_start(struct rv_output *output) {
	switch (output->scheduled.state) {
	case RV_OUTPUT_STATE_STATE_ENABLED:
	case RV_OUTPUT_STATE_STATE_DISABLED_SOFT: {
		// We cannot send 0 width/height to the window manager client.
		assert(output->scheduled.mode.tag != RV_OUTPUT_STATE_MODE_NONE);

		struct wlr_output *wlr_output = output->wlr_output;
		assert(wlr_output != NULL);

		rv_layer_shell_output_manage_start(&output->layer_shell);

		struct wl_resource *wm_v1 = server.wm.object;
		if (wm_v1 != NULL) {
			const bool new = output->object == NULL;
			struct wl_resource *output_v1 = output->object;
			if (output_v1 == NULL) {
				output_v1 = wl_resource_create(wl_resource_get_client(wm_v1), &river_output_v1_interface,
					wl_resource_get_version(wm_v1), 0);
				if (output_v1 == NULL) {
					rv_err(RV_SCOPE_OUTPUT, "out of memory");
					return; // try again next update
				}
				output->object = output_v1;

				wl_resource_set_implementation(output_v1, &output_impl, output, handle_object_destroy);
				river_window_manager_v1_send_output(wm_v1, output_v1);
			}

			if (!output->sent_wl_output) {
				// wl_output globals are created/destroyed by the wlroots output layout.
				if (wlr_output->global != NULL) {
					river_output_v1_send_wl_output(output_v1,
						wl_global_get_name(wlr_output->global, wl_resource_get_client(output_v1)));
					output->sent_wl_output = true;
				}
			}

			const struct rv_output_state *scheduled = &output->scheduled;
			const struct rv_output_state *sent = &output->sent;

			int32_t scheduled_width, scheduled_height;
			rv_output_state_dimensions(scheduled, &scheduled_width, &scheduled_height);
			int32_t sent_width, sent_height;
			rv_output_state_dimensions(sent, &sent_width, &sent_height);

			if (new || scheduled_width != sent_width || scheduled_height != sent_height) {
				river_output_v1_send_dimensions(output_v1, scheduled_width, scheduled_height);
			}
			if (new || scheduled->x != sent->x || scheduled->y != sent->y) {
				river_output_v1_send_position(output_v1, scheduled->x, scheduled->y);
			}
		}

		output->sent = output->scheduled;

		wl_list_remove(&output->link_sent);
		wl_list_insert(server.wm.sent.outputs.prev, &output->link_sent);
		break;
	}
	case RV_OUTPUT_STATE_STATE_DISABLED_HARD:
	case RV_OUTPUT_STATE_STATE_DESTROYING:
		rv_output_make_inert(output);

		output->sent = output->scheduled;

		if (output->scheduled.state == RV_OUTPUT_STATE_STATE_DESTROYING) {
			assert(output->wlr_output == NULL);
			{
				struct rv_slotmap_iterator it = rv_slotmap_iterator(&server.wm.windows);
				struct rv_window *window;
				while ((window = rv_slotmap_iterator_next(&it)) != NULL) {
					switch (window->wm_scheduled.fullscreen_requested.tag) {
					case RV_WINDOW_FULLSCREEN_REQUEST_FULLSCREEN:
						if (window->wm_scheduled.fullscreen_requested.fullscreen == output) {
							window->wm_scheduled.fullscreen_requested =
								(struct rv_window_fullscreen_request){
									.tag = RV_WINDOW_FULLSCREEN_REQUEST_FULLSCREEN,
									.fullscreen = NULL,
								};
						}
						break;
					case RV_WINDOW_FULLSCREEN_REQUEST_NO_REQUEST:
					case RV_WINDOW_FULLSCREEN_REQUEST_EXIT:
						break;
					}
					if (window->wm_requested.fullscreen == output) {
						window->wm_requested.fullscreen = NULL;
					}
				}
			}
			wl_list_remove(&output->link);
			wl_list_remove(&output->link_sent);

			free(output);
		}
		break;
	}
}

void rv_output_make_inert(struct rv_output *output) {
	struct wl_resource *output_v1 = output->object;
	if (output_v1 != NULL) {
		river_output_v1_send_removed(output_v1);
		wl_resource_set_implementation(output_v1, &output_inert_impl, NULL, NULL);
		rv_layer_shell_output_make_inert(&output->layer_shell);
		handle_object_destroy_data(output);
	}
}

static void request_destroy(struct wl_client *client, struct wl_resource *output_v1) {
	struct rv_output *output = wl_resource_get_user_data(output_v1);
	assert(output->object == output_v1);
	wl_resource_destroy(output_v1);
}

static void request_set_presentation_mode(struct wl_client *client, struct wl_resource *output_v1, uint32_t mode) {
	struct rv_output *output = wl_resource_get_user_data(output_v1);
	assert(output->object == output_v1);
	if (!rv_window_manager_ensure_rendering(&server.wm)) {
		return;
	}
	switch (mode) {
	case RIVER_OUTPUT_V1_PRESENTATION_MODE_VSYNC:
		output->rendering_requested.tearing = false;
		break;
	case RIVER_OUTPUT_V1_PRESENTATION_MODE_ASYNC:
		output->rendering_requested.tearing = true;
		break;
	default:
		wl_resource_post_error(output_v1, RIVER_OUTPUT_V1_ERROR_INVALID_PRESENTATION_MODE, "%s",
			"invalid presentation mode enum value");
		return;
	}
}

static void handle_request_state(struct wl_listener *listener, void *data) {
	struct rv_output *output = wl_container_of(listener, output, request_state);
	struct wlr_output_event_request_state *event = data;

	// The only state currently requested by a wlroots backend is a
	// custom mode as the Wayland/X11 backend window is resized.
	const uint32_t committed = event->state->committed;
	const uint32_t supported = WLR_OUTPUT_STATE_MODE;

	if (committed != supported) {
		rv_err(RV_SCOPE_OUTPUT, "backend requested unsupported state %u", committed);
		return;
	}

	rv_debug(RV_SCOPE_OUTPUT, "backend requested new mode");

	if (event->state->mode != NULL) {
		output->scheduled.mode.tag = RV_OUTPUT_STATE_MODE_STANDARD;
		output->scheduled.mode.standard = event->state->mode;
	} else {
		output->scheduled.mode.tag = RV_OUTPUT_STATE_MODE_CUSTOM;
		output->scheduled.mode.custom = (struct rv_output_state_mode_custom){
			.width = event->state->custom_mode.width,
			.height = event->state->custom_mode.height,
			.refresh = event->state->custom_mode.refresh,
		};
	}

	rv_window_manager_dirty_windowing(&server.wm);
}

// Zig fn renderAndCommit(output: *Output) !void: the only error is
// error.CommitFailed, returned here as false.
static bool render_and_commit(struct rv_output *output) {
	assert(output->scene_output != NULL);
	if (!wlr_scene_output_needs_frame(output->scene_output)) {
		return true;
	}

	struct wlr_output *wlr_output = output->wlr_output;
	assert(wlr_output != NULL);

	struct wlr_output_state state;
	wlr_output_state_init(&state);

	rv_output_state_apply_no_modeset(&output->current, &state);

	if (!wlr_scene_output_build_state(output->scene_output, &state, NULL)) {
		wlr_output_state_finish(&state);
		return false;
	}

	if (output->rendering_current.tearing) {
		state.tearing_page_flip = true;
		// TODO don't try this every frame if it consistently fails. Stop trying if it fails
		// for 10 frames in a row or something.
		if (!wlr_output_test_state(wlr_output, &state)) {
			rv_info(RV_SCOPE_OUTPUT, "tearing page flip test failed for %s, retrying without tearing",
				wlr_output->name);
			state.tearing_page_flip = false;
		}
	}

	if (!wlr_output_commit_state(wlr_output, &state)) {
		wlr_output_state_finish(&state);
		return false;
	}

	switch (server.lock_manager.state) {
	case RV_LOCK_MANAGER_STATE_UNLOCKED:
		if (output->lock_render_state != RV_OUTPUT_LOCK_RENDER_STATE_UNLOCKED) {
			output->lock_render_state = RV_OUTPUT_LOCK_RENDER_STATE_PENDING_UNLOCK;
		}
		break;
	case RV_LOCK_MANAGER_STATE_LOCKED:
		assert(!server.scene.normal_tree->node.enabled);
		switch (output->lock_render_state) {
		case RV_OUTPUT_LOCK_RENDER_STATE_PENDING_UNLOCK:
		case RV_OUTPUT_LOCK_RENDER_STATE_UNLOCKED:
		case RV_OUTPUT_LOCK_RENDER_STATE_PENDING_BLANK:
		case RV_OUTPUT_LOCK_RENDER_STATE_PENDING_LOCK_SURFACE:
			abort(); // unreachable
		case RV_OUTPUT_LOCK_RENDER_STATE_BLANKED:
		case RV_OUTPUT_LOCK_RENDER_STATE_LOCK_SURFACE:
			break;
		}
		break;
	case RV_LOCK_MANAGER_STATE_WAITING_FOR_BLANK:
		assert(!server.scene.normal_tree->node.enabled);
		if (output->lock_render_state != RV_OUTPUT_LOCK_RENDER_STATE_BLANKED) {
			output->lock_render_state = RV_OUTPUT_LOCK_RENDER_STATE_PENDING_BLANK;
		}
		break;
	case RV_LOCK_MANAGER_STATE_WAITING_FOR_LOCK_SURFACES: {
		bool lock_surface_mapped;
		struct rv_lock_surface *lock_surface =
			rv_lock_manager_lock_surface_from_output(&server.lock_manager, output);
		if (lock_surface != NULL) {
			lock_surface_mapped = lock_surface->wlr_lock_surface->surface->mapped;
		} else {
			lock_surface_mapped = false;
		}
		if (lock_surface_mapped) {
			if (output->lock_render_state != RV_OUTPUT_LOCK_RENDER_STATE_LOCK_SURFACE) {
				output->lock_render_state = RV_OUTPUT_LOCK_RENDER_STATE_PENDING_LOCK_SURFACE;
			}
		} else {
			if (output->lock_render_state != RV_OUTPUT_LOCK_RENDER_STATE_UNLOCKED) {
				output->lock_render_state = RV_OUTPUT_LOCK_RENDER_STATE_PENDING_UNLOCK;
			}
		}
		break;
	}
	}

	wlr_output_state_finish(&state);
	return true;
}

static void handle_frame(struct wl_listener *listener, void *data) {
	struct rv_output *output = wl_container_of(listener, output, frame);
	struct wlr_output *wlr_output = data;

	// TODO this should probably be retried on failure
	if (!render_and_commit(output)) {
		rv_err(RV_SCOPE_OUTPUT, "output commit failed for %s", wlr_output->name);
	}

	struct timespec now = rv_util_timestamp();
	assert(output->scene_output != NULL);
	wlr_scene_output_send_frame_done(output->scene_output, &now);
}

static void handle_present(struct wl_listener *listener, void *data) {
	struct rv_output *output = wl_container_of(listener, output, present);
	struct wlr_output_event_present *event = data;
	if (!event->presented) {
		return;
	}
	switch (output->lock_render_state) {
	case RV_OUTPUT_LOCK_RENDER_STATE_PENDING_UNLOCK:
		assert(server.lock_manager.state != RV_LOCK_MANAGER_STATE_LOCKED);
		output->lock_render_state = RV_OUTPUT_LOCK_RENDER_STATE_UNLOCKED;
		break;
	case RV_OUTPUT_LOCK_RENDER_STATE_UNLOCKED:
		assert(server.lock_manager.state != RV_LOCK_MANAGER_STATE_LOCKED);
		break;
	case RV_OUTPUT_LOCK_RENDER_STATE_PENDING_BLANK:
		output->lock_render_state = RV_OUTPUT_LOCK_RENDER_STATE_BLANKED;
		if (server.lock_manager.state != RV_LOCK_MANAGER_STATE_LOCKED) {
			rv_lock_manager_maybe_lock(&server.lock_manager);
		}
		break;
	case RV_OUTPUT_LOCK_RENDER_STATE_PENDING_LOCK_SURFACE:
		output->lock_render_state = RV_OUTPUT_LOCK_RENDER_STATE_LOCK_SURFACE;
		if (server.lock_manager.state != RV_LOCK_MANAGER_STATE_LOCKED) {
			rv_lock_manager_maybe_lock(&server.lock_manager);
		}
		break;
	case RV_OUTPUT_LOCK_RENDER_STATE_BLANKED:
	case RV_OUTPUT_LOCK_RENDER_STATE_LOCK_SURFACE:
		break;
	}
}
