// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Cursor.zig for Oath, 2026-10-18.

#include <math.h>

#include "river.h"

#define DEFAULT_SIZE 24

static void handle_image_surface_destroy(struct wl_listener *listener, void *data);
static void handle_wm_image_surface_destroy(struct wl_listener *listener, void *data);
static void handle_request_set_cursor(struct wl_listener *listener, void *data);

static void queue_motion_relative(struct wl_listener *listener, void *data);
static void queue_motion_absolute(struct wl_listener *listener, void *data);
static void queue_button(struct wl_listener *listener, void *data);
static void queue_axis(struct wl_listener *listener, void *data);
static void queue_frame(struct wl_listener *listener, void *data);

static void queue_swipe_begin(struct wl_listener *listener, void *data);
static void queue_swipe_update(struct wl_listener *listener, void *data);
static void queue_swipe_end(struct wl_listener *listener, void *data);

static void queue_pinch_begin(struct wl_listener *listener, void *data);
static void queue_pinch_update(struct wl_listener *listener, void *data);
static void queue_pinch_end(struct wl_listener *listener, void *data);

static void queue_hold_begin(struct wl_listener *listener, void *data);
static void queue_hold_end(struct wl_listener *listener, void *data);

static void handle_touch_down(struct wl_listener *listener, void *data);
static void handle_touch_motion(struct wl_listener *listener, void *data);
static void handle_touch_up(struct wl_listener *listener, void *data);
static void handle_touch_cancel(struct wl_listener *listener, void *data);
static void handle_touch_frame(struct wl_listener *listener, void *data);

static void handle_tablet_tool_axis(struct wl_listener *listener, void *data);
static void handle_tablet_tool_proximity(struct wl_listener *listener, void *data);
static void handle_tablet_tool_tip(struct wl_listener *listener, void *data);
static void handle_tablet_tool_button(struct wl_listener *listener, void *data);

static void clear_focus(struct rv_cursor *cursor);
static void move(const struct rv_cursor *cursor, const struct wlr_box *mapping, double dx, double dy);
static void update_hovered(struct rv_cursor *cursor);
static void interact(struct rv_cursor *cursor, const struct rv_scene_at_result *result);
static void passthrough(struct rv_cursor *cursor, uint32_t time);
static void update_drag_icons(struct rv_cursor *cursor);

// @tagName(cursor.mode)
static const char *mode_tag_name(enum rv_cursor_mode_tag tag) {
	switch (tag) {
	case RV_CURSOR_MODE_PASSTHROUGH:
		return "passthrough";
	case RV_CURSOR_MODE_IGNORE:
		return "ignore";
	case RV_CURSOR_MODE_DRAG:
		return "drag";
	case RV_CURSOR_MODE_DOWN:
		return "down";
	case RV_CURSOR_MODE_OP:
		return "op";
	}
	abort();
}

// std.math.lossyCast(i32, value) for a float value.
static int32_t lossy_cast_i32(double value) {
	if (isnan(value)) {
		return 0;
	} else if (value >= (double)INT32_MAX) {
		return INT32_MAX;
	} else if (value <= (double)INT32_MIN) {
		return INT32_MIN;
	} else {
		return (int32_t)value;
	}
}

// Cursor.pressed (std.AutoHashMapUnmanaged(u32, ?*PointerBinding)) lookup.
static bool pressed_find(const struct rv_cursor *cursor, uint32_t button, size_t *index) {
	for (size_t i = 0; i < cursor->pressed.len; i++) {
		if (cursor->pressed.items[i].button == button) {
			*index = i;
			return true;
		}
	}
	return false;
}

// Cursor.touch_points (std.AutoHashMapUnmanaged(i32, LayoutPoint)) lookup.
static struct rv_cursor_layout_point *touch_points_get_ptr(struct rv_cursor *cursor, int32_t touch_id) {
	for (size_t i = 0; i < cursor->touch_points.len; i++) {
		if (cursor->touch_points.items[i].touch_id == touch_id) {
			return &cursor->touch_points.items[i].point;
		}
	}
	return NULL;
}

// Cursor.touch_points.remove(): returns true if the key was present.
static bool touch_points_remove(struct rv_cursor *cursor, int32_t touch_id) {
	for (size_t i = 0; i < cursor->touch_points.len; i++) {
		if (cursor->touch_points.items[i].touch_id == touch_id) {
			rv_array_swap_remove(&cursor->touch_points, i);
			return true;
		}
	}
	return false;
}

