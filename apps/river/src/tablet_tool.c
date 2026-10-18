// SPDX-FileCopyrightText: © 2024 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/TabletTool.zig for Oath, 2026-10-18.

#include <math.h>

#include "river.h"

static struct rv_tablet_tool *create(struct wlr_seat *wlr_seat, struct wlr_tablet_tool *wlr_tool);
static void handle_destroy(struct wl_listener *listener, void *data);
static void handle_set_cursor(struct wl_listener *listener, void *data);
static void maybe_exit_down(struct rv_tablet_tool *tool, struct rv_tablet *tablet);
static void passthrough(struct rv_tablet_tool *tool, struct rv_tablet *tablet);

struct rv_tablet_tool *rv_tablet_tool_get(struct wlr_seat *wlr_seat, struct wlr_tablet_tool *wlr_tool) {
	struct rv_tablet_tool *tool = wlr_tool->data;
	if (tool != NULL) {
		return tool;
	} else {
		return create(wlr_seat, wlr_tool);
	}
}

static struct rv_tablet_tool *create(struct wlr_seat *wlr_seat, struct wlr_tablet_tool *wlr_tool) {
	struct rv_tablet_tool *tool = calloc(1, sizeof(*tool));
	if (tool == NULL) {
		return NULL;
	}

	struct wlr_cursor *wlr_cursor = wlr_cursor_create();
	if (wlr_cursor == NULL) {
		free(tool);
		return NULL;
	}

	wlr_cursor_attach_output_layout(wlr_cursor, server.om.output_layout);

	struct wlr_tablet_manager_v2 *tablet_manager = server.input_manager.tablet_manager;
	struct wlr_tablet_v2_tablet_tool *wp_tool = wlr_tablet_tool_create(tablet_manager, wlr_seat, wlr_tool);
	if (wp_tool == NULL) {
		wlr_cursor_destroy(wlr_cursor);
		free(tool);
		return NULL;
	}
	*tool = (struct rv_tablet_tool){
		.wp_tool = wp_tool,
		.wlr_cursor = wlr_cursor,
		.mode = { .tag = RV_TABLET_TOOL_MODE_PASSTHROUGH },
		.tilt_x = 0,
		.tilt_y = 0,
	};
	tool->destroy.notify = handle_destroy;
	tool->set_cursor.notify = handle_set_cursor;

	wlr_tool->data = tool;

	wl_signal_add(&wlr_tool->events.destroy, &tool->destroy);
	wl_signal_add(&tool->wp_tool->events.set_cursor, &tool->set_cursor);

	return tool;
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_tablet_tool *tool = wl_container_of(listener, tool, destroy);

	wlr_cursor_destroy(tool->wlr_cursor);

	wl_list_remove(&tool->destroy.link);
	wl_list_remove(&tool->set_cursor.link);

	free(tool);
}

bool rv_tablet_tool_allow_set_cursor(struct rv_tablet_tool *tool, struct wlr_seat_client *seat_client,
		uint32_t serial) {
	if (tool->wp_tool->focused_surface == NULL ||
			wl_resource_get_client(tool->wp_tool->focused_surface->resource) != seat_client->client) {
		rv_debug(RV_SCOPE_INPUT, "client tried to set cursor without focus");
		return false;
	}
	if (serial != tool->wp_tool->proximity_serial) {
		rv_debug(RV_SCOPE_INPUT, "focused client tried to set cursor with incorrect serial");
		return false;
	}
	return true;
}

static void handle_set_cursor(struct wl_listener *listener, void *data) {
	struct rv_tablet_tool *tool = wl_container_of(listener, tool, set_cursor);
	struct wlr_tablet_v2_event_cursor *event = data;

	if (rv_tablet_tool_allow_set_cursor(tool, event->seat_client, event->serial)) {
		wlr_cursor_set_surface(tool->wlr_cursor, event->surface, event->hotspot_x, event->hotspot_y);
	}
}

// Must be called before moving/warping TabletTool.wlr_cursor
// Must call detach() after attach() is called before returning.
static void attach(struct rv_tablet_tool *tool, struct rv_tablet *tablet) {
	wlr_cursor_attach_input_device(tool->wlr_cursor, tablet->device.wlr_device);
	wlr_cursor_map_input_to_output(tool->wlr_cursor, tablet->device.wlr_device, tablet->device.config.map_to_output);
	wlr_cursor_map_input_to_region(tool->wlr_cursor, tablet->device.wlr_device,
		&tablet->device.config.map_to_rectangle);
}

