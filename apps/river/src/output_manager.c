// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/OutputManager.zig for Oath, 2026-10-18.

#include "river.h"

static void handle_new_output(struct wl_listener *listener, void *data);
static void handle_manager_apply(struct wl_listener *listener, void *data);
static void handle_manager_test(struct wl_listener *listener, void *data);
static void handle_power_manager_set_mode(struct wl_listener *listener, void *data);
static void modeset_failed(struct rv_output_manager *om);
static bool send_config(struct rv_output_manager *om);

bool rv_output_manager_init(struct rv_output_manager *om) {
	struct wlr_output_layout *output_layout = wlr_output_layout_create(server.wl_server);
	if (output_layout == NULL) {
		return false;
	}

	struct wlr_gamma_control_manager_v1 *gamma_control_manager =
		wlr_gamma_control_manager_v1_create(server.wl_server);
	if (gamma_control_manager == NULL) {
		goto error;
	}
	wlr_scene_set_gamma_control_manager_v1(server.scene.wlr_scene, gamma_control_manager);

	struct wlr_presentation *presentation = wlr_presentation_create(server.wl_server, server.backend, 2);
	if (presentation == NULL) {
		goto error;
	}
	struct wlr_xdg_output_manager_v1 *xdg_output_manager =
		wlr_xdg_output_manager_v1_create(server.wl_server, output_layout);
	if (xdg_output_manager == NULL) {
		goto error;
	}
	struct wlr_output_manager_v1 *wlr_output_manager = wlr_output_manager_v1_create(server.wl_server);
	if (wlr_output_manager == NULL) {
		goto error;
	}
	struct wlr_output_power_manager_v1 *power_manager = wlr_output_power_manager_v1_create(server.wl_server);
	if (power_manager == NULL) {
		goto error;
	}

	*om = (struct rv_output_manager){
		.first_modeset = true,
		.new_output = { .notify = handle_new_output },
		.output_layout = output_layout,
		.presentation = presentation,
		.xdg_output_manager = xdg_output_manager,
		.wlr_output_manager = wlr_output_manager,
		.manager_apply = { .notify = handle_manager_apply },
		.manager_test = { .notify = handle_manager_test },
		.power_manager = power_manager,
		.power_manager_set_mode = { .notify = handle_power_manager_set_mode },
		.gamma_control_manager = gamma_control_manager,
	};

	wl_list_init(&om->outputs);

	wl_signal_add(&server.backend->events.new_output, &om->new_output);
	wl_signal_add(&om->wlr_output_manager->events.apply, &om->manager_apply);
	wl_signal_add(&om->wlr_output_manager->events.test, &om->manager_test);
	wl_signal_add(&om->power_manager->events.set_mode, &om->power_manager_set_mode);

	return true;

error:
	wlr_output_layout_destroy(output_layout);
	return false;
}

void rv_output_manager_deinit(struct rv_output_manager *om) {
	wl_list_remove(&om->manager_apply.link);
	wl_list_remove(&om->manager_test.link);
	wl_list_remove(&om->power_manager_set_mode.link);

	wlr_output_layout_destroy(om->output_layout);
}

static void handle_new_output(struct wl_listener *listener, void *data) {
	struct wlr_output *wlr_output = data;

	rv_debug(RV_SCOPE_OUTPUT, "new output %s", wlr_output->name);

	bool init_render_failed;
	if (!rv_output_create(wlr_output, &init_render_failed)) {
		if (init_render_failed) {
			rv_err(RV_SCOPE_OUTPUT, "failed to initialize renderer for output %s", wlr_output->name);
		} else {
			rv_err(RV_SCOPE_OUTPUT, "out of memory");
		}
		wlr_output_destroy(wlr_output);
		return;
	}
}

// Returns null if there are no outputs in the output layout
struct wlr_output *rv_output_manager_output_at(struct rv_output_manager *om, double lx, double ly) {
	double output_lx;
	double output_ly;
	wlr_output_layout_closest_point(om->output_layout, NULL, lx, ly, &output_lx, &output_ly);
	return wlr_output_layout_output_at(om->output_layout, output_lx, output_ly);
}