bool rv_cursor_init(struct rv_cursor *cursor, struct rv_seat *seat) {
	struct wlr_cursor *wlr_cursor = wlr_cursor_create();
	if (wlr_cursor == NULL) {
		return false;
	}
	wlr_cursor_attach_output_layout(wlr_cursor, server.om.output_layout);

	// This is here so that cursor.xcursor_manager doesn't need to be an
	// optional pointer. This isn't optimal as it does a needless allocation,
	// but this is not a hot path.
	struct wlr_xcursor_manager *xcursor_manager = wlr_xcursor_manager_create(NULL, DEFAULT_SIZE);
	if (xcursor_manager == NULL) {
		wlr_cursor_destroy(wlr_cursor);
		return false;
	}

	*cursor = (struct rv_cursor){
		.mode = { .tag = RV_CURSOR_MODE_PASSTHROUGH },
		.seat = seat,
		.wlr_cursor = wlr_cursor,
		.xcursor_manager = xcursor_manager,
		.image = { .tag = RV_CURSOR_IMAGE_NONE },
		.hidden = false,
		.wm_image = { .tag = RV_CURSOR_IMAGE_XCURSOR, .xcursor = "default" },
		.constraint = NULL,
	};
	cursor->image_surface_destroy.notify = handle_image_surface_destroy;
	cursor->wm_image_surface_destroy.notify = handle_wm_image_surface_destroy;
	cursor->request_set_cursor.notify = handle_request_set_cursor;

	cursor->motion_relative.notify = queue_motion_relative;
	cursor->motion_absolute.notify = queue_motion_absolute;
	cursor->button.notify = queue_button;
	cursor->axis.notify = queue_axis;
	cursor->frame.notify = queue_frame;

	cursor->swipe_begin.notify = queue_swipe_begin;
	cursor->swipe_update.notify = queue_swipe_update;
	cursor->swipe_end.notify = queue_swipe_end;

	cursor->pinch_begin.notify = queue_pinch_begin;
	cursor->pinch_update.notify = queue_pinch_update;
	cursor->pinch_end.notify = queue_pinch_end;

	cursor->hold_begin.notify = queue_hold_begin;
	cursor->hold_end.notify = queue_hold_end;

	cursor->touch_down.notify = handle_touch_down;
	cursor->touch_motion.notify = handle_touch_motion;
	cursor->touch_up.notify = handle_touch_up;
	cursor->touch_cancel.notify = handle_touch_cancel;
	cursor->touch_frame.notify = handle_touch_frame;

	cursor->tablet_tool_axis.notify = handle_tablet_tool_axis;
	cursor->tablet_tool_proximity.notify = handle_tablet_tool_proximity;
	cursor->tablet_tool_tip.notify = handle_tablet_tool_tip;
	cursor->tablet_tool_button.notify = handle_tablet_tool_button;

	if (!rv_cursor_set_theme(cursor, NULL, NULL)) {
		wlr_xcursor_manager_destroy(xcursor_manager);
		wlr_cursor_destroy(wlr_cursor);
		return false;
	}

	wl_signal_add(&seat->wlr_seat->events.request_set_cursor, &cursor->request_set_cursor);

	wl_signal_add(&wlr_cursor->events.motion, &cursor->motion_relative);
	wl_signal_add(&wlr_cursor->events.motion_absolute, &cursor->motion_absolute);
	wl_signal_add(&wlr_cursor->events.button, &cursor->button);
	wl_signal_add(&wlr_cursor->events.axis, &cursor->axis);
	wl_signal_add(&wlr_cursor->events.frame, &cursor->frame);

	wl_signal_add(&wlr_cursor->events.swipe_begin, &cursor->swipe_begin);
	wl_signal_add(&wlr_cursor->events.swipe_update, &cursor->swipe_update);
	wl_signal_add(&wlr_cursor->events.swipe_end, &cursor->swipe_end);

	wl_signal_add(&wlr_cursor->events.pinch_begin, &cursor->pinch_begin);
	wl_signal_add(&wlr_cursor->events.pinch_update, &cursor->pinch_update);
	wl_signal_add(&wlr_cursor->events.pinch_end, &cursor->pinch_end);

	wl_signal_add(&wlr_cursor->events.hold_begin, &cursor->hold_begin);
	wl_signal_add(&wlr_cursor->events.hold_end, &cursor->hold_end);

	wl_signal_add(&wlr_cursor->events.touch_down, &cursor->touch_down);
	wl_signal_add(&wlr_cursor->events.touch_motion, &cursor->touch_motion);
	wl_signal_add(&wlr_cursor->events.touch_up, &cursor->touch_up);
	wl_signal_add(&wlr_cursor->events.touch_cancel, &cursor->touch_cancel);
	wl_signal_add(&wlr_cursor->events.touch_frame, &cursor->touch_frame);

	wl_signal_add(&wlr_cursor->events.tablet_tool_axis, &cursor->tablet_tool_axis);
	wl_signal_add(&wlr_cursor->events.tablet_tool_proximity, &cursor->tablet_tool_proximity);
	wl_signal_add(&wlr_cursor->events.tablet_tool_tip, &cursor->tablet_tool_tip);
	wl_signal_add(&wlr_cursor->events.tablet_tool_button, &cursor->tablet_tool_button);

	return true;
}

void rv_cursor_deinit(struct rv_cursor *cursor) {
	wl_list_remove(&cursor->axis.link);
	wl_list_remove(&cursor->button.link);
	wl_list_remove(&cursor->frame.link);
	wl_list_remove(&cursor->motion_absolute.link);
	wl_list_remove(&cursor->motion_relative.link);
	wl_list_remove(&cursor->swipe_begin.link);
	wl_list_remove(&cursor->swipe_update.link);
	wl_list_remove(&cursor->swipe_end.link);
	wl_list_remove(&cursor->pinch_begin.link);
	wl_list_remove(&cursor->pinch_update.link);
	wl_list_remove(&cursor->pinch_end.link);
	wl_list_remove(&cursor->hold_begin.link);
	wl_list_remove(&cursor->hold_end.link);
	wl_list_remove(&cursor->request_set_cursor.link);

	wl_list_remove(&cursor->touch_down.link);
	wl_list_remove(&cursor->touch_motion.link);
	wl_list_remove(&cursor->touch_up.link);
	wl_list_remove(&cursor->touch_cancel.link);
	wl_list_remove(&cursor->touch_frame.link);

	wl_list_remove(&cursor->tablet_tool_axis.link);
	wl_list_remove(&cursor->tablet_tool_proximity.link);
	wl_list_remove(&cursor->tablet_tool_tip.link);
	wl_list_remove(&cursor->tablet_tool_button.link);

	wlr_xcursor_manager_destroy(cursor->xcursor_manager);
	wlr_cursor_destroy(cursor->wlr_cursor);
	rv_array_finish(&cursor->pressed);
	rv_array_finish(&cursor->touch_points);
}