static void detach(struct rv_tablet_tool *tool, struct rv_tablet *tablet) {
	wlr_cursor_detach_input_device(tool->wlr_cursor, tablet->device.wlr_device);
}

void rv_tablet_tool_axis(struct rv_tablet_tool *tool, struct rv_tablet *tablet,
		struct wlr_tablet_tool_axis_event *event) {
	attach(tool, tablet);

	const bool updated_x = (event->updated_axes & WLR_TABLET_TOOL_AXIS_X) != 0;
	const bool updated_y = (event->updated_axes & WLR_TABLET_TOOL_AXIS_Y) != 0;
	if (updated_x || updated_y) {
		// I don't own all these different types of tablet tools to test that this
		// is correct for each, this is best effort from reading code/docs.
		// The same goes for all the different axes events.
		switch (tool->wp_tool->wlr_tool->type) {
		case WLR_TABLET_TOOL_TYPE_PEN:
		case WLR_TABLET_TOOL_TYPE_ERASER:
		case WLR_TABLET_TOOL_TYPE_BRUSH:
		case WLR_TABLET_TOOL_TYPE_PENCIL:
		case WLR_TABLET_TOOL_TYPE_AIRBRUSH:
		case WLR_TABLET_TOOL_TYPE_TOTEM:
			wlr_cursor_warp_absolute(
				tool->wlr_cursor,
				tablet->device.wlr_device,
				updated_x ? event->x : NAN,
				updated_y ? event->y : NAN
			);
			break;
		case WLR_TABLET_TOOL_TYPE_LENS:
		case WLR_TABLET_TOOL_TYPE_MOUSE:
			wlr_cursor_move(tool->wlr_cursor, tablet->device.wlr_device, event->dx, event->dy);
			break;
		}

		switch (tool->mode.tag) {
		case RV_TABLET_TOOL_MODE_PASSTHROUGH:
			passthrough(tool, tablet);
			break;
		case RV_TABLET_TOOL_MODE_DOWN: {
			struct rv_tablet_tool_mode_down data = tool->mode.down;
			wlr_tablet_v2_tablet_tool_notify_motion(
				tool->wp_tool,
				data.sx + (tool->wlr_cursor->x - data.lx),
				data.sy + (tool->wlr_cursor->y - data.ly)
			);
			break;
		}
		}
	}
	if (event->updated_axes & WLR_TABLET_TOOL_AXIS_DISTANCE) {
		wlr_tablet_v2_tablet_tool_notify_distance(tool->wp_tool, event->distance);
	}
	if (event->updated_axes & WLR_TABLET_TOOL_AXIS_PRESSURE) {
		wlr_tablet_v2_tablet_tool_notify_pressure(tool->wp_tool, event->pressure);
	}
	if (event->updated_axes & (WLR_TABLET_TOOL_AXIS_TILT_X | WLR_TABLET_TOOL_AXIS_TILT_Y)) {
		if (event->updated_axes & WLR_TABLET_TOOL_AXIS_TILT_X) {
			tool->tilt_x = event->tilt_x;
		}
		if (event->updated_axes & WLR_TABLET_TOOL_AXIS_TILT_Y) {
			tool->tilt_y = event->tilt_y;
		}

		wlr_tablet_v2_tablet_tool_notify_tilt(tool->wp_tool, tool->tilt_x, tool->tilt_y);
	}
	if (event->updated_axes & WLR_TABLET_TOOL_AXIS_ROTATION) {
		wlr_tablet_v2_tablet_tool_notify_rotation(tool->wp_tool, event->rotation);
	}
	if (event->updated_axes & WLR_TABLET_TOOL_AXIS_SLIDER) {
		wlr_tablet_v2_tablet_tool_notify_slider(tool->wp_tool, event->slider);
	}
	if (event->updated_axes & WLR_TABLET_TOOL_AXIS_WHEEL) {
		wlr_tablet_v2_tablet_tool_notify_wheel(tool->wp_tool, event->wheel_delta, 0);
	}

	// defer tool.detach(tablet)
	detach(tool, tablet);
}