static bool validate_config_coordinates(struct wlr_output_configuration_v1 *config) {
	struct wlr_output_configuration_head_v1 *head, *tmp;
	wl_list_for_each_safe(head, tmp, &config->heads, link) {
		if (!head->state.enabled) {
			continue;
		}

		const struct rv_output_state proposed = rv_output_state_from_head_state(&head->state);
#if WLR_HAS_XWAYLAND
		if (server.xwayland != NULL) {
			// Negative output coordinates currently cause Xwayland clients to not receive click events.
			// See: https://gitlab.freedesktop.org/xorg/xserver/-/issues/899
			if (proposed.x < 0 || proposed.y < 0) {
				rv_err(RV_SCOPE_OUTPUT,
					"Attempted to set negative coordinates for output %s.\n"
					"Negative output coordinates are disallowed if Xwayland is enabled due to a limitation of Xwayland.",
					head->state.output->name);
				return false;
			}
			int32_t width, height;
			rv_output_state_dimensions(&proposed, &width, &height);
			if (proposed.x + width > INT16_MAX || proposed.y + height > INT16_MAX) {
				rv_err(RV_SCOPE_OUTPUT,
					"Attempted to set too-large coordinates for output %s.\n"
					"Coordinates greater than %d are disallowed if Xwayland is enabled due to a limitation of X11.",
					head->state.output->name, INT16_MAX);
				return false;
			}
		}
#else
		(void)proposed;
#endif
	}
	return true;
}

static void handle_manager_test(struct wl_listener *listener, void *data) {
	struct wlr_output_configuration_v1 *config = data;

	if (!validate_config_coordinates(config)) {
		wlr_output_configuration_v1_send_failed(config);
		wlr_output_configuration_v1_destroy(config);
		return;
	}

	size_t states_len;
	struct wlr_backend_output_state *states = wlr_output_configuration_v1_build_state(config, &states_len);
	if (states == NULL) {
		rv_err(RV_SCOPE_OUTPUT, "out of memory");
		wlr_output_configuration_v1_send_failed(config);
		wlr_output_configuration_v1_destroy(config);
		return;
	}

	struct wlr_output_swapchain_manager swapchain_manager;
	wlr_output_swapchain_manager_init(&swapchain_manager, server.backend);

	if (wlr_output_swapchain_manager_prepare(&swapchain_manager, states, states_len)) {
		wlr_output_configuration_v1_send_succeeded(config);
	} else {
		wlr_output_configuration_v1_send_failed(config);
	}

	wlr_output_swapchain_manager_finish(&swapchain_manager);
	free(states);
	wlr_output_configuration_v1_destroy(config);
}

static void handle_manager_apply(struct wl_listener *listener, void *data) {
	struct wlr_output_configuration_v1 *config = data;

	rv_info(RV_SCOPE_OUTPUT, "applying output configuration");

	if (!validate_config_coordinates(config)) {
		wlr_output_configuration_v1_send_failed(config);
		return;
	}

	struct wlr_output_configuration_head_v1 *head, *tmp;
	wl_list_for_each_safe(head, tmp, &config->heads, link) {
		struct rv_output *output = head->state.output->data;
		if (head->state.enabled) {
			const enum rv_output_state_state previous = output->scheduled.state;
			output->scheduled = rv_output_state_from_head_state(&head->state);
			// Maintain power management state set with wlr-output-power-management-v1
			if (previous == RV_OUTPUT_STATE_STATE_DISABLED_SOFT) {
				output->scheduled.state = RV_OUTPUT_STATE_STATE_DISABLED_SOFT;
			} else {
				assert(output->scheduled.state == RV_OUTPUT_STATE_STATE_ENABLED);
			}
		} else {
			// Avoid overwriting and losing all other output state on disable.
			output->scheduled.state = RV_OUTPUT_STATE_STATE_DISABLED_HARD;
		}
	}

	struct wlr_output_configuration_v1 *old = server.wm.scheduled.output_config;
	if (old != NULL) {
		wlr_output_configuration_v1_send_failed(old);
		wlr_output_configuration_v1_destroy(old);
	}
	server.wm.scheduled.output_config = config;

	rv_window_manager_dirty_windowing(&server.wm);
}

static const char *power_mode_name(enum zwlr_output_power_v1_mode mode) {
	switch (mode) {
	case ZWLR_OUTPUT_POWER_V1_MODE_OFF:
		return "off";
	case ZWLR_OUTPUT_POWER_V1_MODE_ON:
		return "on";
	}
	abort(); // @tagName of an unnamed enum value
}