// Set the cursor theme for the given seat, as well as the xwayland theme if
// this is the default seat. Either argument may be null, in which case a
// default will be used.
bool rv_cursor_set_theme(struct rv_cursor *cursor, const char *theme, const uint32_t *_size) {
	const uint32_t size = _size != NULL ? *_size : DEFAULT_SIZE;

	struct wlr_xcursor_manager *xcursor_manager = wlr_xcursor_manager_create(theme, size);
	if (xcursor_manager == NULL) {
		return false;
	}

	// If this cursor belongs to the default seat, update the Xwayland cursor to match the theme.
	// This cursor is communicated to Xwayland clients through the XCB_CW_CURSOR window attribute.
	if (cursor->seat == rv_input_manager_default_seat(&server.input_manager)) {
#if WLR_HAS_XWAYLAND
		if (server.xwayland != NULL) {
			struct wlr_xwayland *xwayland = server.xwayland;
			if (!wlr_xcursor_manager_load(xcursor_manager, 1)) {
				wlr_xcursor_manager_destroy(xcursor_manager);
				return false;
			}
			struct wlr_xcursor *wlr_xcursor = wlr_xcursor_manager_get_xcursor(xcursor_manager, "default", 1);
			assert(wlr_xcursor != NULL);
			struct wlr_xcursor_image *image = wlr_xcursor->images[0];
			wlr_xwayland_set_cursor(
				xwayland,
				wlr_xcursor_image_get_buffer(image),
				(int32_t)image->hotspot_x,
				(int32_t)image->hotspot_y
			);
		}
#endif
	}

	// Everything fallible is now done so the the old xcursor_manager can be destroyed.
	wlr_xcursor_manager_destroy(cursor->xcursor_manager);
	cursor->xcursor_manager = xcursor_manager;

	switch (cursor->image.tag) {
	case RV_CURSOR_IMAGE_NONE:
	case RV_CURSOR_IMAGE_CLIENT:
		break;
	case RV_CURSOR_IMAGE_XCURSOR:
		wlr_cursor_set_xcursor(cursor->wlr_cursor, xcursor_manager, cursor->image.xcursor);
		break;
	}

	return true;
}

void rv_cursor_set_hidden(struct rv_cursor *cursor, bool hidden) {
	if (cursor->hidden == hidden) {
		return;
	}
	cursor->hidden = hidden;
	if (hidden) {
		rv_cursor_set_image(cursor, (struct rv_cursor_image){ .tag = RV_CURSOR_IMAGE_NONE });
	} else {
		// Force re-enter so the focused client sends set_cursor again.
		wlr_seat_pointer_notify_clear_focus(cursor->seat->wlr_seat);
		rv_cursor_update_state(cursor);
	}
}

void rv_cursor_set_image(struct rv_cursor *cursor, struct rv_cursor_image image) {
	if (cursor->hidden) {
		switch (image.tag) {
		case RV_CURSOR_IMAGE_NONE:
			break;
		default:
			return;
		}
	}
	if (cursor->image.tag == RV_CURSOR_IMAGE_CLIENT) {
		wl_list_remove(&cursor->image_surface_destroy.link);
	}
	cursor->image = image;
	switch (cursor->image.tag) {
	case RV_CURSOR_IMAGE_NONE:
		wlr_cursor_unset_image(cursor->wlr_cursor);
		break;
	case RV_CURSOR_IMAGE_XCURSOR:
		wlr_cursor_set_xcursor(cursor->wlr_cursor, cursor->xcursor_manager, cursor->image.xcursor);
		break;
	case RV_CURSOR_IMAGE_CLIENT: {
		struct rv_cursor_image_client client = cursor->image.client;
		wl_signal_add(&client.surface->events.destroy, &cursor->image_surface_destroy);
		wlr_cursor_set_surface(cursor->wlr_cursor, client.surface, client.hotspot_x, client.hotspot_y);
		break;
	}
	}
}

static void handle_image_surface_destroy(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, image_surface_destroy);
	// wlroots calls wlr_cursor_unset_image() automatically
	// when the cursor surface is destroyed.
	cursor->image = (struct rv_cursor_image){ .tag = RV_CURSOR_IMAGE_NONE };
	wl_list_remove(&cursor->image_surface_destroy.link);
}

void rv_cursor_set_wm_image(struct rv_cursor *cursor, struct rv_cursor_image wm_image) {
	if (cursor->wm_image.tag == RV_CURSOR_IMAGE_CLIENT) {
		wl_list_remove(&cursor->wm_image_surface_destroy.link);
	}
	cursor->wm_image = wm_image;
	if (cursor->wm_image.tag == RV_CURSOR_IMAGE_CLIENT) {
		wl_signal_add(&cursor->wm_image.client.surface->events.destroy, &cursor->wm_image_surface_destroy);
	}
	if (cursor->seat->wlr_seat->pointer_state.focused_client == NULL) {
		rv_cursor_set_image(cursor, wm_image);
	}
}

static void handle_wm_image_surface_destroy(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, wm_image_surface_destroy);
	cursor->wm_image = (struct rv_cursor_image){ .tag = RV_CURSOR_IMAGE_NONE };
	wl_list_remove(&cursor->wm_image_surface_destroy.link);
}

static void handle_request_set_cursor(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, request_set_cursor);
	struct wlr_seat_pointer_request_set_cursor_event *event = data;
	if (cursor->hidden) {
		return;
	}
	struct wlr_seat_client *focused_client = cursor->seat->wlr_seat->pointer_state.focused_client;

	// Only the client with pointer focus is allowed to set the cursor
	if (event->seat_client == focused_client) {
		rv_debug(RV_SCOPE_INPUT, "focused client set cursor");
		if (event->surface != NULL) {
			rv_cursor_set_image(cursor, (struct rv_cursor_image){
				.tag = RV_CURSOR_IMAGE_CLIENT,
				.client = {
					.surface = event->surface,
					.hotspot_x = event->hotspot_x,
					.hotspot_y = event->hotspot_y,
				},
			});
		} else {
			rv_cursor_set_image(cursor, (struct rv_cursor_image){ .tag = RV_CURSOR_IMAGE_NONE });
		}
	}
	// Except for the window manager client
	if (server.wm.object != NULL) {
		struct wl_resource *object = server.wm.object;
		if (event->seat_client->client == wl_resource_get_client(object) &&
				wl_resource_get_version(object) >= 4) {
			if (event->surface != NULL) {
				rv_cursor_set_wm_image(cursor, (struct rv_cursor_image){
					.tag = RV_CURSOR_IMAGE_CLIENT,
					.client = {
						.surface = event->surface,
						.hotspot_x = event->hotspot_x,
						.hotspot_y = event->hotspot_y,
					},
				});
			} else {
				rv_cursor_set_wm_image(cursor, (struct rv_cursor_image){ .tag = RV_CURSOR_IMAGE_NONE });
			}
		}
	}
}