void rv_tablet_tool_proximity(struct rv_tablet_tool *tool, struct rv_tablet *tablet,
		struct wlr_tablet_tool_proximity_event *event) {
	switch (event->state) {
	case WLR_TABLET_TOOL_PROXIMITY_IN:
		attach(tool, tablet);

		wlr_cursor_warp_absolute(tool->wlr_cursor, tablet->device.wlr_device, event->x, event->y);

		wlr_cursor_set_xcursor(tool->wlr_cursor, tablet->device.seat->cursor.xcursor_manager, "pencil");

		passthrough(tool, tablet);

		// defer tool.detach(tablet)
		detach(tool, tablet);
		break;
	case WLR_TABLET_TOOL_PROXIMITY_OUT:
		wlr_tablet_v2_tablet_tool_notify_proximity_out(tool->wp_tool);
		wlr_cursor_unset_image(tool->wlr_cursor);
		break;
	}
}

void rv_tablet_tool_tip(struct rv_tablet_tool *tool, struct rv_tablet *tablet,
		struct wlr_tablet_tool_tip_event *event) {
	switch (event->state) {
	case WLR_TABLET_TOOL_TIP_DOWN: {
		// There have been reports of libinput emitting inconsistent down/up events.
		if (tool->wp_tool->is_down) {
			return;
		}

		wlr_tablet_v2_tablet_tool_notify_down(tool->wp_tool);

		struct rv_scene_at_result result;
		if (rv_scene_at(&server.scene, tool->wlr_cursor->x, tool->wlr_cursor->y, &result)) {
			if (result.surface != NULL) {
				tool->mode = (struct rv_tablet_tool_mode){
					.tag = RV_TABLET_TOOL_MODE_DOWN,
					.down = {
						.lx = tool->wlr_cursor->x,
						.ly = tool->wlr_cursor->y,
						.sx = result.sx,
						.sy = result.sy,
					},
				};
			}
		}
		break;
	}
	case WLR_TABLET_TOOL_TIP_UP:
		// There have been reports of libinput emitting inconsistent down/up events.
		if (!tool->wp_tool->is_down) {
			return;
		}

		wlr_tablet_v2_tablet_tool_notify_up(tool->wp_tool);
		maybe_exit_down(tool, tablet);
		break;
	}
}

void rv_tablet_tool_button(struct rv_tablet_tool *tool, struct rv_tablet *tablet,
		struct wlr_tablet_tool_button_event *event) {
	// zig-wlroots types this state as zwp.TabletToolV2.ButtonState; the wlroots
	// header declares the parameter as enum zwp_tablet_pad_v2_button_state.
	// All three enums use released = 0, pressed = 1.
	wlr_tablet_v2_tablet_tool_notify_button(tool->wp_tool, event->button,
		(enum zwp_tablet_pad_v2_button_state)event->state);

	maybe_exit_down(tool, tablet);
}

// Exit down mode if the tool is up and there are no buttons pressed.
static void maybe_exit_down(struct rv_tablet_tool *tool, struct rv_tablet *tablet) {
	if (tool->mode.tag != RV_TABLET_TOOL_MODE_DOWN || tool->wp_tool->is_down || tool->wp_tool->num_buttons > 0) {
		return;
	}

	tool->mode = (struct rv_tablet_tool_mode){ .tag = RV_TABLET_TOOL_MODE_PASSTHROUGH };
	passthrough(tool, tablet);
}

// Send a motion event for the surface under the tablet tool's cursor if any.
// Send a proximity_in event first if needed.
// If there is no surface under the cursor or the surface under the cursor
// does not support the tablet v2 protocol, send a proximity_out event.
static void passthrough(struct rv_tablet_tool *tool, struct rv_tablet *tablet) {
	struct rv_scene_at_result result;
	if (rv_scene_at(&server.scene, tool->wlr_cursor->x, tool->wlr_cursor->y, &result)) {
		if (result.data.tag == RV_SCENE_NODE_DATA_DATA_LOCK_SURFACE) {
			assert(server.lock_manager.state != RV_LOCK_MANAGER_STATE_UNLOCKED);
		} else {
			assert(server.lock_manager.state != RV_LOCK_MANAGER_STATE_LOCKED);
		}

		if (result.surface != NULL) {
			struct wlr_surface *surface = result.surface;
			wlr_tablet_v2_tablet_tool_notify_proximity_in(tool->wp_tool, tablet->wp_tablet, surface);
			wlr_tablet_v2_tablet_tool_notify_motion(tool->wp_tool, result.sx, result.sy);
			return;
		}
	} else {
		wlr_cursor_set_xcursor(tool->wlr_cursor, tablet->device.seat->cursor.xcursor_manager, "pencil");
	}

	wlr_tablet_v2_tablet_tool_notify_proximity_out(tool->wp_tool);
}