static void handle_power_manager_set_mode(struct wl_listener *listener, void *data) {
	struct wlr_output_power_v1_set_mode_event *event = data;

	// The output may have been destroyed, in which case there is nothing to do
	struct rv_output *output = event->output->data;
	if (output == NULL) {
		return;
	}

	rv_debug(RV_SCOPE_OUTPUT, "client requested dpms %s for output %s", power_mode_name(event->mode),
		event->output->name);

	switch (output->scheduled.state) {
	case RV_OUTPUT_STATE_STATE_ENABLED:
		if (event->mode == ZWLR_OUTPUT_POWER_V1_MODE_OFF) {
			output->scheduled.state = RV_OUTPUT_STATE_STATE_DISABLED_SOFT;
		} else {
			return;
		}
		break;
	case RV_OUTPUT_STATE_STATE_DISABLED_SOFT:
		if (event->mode == ZWLR_OUTPUT_POWER_V1_MODE_ON) {
			output->scheduled.state = RV_OUTPUT_STATE_STATE_ENABLED;
		} else {
			return;
		}
		break;
	case RV_OUTPUT_STATE_STATE_DISABLED_HARD:
	case RV_OUTPUT_STATE_STATE_DESTROYING:
		abort(); // unreachable
	}

	rv_window_manager_dirty_windowing(&server.wm);
}

void rv_output_manager_auto_layout(struct rv_output_manager *om) {
	// Find the right most edge of any non-autolayout output.
	int32_t rightmost_edge = 0;
	int32_t row_y = 0;
	{
		struct rv_output *output, *tmp;
		wl_list_for_each_safe(output, tmp, &om->outputs, link) {
			if (output->scheduled.auto_layout) {
				continue;
			}

			int32_t width, height;
			rv_output_state_dimensions(&output->scheduled, &width, &height);
			const int32_t x = output->scheduled.x + width;
			if (x > rightmost_edge) {
				rightmost_edge = x;
				row_y = output->scheduled.y;
			}
		}
	}
	// Place autolayout outputs in a row starting at the rightmost edge.
	{
		struct rv_output *output, *tmp;
		wl_list_for_each_safe(output, tmp, &om->outputs, link) {
			if (!output->scheduled.auto_layout) {
				continue;
			}

			output->scheduled.x = rightmost_edge;
			output->scheduled.y = row_y;
			int32_t width, height;
			rv_output_state_dimensions(&output->scheduled, &width, &height);
			rightmost_edge += width;
		}
	}
}

// The body of the Zig need_modeset block.
static bool need_modeset(struct rv_window_manager *wm) {
	struct rv_output *output, *tmp;
	wl_list_for_each_safe(output, tmp, &wm->sent.outputs, link_sent) {
		struct wlr_output *wlr_output = output->wlr_output;
		if (wlr_output == NULL) {
			continue;
		}
		switch (output->sent.state) {
		case RV_OUTPUT_STATE_STATE_ENABLED:
			if (!wlr_output->enabled) {
				return true;
			}
			break;
		case RV_OUTPUT_STATE_STATE_DISABLED_SOFT:
		case RV_OUTPUT_STATE_STATE_DISABLED_HARD:
			if (wlr_output->enabled) {
				return true;
			}
			break;
		case RV_OUTPUT_STATE_STATE_DESTROYING:
			abort(); // unreachable
		}
		switch (output->sent.mode.tag) {
		case RV_OUTPUT_STATE_MODE_STANDARD:
			if (output->sent.mode.standard != wlr_output->current_mode) {
				return true;
			}
			break;
		case RV_OUTPUT_STATE_MODE_CUSTOM:
			if (output->sent.mode.custom.width != wlr_output->width) {
				return true;
			}
			if (output->sent.mode.custom.height != wlr_output->height) {
				return true;
			}
			if (output->sent.mode.custom.refresh != wlr_output->refresh) {
				return true;
			}
			break;
		// This branch is reachable if we fail to enable an output.
		case RV_OUTPUT_STATE_MODE_NONE:
			assert(output->sent.state == RV_OUTPUT_STATE_STATE_DISABLED_HARD);
			break;
		}
		// If an output newly exposed to river is already enabled, we
		// must modeset since the mode is otherwise undefined.
		if (output->current.mode.tag == RV_OUTPUT_STATE_MODE_NONE &&
				output->sent.state == RV_OUTPUT_STATE_STATE_ENABLED) {
			return true;
		}
		if (output->sent.adaptive_sync !=
				(wlr_output->adaptive_sync_status == WLR_OUTPUT_ADAPTIVE_SYNC_ENABLED)) {
			return true;
		}
	}
	return false;
}