static void clear_focus(struct rv_cursor *cursor) {
	rv_cursor_set_image(cursor, cursor->wm_image);
	wlr_seat_pointer_notify_clear_focus(cursor->seat->wlr_seat);
}

void rv_cursor_op_start_pointer(struct rv_cursor *cursor) {
	if (cursor->constraint != NULL) {
		struct rv_pointer_constraint *constraint = cursor->constraint;
		if (constraint->state.tag == RV_POINTER_CONSTRAINT_STATE_ACTIVE) {
			rv_pointer_constraint_deactivate(constraint);
		}
	}

	rv_debug(RV_SCOPE_INPUT, "entering cursor mode op");
	cursor->mode = (struct rv_cursor_mode){
		.tag = RV_CURSOR_MODE_OP,
		.op = { .delta_x = 0, .delta_y = 0 },
	};

	clear_focus(cursor);
}

void rv_cursor_op_end_pointer(struct rv_cursor *cursor) {
	if (cursor->pressed.len == 0) {
		rv_debug(RV_SCOPE_INPUT, "entering cursor mode passthrough");
		cursor->mode = (struct rv_cursor_mode){ .tag = RV_CURSOR_MODE_PASSTHROUGH };
		rv_cursor_update_state(cursor);
	} else {
		rv_debug(RV_SCOPE_INPUT, "entering cursor mode ignore");
		cursor->mode = (struct rv_cursor_mode){ .tag = RV_CURSOR_MODE_IGNORE };
	}
}

void rv_cursor_process_motion_relative(struct rv_cursor *cursor,
		const struct rv_seat_event_pointer_motion_relative *event) {
	wlr_relative_pointer_manager_v1_send_relative_motion(
		server.input_manager.relative_pointer_manager,
		cursor->seat->wlr_seat,
		(uint64_t)event->time_msec * 1000,
		event->delta_x,
		event->delta_y,
		event->unaccel_dx,
		event->unaccel_dy
	);

	double dx = event->delta_x;
	double dy = event->delta_y;

	if (cursor->constraint != NULL) {
		struct rv_pointer_constraint *constraint = cursor->constraint;
		if (constraint->state.tag == RV_POINTER_CONSTRAINT_STATE_ACTIVE) {
			switch (constraint->wlr_constraint->type) {
			case WLR_POINTER_CONSTRAINT_V1_LOCKED:
				return;
			case WLR_POINTER_CONSTRAINT_V1_CONFINED:
				rv_pointer_constraint_confine(constraint, &dx, &dy);
				break;
			}
		}
	}

	switch (cursor->mode.tag) {
	case RV_CURSOR_MODE_PASSTHROUGH:
	case RV_CURSOR_MODE_DRAG:
	case RV_CURSOR_MODE_IGNORE:
	case RV_CURSOR_MODE_DOWN:
		move(cursor, &event->mapping, dx, dy);
		rv_seat_send_live_pointer_position(cursor->seat);

		switch (cursor->mode.tag) {
		case RV_CURSOR_MODE_PASSTHROUGH:
		case RV_CURSOR_MODE_DRAG:
			update_hovered(cursor);
			passthrough(cursor, event->time_msec);
			break;
		case RV_CURSOR_MODE_IGNORE:
			break;
		case RV_CURSOR_MODE_DOWN: {
			struct rv_cursor_mode_down data = cursor->mode.down;
			wlr_seat_pointer_notify_motion(
				cursor->seat->wlr_seat,
				event->time_msec,
				data.sx + (cursor->wlr_cursor->x - data.lx),
				data.sy + (cursor->wlr_cursor->y - data.ly)
			);
			break;
		}
		default:
			abort(); // unreachable
		}

		update_drag_icons(cursor);

		if (cursor->constraint != NULL) {
			rv_pointer_constraint_maybe_activate(cursor->constraint);
		}
		break;
	case RV_CURSOR_MODE_OP: {
		struct rv_cursor_mode_op *data = &cursor->mode.op;
		dx += data->delta_x;
		dy += data->delta_y;
		data->delta_x = dx - trunc(dx);
		data->delta_y = dy - trunc(dy);

		move(cursor, &event->mapping, dx, dy);
		rv_seat_send_live_pointer_position(cursor->seat);
		rv_seat_op_update(cursor->seat, (int32_t)cursor->wlr_cursor->x, (int32_t)cursor->wlr_cursor->y);
		break;
	}
	}
}

static void move(const struct rv_cursor *cursor, const struct wlr_box *mapping, double dx, double dy) {
	double lx = cursor->wlr_cursor->x + dx;
	double ly = cursor->wlr_cursor->y + dy;
	if (!wlr_box_empty(mapping)) {
		wlr_box_closest_point(mapping, lx, ly, &lx, &ly);
	}
	wlr_cursor_warp_closest(cursor->wlr_cursor, NULL, lx, ly);
}