// The Zig `if (need_modeset) { ... }` block. Returns false where the Zig
// returns from commitOutputState() inside that block.
static bool commit_modeset(struct rv_output_manager *om, struct rv_window_manager *wm) {
	rv_debug(RV_SCOPE_OUTPUT, "committing output state requires modeset");

	RV_ARRAY(struct wlr_backend_output_state) states = { 0 };
	bool ok = false;

	{
		struct rv_output *output, *tmp;
		wl_list_for_each_safe(output, tmp, &wm->sent.outputs, link_sent) {
			struct wlr_output *wlr_output = output->wlr_output;
			if (wlr_output == NULL) {
				continue;
			}
			if (!rv_array_reserve(&states, states.len + 1)) {
				rv_err(RV_SCOPE_OUTPUT, "out of memory");
				goto out_states;
			}
			struct wlr_backend_output_state *state = &states.items[states.len++];

			state->output = wlr_output;
			wlr_output_state_init(&state->base);

			rv_output_state_apply_modeset(&output->sent, &state->base);
		}
	}

	struct wlr_output_swapchain_manager swapchain_manager;
	wlr_output_swapchain_manager_init(&swapchain_manager, server.backend);

	if (!wlr_output_swapchain_manager_prepare(&swapchain_manager, states.items, states.len)) {
		rv_err(RV_SCOPE_OUTPUT, "failed to prepare new output configuration");
		modeset_failed(om);
		goto out_swapchain;
	}

	for (size_t i = 0; i < states.len; i++) {
		struct wlr_backend_output_state *state = &states.items[i];
		struct rv_output *output = state->output->data;
		const struct wlr_scene_output_state_options options = {
			.swapchain = wlr_output_swapchain_manager_get_swapchain(&swapchain_manager, state->output),
		};
		assert(output->scene_output != NULL);
		if (!wlr_scene_output_build_state(output->scene_output, &state->base, &options)) {
			rv_err(RV_SCOPE_OUTPUT, "failed to render scene for %s", state->output->name);
		}
	}

	if (!wlr_backend_commit(server.backend, states.items, states.len)) {
		rv_err(RV_SCOPE_OUTPUT, "failed to commit new output configuration");
		modeset_failed(om);
		goto out_swapchain;
	}
	om->first_modeset = false;

	wlr_output_swapchain_manager_apply(&swapchain_manager);

	ok = true;

out_swapchain:
	wlr_output_swapchain_manager_finish(&swapchain_manager);
out_states:
	for (size_t i = 0; i < states.len; i++) {
		wlr_output_state_finish(&states.items[i].base);
	}
	rv_array_finish(&states);
	return ok;
}

void rv_output_manager_commit_output_state(struct rv_output_manager *om) {
	struct rv_window_manager *wm = &server.wm;
	{
		struct rv_output *output, *tmp;
		wl_list_for_each_safe(output, tmp, &wm->sent.outputs, link_sent) {
			assert(output->sent.state != RV_OUTPUT_STATE_STATE_DESTROYING);
			output->rendering_current = output->rendering_requested;
			// This may be null even when the state is not .destroying if the
			// output is destroyed between manage start and render finish.
			struct wlr_output *wlr_output = output->wlr_output;
			if (wlr_output == NULL) {
				continue;
			}
			switch (output->sent.state) {
			case RV_OUTPUT_STATE_STATE_ENABLED:
			case RV_OUTPUT_STATE_STATE_DISABLED_SOFT: {
				assert(output->scene_output != NULL);
				wlr_scene_output_set_position(output->scene_output, output->sent.x, output->sent.y);
				if (wlr_output_layout_add(om->output_layout, wlr_output, output->sent.x, output->sent.y) ==
						NULL) {
					rv_err(RV_SCOPE_OUTPUT, "out of memory");
					continue; // Try again next time
				}
				struct rv_lock_surface *lock_surface =
					rv_lock_manager_lock_surface_from_output(&server.lock_manager, output);
				if (lock_surface != NULL) {
					wlr_scene_node_set_position(&lock_surface->tree->node, output->sent.x, output->sent.y);
				}
				break;
			}
			case RV_OUTPUT_STATE_STATE_DISABLED_HARD:
				wlr_output_layout_remove(om->output_layout, wlr_output);
				break;
			case RV_OUTPUT_STATE_STATE_DESTROYING:
				abort(); // unreachable
			}
		}
	}

	if (need_modeset(wm)) {
		if (!commit_modeset(om, wm)) {
			return;
		}
	}

	if (wm->sent.output_config != NULL) {
		struct wlr_output_configuration_v1 *config = wm->sent.output_config;
		wlr_output_configuration_v1_send_succeeded(config);
		wlr_output_configuration_v1_destroy(config);
		wm->sent.output_config = NULL;
	}

	{
		struct rv_output *output, *tmp;
		wl_list_for_each_safe(output, tmp, &wm->sent.outputs, link_sent) {
			struct wlr_output *wlr_output = output->wlr_output;
			if (wlr_output == NULL) {
				continue;
			}

			// The wl_output global is created by wlroots when the output is
			// added to the wlr_output_layout and a mode is committed.
			// Wlroots does not directly notify us when the wl_output global is created.
			// However, we want send the river_output_v1.wl_output event as soon as
			// possible and therefore need to check after committing a mode.
			if (!output->sent_wl_output) {
				if (wlr_output->global != NULL) {
					struct wl_resource *output_v1 = output->object;
					if (output_v1 != NULL) {
						river_output_v1_send_wl_output(output_v1,
							wl_global_get_name(wlr_output->global, wl_resource_get_client(output_v1)));
						output->sent_wl_output = true;
					}
				}
			}
			switch (output->sent.state) {
			case RV_OUTPUT_STATE_STATE_ENABLED:
				assert(wlr_output->enabled);
				wlr_output_schedule_frame(wlr_output);
				break;
			case RV_OUTPUT_STATE_STATE_DISABLED_SOFT:
			case RV_OUTPUT_STATE_STATE_DISABLED_HARD:
				assert(!wlr_output->enabled);
				output->lock_render_state = RV_OUTPUT_LOCK_RENDER_STATE_BLANKED;
				if (output->sent.state == RV_OUTPUT_STATE_STATE_DISABLED_HARD) {
					wl_list_remove(&output->link_sent);
					wl_list_init(&output->link_sent);
				}
				break;
			case RV_OUTPUT_STATE_STATE_DESTROYING:
				abort(); // unreachable
			}
			output->current = output->sent;
		}
	}

	if (!send_config(om)) {
		rv_err(RV_SCOPE_OUTPUT, "out of memory");
	}
}