static void update_hovered(struct rv_cursor *cursor) {
	struct rv_seat *seat = cursor->seat;
	const bool old_has = seat->wm_scheduled.has_hovered;
	const struct rv_window_ref old = seat->wm_scheduled.hovered;
	struct rv_scene_at_result result;
	if (rv_scene_at(&server.scene, cursor->wlr_cursor->x, cursor->wlr_cursor->y, &result)) {
		switch (result.data.tag) {
		case RV_SCENE_NODE_DATA_DATA_WINDOW: {
			struct rv_window *window = result.data.window;
			switch (window->impl.tag) {
			case RV_WINDOW_IMPL_TOPLEVEL: {
				struct rv_xdg_toplevel *toplevel = &window->impl.toplevel;
				// Exclude input regions of the toplevel that extend beyond the window
				if (result.surface != NULL &&
						wlr_surface_get_root_surface(result.surface) == toplevel->wlr_toplevel->base->surface) {
					if (wlr_box_contains_point(&window->box, cursor->wlr_cursor->x, cursor->wlr_cursor->y)) {
						seat->wm_scheduled.has_hovered = true;
						seat->wm_scheduled.hovered = window->ref;
					}
				} else {
					seat->wm_scheduled.has_hovered = true;
					seat->wm_scheduled.hovered = window->ref;
				}
				break;
			}
			case RV_WINDOW_IMPL_XWAYLAND:
				seat->wm_scheduled.has_hovered = true;
				seat->wm_scheduled.hovered = window->ref;
				break;
			case RV_WINDOW_IMPL_DESTROYING:
				break;
			}
			break;
		}
		case RV_SCENE_NODE_DATA_DATA_SHELL_SURFACE:
		case RV_SCENE_NODE_DATA_DATA_LOCK_SURFACE:
		case RV_SCENE_NODE_DATA_DATA_LAYER_SURFACE:
			seat->wm_scheduled.has_hovered = false;
			break;
		case RV_SCENE_NODE_DATA_DATA_OVERRIDE_REDIRECT:
#if WLR_HAS_XWAYLAND
			assert(server.xwayland != NULL);
#else
			assert(false);
#endif
			seat->wm_scheduled.has_hovered = false;
			break;
		}
	} else {
		seat->wm_scheduled.has_hovered = false;
	}

	// ?Window.Ref inequality: one null and the other not, or both set with different keys.
	const bool changed = seat->wm_scheduled.has_hovered != old_has ||
		(old_has && !rv_window_ref_eql(seat->wm_scheduled.hovered, old));
	if (changed) {
		rv_window_manager_dirty_windowing(&server.wm);
	}
}

void rv_cursor_process_motion_absolute(struct rv_cursor *cursor,
		const struct rv_seat_event_pointer_motion_absolute *event) {
	struct wlr_box mapping = event->mapping;
	if (wlr_box_empty(&mapping)) {
		wlr_output_layout_get_box(server.om.output_layout, NULL, &mapping);
	}
	const double lx = (double)mapping.x + (double)mapping.width * event->x;
	const double ly = (double)mapping.y + (double)mapping.height * event->y;
	const double dx = lx - cursor->wlr_cursor->x;
	const double dy = ly - cursor->wlr_cursor->y;
	rv_cursor_process_motion_relative(cursor, &(const struct rv_seat_event_pointer_motion_relative){
		.mapping = event->mapping,
		.time_msec = event->time_msec,
		.delta_x = dx,
		.delta_y = dy,
		.unaccel_dx = dx,
		.unaccel_dy = dy,
	});
}

void rv_cursor_process_button(struct rv_cursor *cursor, const struct rv_seat_event_pointer_button *event) {
	if (event->state == WL_POINTER_BUTTON_STATE_PRESSED) {
		// getOrPut
		size_t index;
		if (pressed_find(cursor, event->button, &index)) {
			rv_err(RV_SCOPE_INPUT, "ignoring duplicate pointer button %u press", event->button);
			return;
		}
		const struct rv_cursor_pressed_entry entry = { .button = event->button, .binding = NULL };
		if (!rv_array_append(&cursor->pressed, entry)) {
			rv_err(RV_SCOPE_INPUT, "out of memory");
			return;
		}
		index = cursor->pressed.len - 1;

		struct rv_pointer_binding *binding = rv_seat_match_pointer_binding(cursor->seat, event->button);
		if (binding != NULL) {
			cursor->pressed.items[index].binding = binding;
			rv_pointer_binding_pressed(binding);
			switch (cursor->mode.tag) {
			case RV_CURSOR_MODE_PASSTHROUGH:
			case RV_CURSOR_MODE_DRAG:
			case RV_CURSOR_MODE_DOWN:
				rv_debug(RV_SCOPE_INPUT, "entering cursor mode ignore");
				cursor->mode = (struct rv_cursor_mode){ .tag = RV_CURSOR_MODE_IGNORE };
				clear_focus(cursor);
				break;
			// It is important that we do not enter ignore mode if an op is in progress,
			// doing so would result in op_release never being sent for example.
			case RV_CURSOR_MODE_OP:
			case RV_CURSOR_MODE_IGNORE:
				break;
			}
			return;
		}

		cursor->pressed.items[index].binding = NULL;

		switch (cursor->mode.tag) {
		case RV_CURSOR_MODE_PASSTHROUGH: {
			struct rv_scene_at_result at;
			if (rv_scene_at(&server.scene, cursor->wlr_cursor->x, cursor->wlr_cursor->y, &at)) {
				interact(cursor, &at);

				if (at.surface != NULL) {
					wlr_seat_pointer_notify_button(cursor->seat->wlr_seat, event->time_msec, event->button,
						event->state);
					rv_debug(RV_SCOPE_INPUT, "entering cursor mode down");
					cursor->mode = (struct rv_cursor_mode){
						.tag = RV_CURSOR_MODE_DOWN,
						.down = {
							.lx = cursor->wlr_cursor->x,
							.ly = cursor->wlr_cursor->y,
							.sx = at.sx,
							.sy = at.sy,
						},
					};
					return;
				}
			}

			rv_debug(RV_SCOPE_INPUT, "entering cursor mode ignore");
			cursor->mode = (struct rv_cursor_mode){ .tag = RV_CURSOR_MODE_IGNORE };
			clear_focus(cursor);
			return;
		}
		case RV_CURSOR_MODE_DRAG: {
			struct rv_scene_at_result at;
			if (rv_scene_at(&server.scene, cursor->wlr_cursor->x, cursor->wlr_cursor->y, &at)) {
				interact(cursor, &at);
				if (at.surface != NULL) {
					wlr_seat_pointer_notify_button(cursor->seat->wlr_seat, event->time_msec, event->button,
						event->state);
					return;
				}
			}
			clear_focus(cursor);
			break;
		}
		// Pointer focus does not change while in down mode.
		case RV_CURSOR_MODE_DOWN:
			wlr_seat_pointer_notify_button(cursor->seat->wlr_seat, event->time_msec, event->button, event->state);
			break;
		// No client has pointer focus while in ignore/op mode.
		case RV_CURSOR_MODE_IGNORE:
		case RV_CURSOR_MODE_OP:
			break;
		}
	} else {
		assert(event->state == WL_POINTER_BUTTON_STATE_RELEASED);
		// fetchRemove
		size_t index;
		if (pressed_find(cursor, event->button, &index)) {
			struct rv_pointer_binding *binding = cursor->pressed.items[index].binding;
			rv_array_swap_remove(&cursor->pressed, index);
			if (binding != NULL) {
				rv_pointer_binding_released(binding);
			}

			switch (cursor->mode.tag) {
			case RV_CURSOR_MODE_PASSTHROUGH:
				abort(); // unreachable
			case RV_CURSOR_MODE_DRAG:
			case RV_CURSOR_MODE_DOWN:
			case RV_CURSOR_MODE_IGNORE:
				if (cursor->mode.tag != RV_CURSOR_MODE_IGNORE) {
					wlr_seat_pointer_notify_button(cursor->seat->wlr_seat, event->time_msec, event->button,
						event->state);
				}
				if (cursor->pressed.len == 0) {
					rv_debug(RV_SCOPE_INPUT, "exiting cursor mode %s", mode_tag_name(cursor->mode.tag));
					cursor->mode = (struct rv_cursor_mode){ .tag = RV_CURSOR_MODE_PASSTHROUGH };
					passthrough(cursor, event->time_msec);
				}
				break;
			case RV_CURSOR_MODE_OP:
				if (cursor->pressed.len == 0) {
					cursor->seat->wm_scheduled.op_release = true;
					rv_window_manager_dirty_windowing(&server.wm);
				}
				break;
			}
		} else {
			rv_err(RV_SCOPE_INPUT, "ignoring duplicate pointer button %u release", event->button);
			return;
		}
	}
}

void rv_cursor_process_axis(struct rv_cursor *cursor, const struct rv_seat_event_pointer_axis *event) {
	wlr_seat_pointer_notify_axis(
		cursor->seat->wlr_seat,
		event->time_msec,
		event->orientation,
		event->delta,
		event->delta_discrete,
		event->source,
		event->relative_direction
	);
}

static void interact(struct rv_cursor *cursor, const struct rv_scene_at_result *result) {
	switch (result->data.tag) {
	case RV_SCENE_NODE_DATA_DATA_WINDOW: {
		struct rv_window *window = result->data.window;
		cursor->seat->wm_scheduled.interaction = (struct rv_seat_wm_scheduled_interaction){
			.tag = RV_SEAT_WM_SCHEDULED_INTERACTION_WINDOW,
			.window = window->ref,
		};
		rv_window_manager_dirty_windowing(&server.wm);
		break;
	}
	case RV_SCENE_NODE_DATA_DATA_SHELL_SURFACE: {
		struct rv_shell_surface *shell_surface = result->data.shell_surface;
		cursor->seat->wm_scheduled.interaction = (struct rv_seat_wm_scheduled_interaction){
			.tag = RV_SEAT_WM_SCHEDULED_INTERACTION_SHELL_SURFACE,
			.shell_surface = shell_surface,
		};
		rv_window_manager_dirty_windowing(&server.wm);
		break;
	}
	case RV_SCENE_NODE_DATA_DATA_LOCK_SURFACE: {
		struct rv_lock_surface *lock_surface = result->data.lock_surface;
		assert(server.lock_manager.state != RV_LOCK_MANAGER_STATE_UNLOCKED);
		rv_seat_focus(cursor->seat, (struct rv_seat_focus){
			.tag = RV_SEAT_FOCUS_LOCK_SURFACE,
			.lock_surface = lock_surface,
		});
		break;
	}
	case RV_SCENE_NODE_DATA_DATA_LAYER_SURFACE: {
		struct rv_layer_surface *layer_surface = result->data.layer_surface;
		switch (cursor->seat->layer_shell.scheduled.focus.tag) {
		case RV_LAYER_SHELL_SEAT_FOCUS_NONE:
		case RV_LAYER_SHELL_SEAT_FOCUS_NON_EXCLUSIVE:
			if (layer_surface->wlr_layer_surface->current.keyboard_interactive ==
					ZWLR_LAYER_SURFACE_V1_KEYBOARD_INTERACTIVITY_ON_DEMAND) {
				cursor->seat->layer_shell.scheduled.focus = (struct rv_layer_shell_seat_focus){
					.tag = RV_LAYER_SHELL_SEAT_FOCUS_NON_EXCLUSIVE,
					.non_exclusive = layer_surface->ref,
				};
				rv_window_manager_dirty_windowing(&server.wm);
			}
			break;
		case RV_LAYER_SHELL_SEAT_FOCUS_EXCLUSIVE:
			break;
		}
		break;
	}
	case RV_SCENE_NODE_DATA_DATA_OVERRIDE_REDIRECT: {
#if WLR_HAS_XWAYLAND
		struct rv_xwayland_override_redirect *override_redirect = result->data.override_redirect;
		assert(server.lock_manager.state != RV_LOCK_MANAGER_STATE_LOCKED);
		rv_xwayland_override_redirect_focus_if_desired(override_redirect);
#else
		abort(); // unreachable: noreturn payload without Xwayland
#endif
		break;
	}
	}
}