static void modeset_failed(struct rv_output_manager *om) {
	struct rv_window_manager *wm = &server.wm;

	// If the very first modeset fails, the user's hardware/drivers are
	// probably not compatible with river. In this case, exit rather
	// than running forever without rendering anything.
	if (om->first_modeset) {
		rv_err(RV_SCOPE_OUTPUT, "initial modeset failed, exiting river");
		wl_display_terminate(server.wl_server);
		return;
	}

	if (wm->sent.output_config != NULL) {
		struct wlr_output_configuration_v1 *config = wm->sent.output_config;
		wlr_output_configuration_v1_send_failed(config);
		wlr_output_configuration_v1_destroy(config);
		wm->sent.output_config = NULL;
	}

	{
		// Revert to last working state on failure
		struct rv_output *output, *tmp;
		wl_list_for_each_safe(output, tmp, &wm->sent.outputs, link_sent) {
			output->scheduled = output->current;
			output->sent = output->current;
		}
		rv_window_manager_dirty_windowing(wm);
	}
}

// Send the current output state to all wlr-output-manager clients.
// Zig !void (error.OutOfMemory): returns false on OOM.
static bool send_config(struct rv_output_manager *om) {
	struct wlr_output_configuration_v1 *config = wlr_output_configuration_v1_create();
	if (config == NULL) {
		return false;
	}

	struct rv_output *output, *tmp;
	wl_list_for_each_safe(output, tmp, &om->outputs, link) {
		struct wlr_output *wlr_output = output->wlr_output;
		if (wlr_output == NULL) {
			continue;
		}
		struct wlr_output_configuration_head_v1 *head = wlr_output_configuration_head_v1_create(config, wlr_output);
		if (head == NULL) {
			// this destroys all associated config heads as well
			wlr_output_configuration_v1_destroy(config);
			return false;
		}

		// It's only necessary to overwrite the state that does not require a modeset.
		// All state that requires a modeset will have already been committed to the wlr_output.
		switch (output->current.state) {
		case RV_OUTPUT_STATE_STATE_ENABLED:
		case RV_OUTPUT_STATE_STATE_DISABLED_SOFT:
			head->state.enabled = true;
			break;
		case RV_OUTPUT_STATE_STATE_DISABLED_HARD:
			head->state.enabled = false;
			break;
		case RV_OUTPUT_STATE_STATE_DESTROYING:
			abort(); // unreachable
		}
		head->state.scale = output->current.scale;
		head->state.transform = output->current.transform;
		head->state.x = output->current.x;
		head->state.y = output->current.y;
	}

	// wlroots won't send events to clients unless something has changed
	// compared to the last config set.
	wlr_output_manager_v1_set_configuration(om->wlr_output_manager, config);
	return true;
}