static void handle_touch_down(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, touch_down);
	struct wlr_touch_down_event *event = data;

	rv_seat_handle_activity(cursor->seat);

	double lx;
	double ly;
	wlr_cursor_absolute_to_layout_coords(cursor->wlr_cursor, &event->touch->base, event->x, event->y, &lx, &ly);

	// putNoClobber
	assert(touch_points_get_ptr(cursor, event->touch_id) == NULL);
	const struct rv_cursor_touch_point point = {
		.touch_id = event->touch_id,
		.point = { .lx = lx, .ly = ly },
	};
	if (!rv_array_append(&cursor->touch_points, point)) {
		rv_err(RV_SCOPE_INPUT, "out of memory");
		return;
	}

	struct rv_scene_at_result result;
	if (rv_scene_at(&server.scene, lx, ly, &result)) {
		interact(cursor, &result);

		if (result.surface != NULL) {
			struct wlr_surface *surface = result.surface;
			wlr_seat_touch_notify_down(
				cursor->seat->wlr_seat,
				surface,
				event->time_msec,
				event->touch_id,
				result.sx,
				result.sy
			);
		}
	}
}

static void handle_touch_motion(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, touch_motion);
	struct wlr_touch_motion_event *event = data;

	rv_seat_handle_activity(cursor->seat);

	struct rv_cursor_layout_point *point = touch_points_get_ptr(cursor, event->touch_id);
	if (point != NULL) {
		wlr_cursor_absolute_to_layout_coords(cursor->wlr_cursor, &event->touch->base, event->x, event->y,
			&point->lx, &point->ly);

		update_drag_icons(cursor);

		struct rv_scene_at_result result;
		if (rv_scene_at(&server.scene, point->lx, point->ly, &result)) {
			wlr_seat_touch_notify_motion(cursor->seat->wlr_seat, event->time_msec, event->touch_id,
				result.sx, result.sy);
		}
	}
}

static void handle_touch_up(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, touch_up);
	struct wlr_touch_up_event *event = data;

	rv_seat_handle_activity(cursor->seat);

	if (touch_points_remove(cursor, event->touch_id)) {
		wlr_seat_touch_notify_up(cursor->seat->wlr_seat, event->time_msec, event->touch_id);
	}
}

static void handle_touch_cancel(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, touch_cancel);

	rv_seat_handle_activity(cursor->seat);

	// clearRetainingCapacity
	cursor->touch_points.len = 0;

	struct wlr_seat *wlr_seat = cursor->seat->wlr_seat;
	while (!wl_list_empty(&wlr_seat->touch_state.touch_points)) {
		struct wlr_touch_point *touch_point =
			wl_container_of(wlr_seat->touch_state.touch_points.next, touch_point, link);
		wlr_seat_touch_notify_cancel(wlr_seat, touch_point->client);
	}
}

static void handle_touch_frame(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, touch_frame);

	rv_seat_handle_activity(cursor->seat);

	wlr_seat_touch_notify_frame(cursor->seat->wlr_seat);
}

static void handle_tablet_tool_axis(struct wl_listener *listener, void *data) {
	struct wlr_tablet_tool_axis_event *event = data;
	struct rv_input_device *device = event->tablet->base.data;
	struct rv_tablet *tablet = wl_container_of(device, tablet, device);

	rv_seat_handle_activity(device->seat);

	struct rv_tablet_tool *tool = rv_tablet_tool_get(device->seat->wlr_seat, event->tool);
	if (tool == NULL) {
		return;
	}

	rv_tablet_tool_axis(tool, tablet, event);
}

static void handle_tablet_tool_proximity(struct wl_listener *listener, void *data) {
	struct wlr_tablet_tool_proximity_event *event = data;
	struct rv_input_device *device = event->tablet->base.data;
	struct rv_tablet *tablet = wl_container_of(device, tablet, device);

	rv_seat_handle_activity(device->seat);

	struct rv_tablet_tool *tool = rv_tablet_tool_get(device->seat->wlr_seat, event->tool);
	if (tool == NULL) {
		return;
	}

	rv_tablet_tool_proximity(tool, tablet, event);
}

static void handle_tablet_tool_tip(struct wl_listener *listener, void *data) {
	struct wlr_tablet_tool_tip_event *event = data;
	struct rv_input_device *device = event->tablet->base.data;
	struct rv_tablet *tablet = wl_container_of(device, tablet, device);

	rv_seat_handle_activity(device->seat);

	struct rv_tablet_tool *tool = rv_tablet_tool_get(device->seat->wlr_seat, event->tool);
	if (tool == NULL) {
		return;
	}

	rv_tablet_tool_tip(tool, tablet, event);
}

static void handle_tablet_tool_button(struct wl_listener *listener, void *data) {
	struct wlr_tablet_tool_button_event *event = data;
	struct rv_input_device *device = event->tablet->base.data;
	struct rv_tablet *tablet = wl_container_of(device, tablet, device);

	rv_seat_handle_activity(device->seat);

	struct rv_tablet_tool *tool = rv_tablet_tool_get(device->seat->wlr_seat, event->tool);
	if (tool == NULL) {
		return;
	}

	rv_tablet_tool_button(tool, tablet, event);
}

void rv_cursor_update_state(struct rv_cursor *cursor) {
	if (cursor->constraint != NULL) {
		rv_pointer_constraint_update_state(cursor->constraint);
	}

	switch (cursor->mode.tag) {
	case RV_CURSOR_MODE_PASSTHROUGH:
	case RV_CURSOR_MODE_DRAG:
		update_hovered(cursor);
		passthrough(cursor, rv_util_msec_timestamp());
		break;
	case RV_CURSOR_MODE_IGNORE:
	case RV_CURSOR_MODE_DOWN:
	case RV_CURSOR_MODE_OP:
		break;
	}
}

// Pass an event on to the surface under the cursor, if any.
static void passthrough(struct rv_cursor *cursor, uint32_t time) {
	assert(cursor->mode.tag == RV_CURSOR_MODE_PASSTHROUGH || cursor->mode.tag == RV_CURSOR_MODE_DRAG);

	struct rv_scene_at_result result;
	if (rv_scene_at(&server.scene, cursor->wlr_cursor->x, cursor->wlr_cursor->y, &result)) {
		if (result.data.tag == RV_SCENE_NODE_DATA_DATA_LOCK_SURFACE) {
			assert(server.lock_manager.state != RV_LOCK_MANAGER_STATE_UNLOCKED);
		} else {
			assert(server.lock_manager.state != RV_LOCK_MANAGER_STATE_LOCKED);
		}

		if (result.surface != NULL) {
			struct wlr_surface *surface = result.surface;
			wlr_seat_pointer_notify_enter(cursor->seat->wlr_seat, surface, result.sx, result.sy);
			wlr_seat_pointer_notify_motion(cursor->seat->wlr_seat, time, result.sx, result.sy);
			return;
		}
	}

	clear_focus(cursor);
}

static void update_drag_icons(struct rv_cursor *cursor) {
	struct wlr_scene_node *node, *tmp;
	wl_list_for_each_safe(node, tmp, &server.scene.drag_icons->children, link) {
		struct rv_drag_icon *icon = node->data;

		if (icon->wlr_drag_icon->drag->seat == cursor->seat->wlr_seat) {
			rv_drag_icon_update_position(icon, cursor);
		}
	}
}

static void queue_motion_relative(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, motion_relative);
	struct wlr_pointer_motion_event *event = data;
	struct rv_input_device *device = event->pointer->base.data;
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){
		.tag = RV_SEAT_EVENT_POINTER_MOTION_RELATIVE,
		.pointer_motion_relative = {
			.mapping = rv_input_device_active_mapping(device),
			.time_msec = event->time_msec,
			.delta_x = event->delta_x,
			.delta_y = event->delta_y,
			.unaccel_dx = event->unaccel_dx,
			.unaccel_dy = event->unaccel_dy,
		},
	});
}

static void queue_motion_absolute(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, motion_absolute);
	struct wlr_pointer_motion_absolute_event *event = data;
	struct rv_input_device *device = event->pointer->base.data;
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){
		.tag = RV_SEAT_EVENT_POINTER_MOTION_ABSOLUTE,
		.pointer_motion_absolute = {
			.mapping = rv_input_device_active_mapping(device),
			.time_msec = event->time_msec,
			.x = event->x,
			.y = event->y,
		},
	});
}

static void queue_button(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, button);
	struct wlr_pointer_button_event *event = data;
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){
		.tag = RV_SEAT_EVENT_POINTER_BUTTON,
		.pointer_button = {
			.time_msec = event->time_msec,
			.button = event->button,
			.state = event->state,
		},
	});
}

static void queue_axis(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, axis);
	struct wlr_pointer_axis_event *event = data;
	struct rv_input_device *device = event->pointer->base.data;
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){
		.tag = RV_SEAT_EVENT_POINTER_AXIS,
		.pointer_axis = {
			.time_msec = event->time_msec,
			.source = event->source,
			.orientation = event->orientation,
			.relative_direction = event->relative_direction,
			.delta = event->delta * device->config.scroll_factor,
			.delta_discrete = lossy_cast_i32((double)event->delta_discrete * device->config.scroll_factor),
		},
	});
}

static void queue_frame(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, frame);
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){ .tag = RV_SEAT_EVENT_POINTER_FRAME });
}

static void queue_pinch_begin(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, pinch_begin);
	struct wlr_pointer_pinch_begin_event *event = data;
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){
		.tag = RV_SEAT_EVENT_POINTER_PINCH_BEGIN,
		.pointer_pinch_begin = {
			.time_msec = event->time_msec,
			.fingers = event->fingers,
		},
	});
}

static void queue_pinch_update(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, pinch_update);
	struct wlr_pointer_pinch_update_event *event = data;
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){
		.tag = RV_SEAT_EVENT_POINTER_PINCH_UPDATE,
		.pointer_pinch_update = {
			.time_msec = event->time_msec,
			.fingers = event->fingers,
			.dx = event->dx,
			.dy = event->dy,
			.scale = event->scale,
			.rotation = event->rotation,
		},
	});
}

static void queue_pinch_end(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, pinch_end);
	struct wlr_pointer_pinch_end_event *event = data;
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){
		.tag = RV_SEAT_EVENT_POINTER_PINCH_END,
		.pointer_pinch_end = {
			.time_msec = event->time_msec,
			.cancelled = event->cancelled,
		},
	});
}

static void queue_swipe_begin(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, swipe_begin);
	struct wlr_pointer_swipe_begin_event *event = data;
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){
		.tag = RV_SEAT_EVENT_POINTER_SWIPE_BEGIN,
		.pointer_swipe_begin = {
			.time_msec = event->time_msec,
			.fingers = event->fingers,
		},
	});
}

static void queue_swipe_update(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, swipe_update);
	struct wlr_pointer_swipe_update_event *event = data;
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){
		.tag = RV_SEAT_EVENT_POINTER_SWIPE_UPDATE,
		.pointer_swipe_update = {
			.time_msec = event->time_msec,
			.fingers = event->fingers,
			.dx = event->dx,
			.dy = event->dy,
		},
	});
}

static void queue_swipe_end(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, swipe_end);
	struct wlr_pointer_swipe_end_event *event = data;
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){
		.tag = RV_SEAT_EVENT_POINTER_SWIPE_END,
		.pointer_swipe_end = {
			.time_msec = event->time_msec,
			.cancelled = event->cancelled,
		},
	});
}

static void queue_hold_begin(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, hold_begin);
	struct wlr_pointer_hold_begin_event *event = data;
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){
		.tag = RV_SEAT_EVENT_POINTER_HOLD_BEGIN,
		.pointer_hold_begin = {
			.time_msec = event->time_msec,
			.fingers = event->fingers,
		},
	});
}

static void queue_hold_end(struct wl_listener *listener, void *data) {
	struct rv_cursor *cursor = wl_container_of(listener, cursor, hold_end);
	struct wlr_pointer_hold_end_event *event = data;
	rv_seat_queue_event(cursor->seat, (struct rv_seat_event){
		.tag = RV_SEAT_EVENT_POINTER_HOLD_END,
		.pointer_hold_end = {
			.time_msec = event->time_msec,
			.cancelled = event->cancelled,
		},
	});
}
