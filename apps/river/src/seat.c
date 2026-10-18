// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Seat.zig for Oath, 2026-10-18.

#include "river.h"

static void handle_request_set_selection(struct wl_listener *listener, void *data);
static void handle_request_start_drag(struct wl_listener *listener, void *data);
static void handle_start_drag(struct wl_listener *listener, void *data);
static void handle_drag_destroy(struct wl_listener *listener, void *data);
static void handle_request_set_primary_selection(struct wl_listener *listener, void *data);
static void handle_destroy(struct wl_resource *seat_v1);
static void destroy_object_(struct rv_seat *seat);
static void keyboard_notify_enter(struct rv_seat *seat, struct wlr_surface *wlr_surface);
static const struct river_seat_v1_interface seat_impl;
static const struct river_seat_v1_interface seat_inert_impl;

// @tagName(event)
static const char *event_tag_name(enum rv_seat_event_tag tag) {
	switch (tag) {
	case RV_SEAT_EVENT_KEYBOARD_KEY:
		return "keyboard_key";
	case RV_SEAT_EVENT_KEYBOARD_MODIFIERS:
		return "keyboard_modifiers";
	case RV_SEAT_EVENT_KEYBOARD_KEYMAP:
		return "keyboard_keymap";
	case RV_SEAT_EVENT_POINTER_MOTION_RELATIVE:
		return "pointer_motion_relative";
	case RV_SEAT_EVENT_POINTER_MOTION_ABSOLUTE:
		return "pointer_motion_absolute";
	case RV_SEAT_EVENT_POINTER_BUTTON:
		return "pointer_button";
	case RV_SEAT_EVENT_POINTER_AXIS:
		return "pointer_axis";
	case RV_SEAT_EVENT_POINTER_FRAME:
		return "pointer_frame";
	case RV_SEAT_EVENT_POINTER_SWIPE_BEGIN:
		return "pointer_swipe_begin";
	case RV_SEAT_EVENT_POINTER_SWIPE_UPDATE:
		return "pointer_swipe_update";
	case RV_SEAT_EVENT_POINTER_SWIPE_END:
		return "pointer_swipe_end";
	case RV_SEAT_EVENT_POINTER_PINCH_BEGIN:
		return "pointer_pinch_begin";
	case RV_SEAT_EVENT_POINTER_PINCH_UPDATE:
		return "pointer_pinch_update";
	case RV_SEAT_EVENT_POINTER_PINCH_END:
		return "pointer_pinch_end";
	case RV_SEAT_EVENT_POINTER_HOLD_BEGIN:
		return "pointer_hold_begin";
	case RV_SEAT_EVENT_POINTER_HOLD_END:
		return "pointer_hold_end";
	}
	abort();
}

// std.Deque(Event).pushBackBounded(): fails when the queue is full.
static bool event_queue_push_back_bounded(struct rv_seat_event_queue *queue, struct rv_seat_event event) {
	if (queue->len == queue->cap) {
		return false;
	}
	queue->items[(queue->head + queue->len) % queue->cap] = event;
	queue->len += 1;
	return true;
}

// std.Deque(Event).popFront(): returns false (Zig null) when the queue is empty.
static bool event_queue_pop_front(struct rv_seat_event_queue *queue, struct rv_seat_event *event) {
	if (queue->len == 0) {
		return false;
	}
	*event = queue->items[queue->head];
	queue->head = (queue->head + 1) % queue->cap;
	queue->len -= 1;
	return true;
}

// std.math.lossyCast(i32, value) for an f64 value.
static int32_t lossy_cast_i32(double value) {
	if (value != value) { // isNan
		return 0;
	} else if (value >= (double)INT32_MAX) {
		return INT32_MAX;
	} else if (value <= (double)INT32_MIN) {
		return INT32_MIN;
	} else {
		return (int32_t)value;
	}
}

// ?Window.Ref == ?Window.Ref
static bool window_ref_opt_eql(bool has_a, struct rv_window_ref a, bool has_b, struct rv_window_ref b) {
	if (!has_a || !has_b) {
		return has_a == has_b;
	}
	return rv_window_ref_eql(a, b);
}

// std.meta.eql(Focus, Focus)
static bool focus_eql(struct rv_seat_focus a, struct rv_seat_focus b) {
	if (a.tag != b.tag) {
		return false;
	}
	switch (a.tag) {
	case RV_SEAT_FOCUS_NONE:
		return true;
	case RV_SEAT_FOCUS_WINDOW:
		return a.window == b.window;
	case RV_SEAT_FOCUS_SHELL_SURFACE:
		return a.shell_surface == b.shell_surface;
	case RV_SEAT_FOCUS_OVERRIDE_REDIRECT:
		return a.override_redirect == b.override_redirect;
	case RV_SEAT_FOCUS_LOCK_SURFACE:
		return a.lock_surface == b.lock_surface;
	case RV_SEAT_FOCUS_LAYER_SURFACE:
		return a.layer_surface == b.layer_surface;
	}
	abort();
}

struct wlr_surface *rv_seat_focus_surface(struct rv_seat_focus target) {
	switch (target.tag) {
	case RV_SEAT_FOCUS_WINDOW:
		return rv_window_root_surface(target.window);
	case RV_SEAT_FOCUS_SHELL_SURFACE:
		return target.shell_surface->surface;
	case RV_SEAT_FOCUS_OVERRIDE_REDIRECT:
#if WLR_HAS_XWAYLAND
		return target.override_redirect->xsurface->surface;
#else
		abort(); // noreturn payload
#endif
	case RV_SEAT_FOCUS_LOCK_SURFACE:
		return target.lock_surface->wlr_lock_surface->surface;
	case RV_SEAT_FOCUS_LAYER_SURFACE:
		return target.layer_surface->wlr_layer_surface->surface;
	case RV_SEAT_FOCUS_NONE:
		return NULL;
	}
	abort();
}

bool rv_seat_create(const char *name) {
	struct rv_seat *seat = calloc(1, sizeof(*seat));
	if (seat == NULL) {
		return false;
	}

	// Empirically, this limit is not hit in practice unless the window manager hangs.
	// TODO have better reasoning for choosing this capacity.
	struct rv_seat_event *event_queue_items = calloc(RV_SEAT_EVENT_QUEUE_CAPACITY, sizeof(*event_queue_items));
	if (event_queue_items == NULL) {
		free(seat);
		return false;
	}

	struct wlr_seat *wlr_seat = wlr_seat_create(server.wl_server, name);
	if (wlr_seat == NULL) {
		free(event_queue_items);
		free(seat);
		return false;
	}

	*seat = (struct rv_seat){
		.wlr_seat = wlr_seat,
		.destroying = false,
		.object = NULL,
		.event_queue = {
			.items = event_queue_items,
			.head = 0,
			.len = 0,
			.cap = RV_SEAT_EVENT_QUEUE_CAPACITY,
		},
		.has_op = false,
		.focused = { .tag = RV_SEAT_FOCUS_NONE },
		.drag = RV_SEAT_DRAG_NONE,
	};
	seat->layer_shell.scheduled.focus.tag = RV_LAYER_SHELL_SEAT_FOCUS_NONE;
	seat->layer_shell.sent.focus.tag = RV_LAYER_SHELL_SEAT_FOCUS_NONE;
	seat->request_set_selection.notify = handle_request_set_selection;
	seat->request_start_drag.notify = handle_request_start_drag;
	seat->start_drag.notify = handle_start_drag;
	seat->drag_destroy.notify = handle_drag_destroy;
	seat->request_set_primary_selection.notify = handle_request_set_primary_selection;
	seat->wlr_seat->data = seat;

	wl_list_insert(server.input_manager.seats.prev, &seat->link);
	wl_list_init(&seat->link_sent);
	rv_window_manager_dirty_windowing(&server.wm);

	wl_list_init(&seat->xkb_bindings);
	wl_list_init(&seat->pointer_bindings);

	if (!rv_cursor_init(&seat->cursor, seat)) {
		// As in the Zig errdefers: only the event queue and the seat are freed;
		// the seat stays linked in InputManager.seats and the wlr_seat is not destroyed.
		free(event_queue_items);
		free(seat);
		return false;
	}
	rv_input_relay_init(&seat->relay);

	wl_list_init(&seat->keyboard_groups);

	wl_signal_add(&seat->wlr_seat->events.request_set_selection, &seat->request_set_selection);
	wl_signal_add(&seat->wlr_seat->events.request_start_drag, &seat->request_start_drag);
	wl_signal_add(&seat->wlr_seat->events.start_drag, &seat->start_drag);
	wl_signal_add(&seat->wlr_seat->events.request_set_primary_selection, &seat->request_set_primary_selection);
	return true;
}

void rv_seat_destroy(struct rv_seat *seat) {
	rv_seat_make_inert(seat);

	struct rv_seat_event event;
	while (event_queue_pop_front(&seat->event_queue, &event)) {
		switch (event.tag) {
		case RV_SEAT_EVENT_KEYBOARD_KEY:
			rv_keyboard_drop_event(event.keyboard_key.keyboard);
			break;
		case RV_SEAT_EVENT_KEYBOARD_MODIFIERS:
			rv_keyboard_drop_event(event.keyboard_modifiers.keyboard);
			break;
		case RV_SEAT_EVENT_KEYBOARD_KEYMAP:
			rv_keyboard_drop_event(event.keyboard_keymap.keyboard);
			xkb_keymap_unref(event.keyboard_keymap.keymap);
			break;
		case RV_SEAT_EVENT_POINTER_MOTION_RELATIVE:
		case RV_SEAT_EVENT_POINTER_MOTION_ABSOLUTE:
		case RV_SEAT_EVENT_POINTER_BUTTON:
		case RV_SEAT_EVENT_POINTER_AXIS:
		case RV_SEAT_EVENT_POINTER_FRAME:
		case RV_SEAT_EVENT_POINTER_SWIPE_BEGIN:
		case RV_SEAT_EVENT_POINTER_SWIPE_UPDATE:
		case RV_SEAT_EVENT_POINTER_SWIPE_END:
		case RV_SEAT_EVENT_POINTER_PINCH_BEGIN:
		case RV_SEAT_EVENT_POINTER_PINCH_UPDATE:
		case RV_SEAT_EVENT_POINTER_PINCH_END:
		case RV_SEAT_EVENT_POINTER_HOLD_BEGIN:
		case RV_SEAT_EVENT_POINTER_HOLD_END:
			break;
		}
	}

	{
		struct rv_input_device *device, *tmp;
		wl_list_for_each_safe(device, tmp, &server.input_manager.devices, link) {
			if (device->seat == seat) {
				rv_input_device_assign_to_seat(device, rv_input_manager_default_seat(&server.input_manager));
			}
		}
	}
	{
		struct rv_input_device *device;
		wl_list_for_each(device, &server.input_manager.devices, link) {
			assert(device->seat != seat);
		}
	}
	assert(wl_list_empty(&seat->keyboard_groups));

	wl_list_remove(&seat->link);
	wl_list_remove(&seat->link_sent);

	free(seat->event_queue.items);
	seat->event_queue = (struct rv_seat_event_queue){0};
	rv_cursor_deinit(&seat->cursor);

	wl_list_remove(&seat->request_set_selection.link);
	wl_list_remove(&seat->request_start_drag.link);
	wl_list_remove(&seat->start_drag.link);
	if (seat->drag != RV_SEAT_DRAG_NONE) {
		wl_list_remove(&seat->drag_destroy.link);
	}
	wl_list_remove(&seat->request_set_primary_selection.link);

	wlr_seat_destroy(seat->wlr_seat);
	// As in the Zig, the Seat struct itself is not freed here.
}

bool rv_seat_queue_event(struct rv_seat *seat, struct rv_seat_event event) {
	rv_seat_handle_activity(seat);

	if (!event_queue_push_back_bounded(&seat->event_queue, event)) {
		rv_err(RV_SCOPE_INPUT, "dropping %s event, no space in event queue", event_tag_name(event.tag));
		return false;
	}

	if (server.wm.state.tag == RV_WINDOW_MANAGER_STATE_IDLE) {
		rv_seat_process_events(seat);
	}
	return true;
}

void rv_seat_process_events(struct rv_seat *seat) {
	assert(server.wm.state.tag == RV_WINDOW_MANAGER_STATE_IDLE);

	// Only process events while there is no new state to be sent to the window manager.
	// The window manager might decide to change focus or redefine keyboard/pointer bindings
	// in response, which can affect further processing of events.
	while (!server.wm.scheduled.dirty) {
		assert(server.wm.state.tag == RV_WINDOW_MANAGER_STATE_IDLE);

		struct rv_seat_event event;
		if (!event_queue_pop_front(&seat->event_queue, &event)) {
			break;
		}

		struct wlr_pointer_gestures_v1 *pg = server.input_manager.pointer_gestures;
		switch (event.tag) {
		case RV_SEAT_EVENT_KEYBOARD_KEY:
			rv_keyboard_process_key(event.keyboard_key.keyboard, &event.keyboard_key.key);
			break;
		case RV_SEAT_EVENT_KEYBOARD_MODIFIERS:
			rv_keyboard_process_modifiers(event.keyboard_modifiers.keyboard, event.keyboard_modifiers.modifiers);
			break;
		case RV_SEAT_EVENT_KEYBOARD_KEYMAP:
			rv_keyboard_process_keymap(event.keyboard_keymap.keyboard, event.keyboard_keymap.keymap);
			break;

		case RV_SEAT_EVENT_POINTER_MOTION_RELATIVE:
			rv_cursor_process_motion_relative(&seat->cursor, &event.pointer_motion_relative);
			break;
		case RV_SEAT_EVENT_POINTER_MOTION_ABSOLUTE:
			rv_cursor_process_motion_absolute(&seat->cursor, &event.pointer_motion_absolute);
			break;
		case RV_SEAT_EVENT_POINTER_BUTTON:
			rv_cursor_process_button(&seat->cursor, &event.pointer_button);
			break;
		case RV_SEAT_EVENT_POINTER_AXIS:
			rv_cursor_process_axis(&seat->cursor, &event.pointer_axis);
			break;
		case RV_SEAT_EVENT_POINTER_FRAME:
			wlr_seat_pointer_notify_frame(seat->wlr_seat);
			break;

		case RV_SEAT_EVENT_POINTER_SWIPE_BEGIN: {
			struct rv_seat_event_pointer_swipe_begin *ev = &event.pointer_swipe_begin;
			wlr_pointer_gestures_v1_send_swipe_begin(pg, seat->wlr_seat, ev->time_msec, ev->fingers);
			break;
		}
		case RV_SEAT_EVENT_POINTER_SWIPE_UPDATE: {
			struct rv_seat_event_pointer_swipe_update *ev = &event.pointer_swipe_update;
			wlr_pointer_gestures_v1_send_swipe_update(pg, seat->wlr_seat, ev->time_msec, ev->dx, ev->dy);
			break;
		}
		case RV_SEAT_EVENT_POINTER_SWIPE_END: {
			struct rv_seat_event_pointer_swipe_end *ev = &event.pointer_swipe_end;
			wlr_pointer_gestures_v1_send_swipe_end(pg, seat->wlr_seat, ev->time_msec, ev->cancelled);
			break;
		}

		case RV_SEAT_EVENT_POINTER_PINCH_BEGIN: {
			struct rv_seat_event_pointer_pinch_begin *ev = &event.pointer_pinch_begin;
			wlr_pointer_gestures_v1_send_pinch_begin(pg, seat->wlr_seat, ev->time_msec, ev->fingers);
			break;
		}
		case RV_SEAT_EVENT_POINTER_PINCH_UPDATE: {
			struct rv_seat_event_pointer_pinch_update *ev = &event.pointer_pinch_update;
			wlr_pointer_gestures_v1_send_pinch_update(pg, seat->wlr_seat, ev->time_msec, ev->dx, ev->dy,
				ev->scale, ev->rotation);
			break;
		}
		case RV_SEAT_EVENT_POINTER_PINCH_END: {
			struct rv_seat_event_pointer_pinch_end *ev = &event.pointer_pinch_end;
			wlr_pointer_gestures_v1_send_pinch_end(pg, seat->wlr_seat, ev->time_msec, ev->cancelled);
			break;
		}

		case RV_SEAT_EVENT_POINTER_HOLD_BEGIN: {
			struct rv_seat_event_pointer_hold_begin *ev = &event.pointer_hold_begin;
			wlr_pointer_gestures_v1_send_hold_begin(pg, seat->wlr_seat, ev->time_msec, ev->fingers);
			break;
		}
		case RV_SEAT_EVENT_POINTER_HOLD_END: {
			struct rv_seat_event_pointer_hold_end *ev = &event.pointer_hold_end;
			wlr_pointer_gestures_v1_send_hold_end(pg, seat->wlr_seat, ev->time_msec, ev->cancelled);
			break;
		}
		}
	}
	assert(server.wm.state.tag == RV_WINDOW_MANAGER_STATE_IDLE);
}

// Send pointer_position without starting a manage sequence.
// Motion alone must not dirty windowing (protocol); tools like sola-scope
// still need live layout coordinates.
void rv_seat_send_live_pointer_position(struct rv_seat *seat) {
	struct wl_resource *seat_v1 = seat->object;
	if (seat_v1 == NULL) {
		return;
	}
	if (wl_resource_get_version(seat_v1) < 2) {
		return;
	}
	int32_t x = lossy_cast_i32(seat->cursor.wlr_cursor->x);
	int32_t y = lossy_cast_i32(seat->cursor.wlr_cursor->y);
	if (x == seat->wm_sent.x && y == seat->wm_sent.y) {
		return;
	}
	river_seat_v1_send_pointer_position(seat_v1, x, y);
	seat->wm_sent.x = x;
	seat->wm_sent.y = y;
}

void rv_seat_manage_start(struct rv_seat *seat) {
	if (seat->destroying) {
		rv_seat_destroy(seat);
		return;
	}

	rv_layer_shell_seat_manage_start(&seat->layer_shell);
	rv_xkb_bindings_seat_manage_start(&seat->xkb_bindings_seat);

	struct wl_resource *wm_v1 = server.wm.object;
	if (wm_v1 != NULL) {
		bool new = seat->object == NULL;
		struct wl_resource *seat_v1 = seat->object;
		if (seat_v1 == NULL) {
			assert(!seat->has_op);
			assert(seat->layer_shell.object == NULL);
			assert(seat->xkb_bindings_seat.object == NULL);
			assert(wl_list_empty(&seat->xkb_bindings));
			assert(wl_list_empty(&seat->pointer_bindings));

			seat_v1 = wl_resource_create(wl_resource_get_client(wm_v1), &river_seat_v1_interface,
				wl_resource_get_version(wm_v1), 0);
			if (seat_v1 == NULL) {
				rv_err(RV_SCOPE_INPUT, "out of memory");
				return; // try again next update
			}
			seat->object = seat_v1;

			wl_resource_set_implementation(seat_v1, &seat_impl, seat, handle_destroy);
			river_window_manager_v1_send_seat(wm_v1, seat_v1);

			wl_list_remove(&seat->link_sent);
			wl_list_insert(server.wm.sent.seats.prev, &seat->link_sent);
		}

		if (new) {
			river_seat_v1_send_wl_seat(seat_v1,
				wl_global_get_name(seat->wlr_seat->global, wl_resource_get_client(seat_v1)));
		}

		if (new) {
			if (seat->wm_scheduled.has_hovered) {
				struct rv_window *window = rv_window_ref_get(seat->wm_scheduled.hovered);
				if (window != NULL) {
					if (window->object != NULL) {
						river_seat_v1_send_pointer_enter(seat_v1, window->object);
						seat->wm_sent.has_hovered = seat->wm_scheduled.has_hovered;
						seat->wm_sent.hovered = seat->wm_scheduled.hovered;
					}
				}
			}
		} else if (!window_ref_opt_eql(seat->wm_scheduled.has_hovered, seat->wm_scheduled.hovered,
				seat->wm_sent.has_hovered, seat->wm_sent.hovered)) {
			if (seat->wm_sent.has_hovered) {
				river_seat_v1_send_pointer_leave(seat_v1);
				seat->wm_sent.has_hovered = false;
			}
			if (seat->wm_scheduled.has_hovered) {
				struct rv_window *window = rv_window_ref_get(seat->wm_scheduled.hovered);
				if (window != NULL) {
					if (window->object != NULL) {
						river_seat_v1_send_pointer_enter(seat_v1, window->object);
						seat->wm_sent.has_hovered = seat->wm_scheduled.has_hovered;
						seat->wm_sent.hovered = seat->wm_scheduled.hovered;
					}
				}
			}
		}

		{
			int32_t x = lossy_cast_i32(seat->cursor.wlr_cursor->x);
			int32_t y = lossy_cast_i32(seat->cursor.wlr_cursor->y);
			if (new || x != seat->wm_sent.x || y != seat->wm_sent.y) {
				if (wl_resource_get_version(seat_v1) >= 2) {
					river_seat_v1_send_pointer_position(seat_v1, x, y);
				}
				seat->wm_sent.x = x;
				seat->wm_sent.y = y;
			}
		}

		switch (seat->wm_scheduled.interaction.tag) {
		case RV_SEAT_WM_SCHEDULED_INTERACTION_NONE:
			break;
		case RV_SEAT_WM_SCHEDULED_INTERACTION_WINDOW: {
			struct rv_window *window = rv_window_ref_get(seat->wm_scheduled.interaction.window);
			if (window != NULL) {
				if (window->object != NULL) {
					river_seat_v1_send_window_interaction(seat_v1, window->object);
				}
			}
			break;
		}
		case RV_SEAT_WM_SCHEDULED_INTERACTION_SHELL_SURFACE: {
			struct rv_shell_surface *shell_surface = seat->wm_scheduled.interaction.shell_surface;
			river_seat_v1_send_shell_surface_interaction(seat_v1, shell_surface->object);
			break;
		}
		}

		if (seat->has_op) {
			struct rv_seat_op *op = &seat->op;
			river_seat_v1_send_op_delta(seat_v1, op->x - op->start_x, op->y - op->start_y);

			if (seat->wm_scheduled.op_release && !op->sent_release) {
				river_seat_v1_send_op_release(seat_v1);
				seat->wm_scheduled.op_release = false;
				op->sent_release = true;
			}
		}

		{
			struct rv_xkb_binding *binding, *tmp;
			wl_list_for_each_safe(binding, tmp, &seat->xkb_bindings, link) {
				switch (binding->wm_scheduled.state_change) {
				case RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_NONE:
					break;
				case RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_PRESSED:
					assert(!binding->sent_pressed);
					binding->sent_pressed = true;
					river_xkb_binding_v1_send_pressed(binding->object);
					break;
				case RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_STOP_REPEAT:
					assert(binding->sent_pressed);
					if (wl_resource_get_version(binding->object) >= 2) {
						river_xkb_binding_v1_send_stop_repeat(binding->object);
					}
					break;
				case RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_RELEASED:
					assert(binding->sent_pressed);
					binding->sent_pressed = false;
					river_xkb_binding_v1_send_released(binding->object);
					break;
				}
				binding->wm_scheduled.state_change = RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_NONE;
			}
		}
		{
			struct rv_pointer_binding *binding, *tmp;
			wl_list_for_each_safe(binding, tmp, &seat->pointer_bindings, link) {
				switch (binding->wm_scheduled.state_change) {
				case RV_POINTER_BINDING_WM_SCHEDULED_STATE_CHANGE_NONE:
					break;
				case RV_POINTER_BINDING_WM_SCHEDULED_STATE_CHANGE_PRESSED:
					assert(!binding->sent_pressed);
					binding->sent_pressed = true;
					river_pointer_binding_v1_send_pressed(binding->object);
					break;
				case RV_POINTER_BINDING_WM_SCHEDULED_STATE_CHANGE_RELEASED:
					assert(binding->sent_pressed);
					binding->sent_pressed = false;
					river_pointer_binding_v1_send_released(binding->object);
					break;
				}
				binding->wm_scheduled.state_change = RV_POINTER_BINDING_WM_SCHEDULED_STATE_CHANGE_NONE;
			}
		}
	}
	// Ensure we don't store an interaction that happens while no window manager
	// is connected until a new window manager connects.
	seat->wm_scheduled.interaction = (struct rv_seat_wm_scheduled_interaction){
		.tag = RV_SEAT_WM_SCHEDULED_INTERACTION_NONE,
	};
}

void rv_seat_make_inert(struct rv_seat *seat) {
	struct wl_resource *seat_v1 = seat->object;
	if (seat_v1 != NULL) {
		river_seat_v1_send_removed(seat_v1);
		wl_resource_set_implementation(seat_v1, &seat_inert_impl, NULL, NULL);
		destroy_object_(seat);
	} else {
		assert(!seat->has_op);
		assert(seat->layer_shell.object == NULL);
		assert(seat->xkb_bindings_seat.object == NULL);
		assert(wl_list_empty(&seat->xkb_bindings));
		assert(wl_list_empty(&seat->pointer_bindings));
	}
}

static void request_inert_destroy(struct wl_client *client, struct wl_resource *seat_v1) {
	wl_resource_destroy(seat_v1);
}

static void request_inert_focus_window(struct wl_client *client, struct wl_resource *seat_v1,
		struct wl_resource *window) {
}

static void request_inert_focus_shell_surface(struct wl_client *client, struct wl_resource *seat_v1,
		struct wl_resource *shell_surface) {
}

static void request_inert_clear_focus(struct wl_client *client, struct wl_resource *seat_v1) {
}

static void request_inert_op_start_pointer(struct wl_client *client, struct wl_resource *seat_v1) {
}

static void request_inert_op_end(struct wl_client *client, struct wl_resource *seat_v1) {
}

static void request_inert_get_pointer_binding(struct wl_client *client, struct wl_resource *seat_v1, uint32_t id,
		uint32_t button, uint32_t modifiers) {
}

static void request_inert_set_xcursor_theme(struct wl_client *client, struct wl_resource *seat_v1,
		const char *name, uint32_t size) {
}

static void request_inert_pointer_warp(struct wl_client *client, struct wl_resource *seat_v1, int32_t x,
		int32_t y) {
}

static const struct river_seat_v1_interface seat_inert_impl = {
	.destroy = request_inert_destroy,
	.focus_window = request_inert_focus_window,
	.focus_shell_surface = request_inert_focus_shell_surface,
	.clear_focus = request_inert_clear_focus,
	.op_start_pointer = request_inert_op_start_pointer,
	.op_end = request_inert_op_end,
	.get_pointer_binding = request_inert_get_pointer_binding,
	.set_xcursor_theme = request_inert_set_xcursor_theme,
	.pointer_warp = request_inert_pointer_warp,
};

// Seat.handleDestroy(), shared by the resource destructor and makeInert().
static void destroy_object_(struct rv_seat *seat) {
	seat->object = NULL;
	rv_seat_op_end(seat);

	rv_layer_shell_seat_make_inert(&seat->layer_shell);
	rv_xkb_bindings_seat_make_inert(&seat->xkb_bindings_seat);

	while (!wl_list_empty(&seat->xkb_bindings)) {
		struct rv_xkb_binding *binding = wl_container_of(seat->xkb_bindings.next, binding, link);
		rv_xkb_binding_destroy(binding);
	}
	while (!wl_list_empty(&seat->pointer_bindings)) {
		struct rv_pointer_binding *binding = wl_container_of(seat->pointer_bindings.next, binding, link);
		rv_pointer_binding_destroy(binding);
	}

	seat->object = NULL;
}

static void handle_destroy(struct wl_resource *seat_v1) {
	struct rv_seat *seat = wl_resource_get_user_data(seat_v1);
	destroy_object_(seat);
}

static struct rv_seat *get_seat(struct wl_resource *seat_v1) {
	struct rv_seat *seat = wl_resource_get_user_data(seat_v1);
	assert(seat->object == seat_v1);
	return seat;
}

static void request_destroy(struct wl_client *client, struct wl_resource *seat_v1) {
	(void)get_seat(seat_v1);
	wl_resource_destroy(seat_v1);
}

static void request_focus_window(struct wl_client *client, struct wl_resource *seat_v1,
		struct wl_resource *window_v1) {
	struct rv_seat *seat = get_seat(seat_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	struct rv_window *window = wl_resource_get_user_data(window_v1);
	if (window == NULL) {
		return;
	}
	seat->wm_requested.focus = (struct rv_seat_wm_requested_focus){
		.tag = RV_SEAT_WM_REQUESTED_FOCUS_WINDOW,
		.window = window->ref,
	};
}

static void request_focus_shell_surface(struct wl_client *client, struct wl_resource *seat_v1,
		struct wl_resource *shell_surface_v1) {
	struct rv_seat *seat = get_seat(seat_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	struct rv_shell_surface *shell_surface = wl_resource_get_user_data(shell_surface_v1);
	if (shell_surface == NULL) {
		return;
	}
	seat->wm_requested.focus = (struct rv_seat_wm_requested_focus){
		.tag = RV_SEAT_WM_REQUESTED_FOCUS_SHELL_SURFACE,
		.shell_surface = shell_surface,
	};
}

static void request_clear_focus(struct wl_client *client, struct wl_resource *seat_v1) {
	struct rv_seat *seat = get_seat(seat_v1);
	seat->wm_requested.focus = (struct rv_seat_wm_requested_focus){ .tag = RV_SEAT_WM_REQUESTED_FOCUS_CLEAR };
}

static void request_op_start_pointer(struct wl_client *client, struct wl_resource *seat_v1) {
	struct rv_seat *seat = get_seat(seat_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	seat->wm_requested.op = RV_SEAT_WM_REQUESTED_OP_START_POINTER;
}

static void request_op_end(struct wl_client *client, struct wl_resource *seat_v1) {
	struct rv_seat *seat = get_seat(seat_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	seat->wm_requested.op = RV_SEAT_WM_REQUESTED_OP_END;
}

static void request_get_pointer_binding(struct wl_client *client, struct wl_resource *seat_v1, uint32_t id,
		uint32_t button, uint32_t modifiers) {
	struct rv_seat *seat = get_seat(seat_v1);
	if (!rv_pointer_binding_create(seat, wl_resource_get_client(seat_v1), wl_resource_get_version(seat_v1), id,
			button, modifiers)) {
		wl_client_post_no_memory(wl_resource_get_client(seat_v1));
		rv_err(RV_SCOPE_INPUT, "out of memory");
		return;
	}
}

static void request_set_xcursor_theme(struct wl_client *client, struct wl_resource *seat_v1, const char *name,
		uint32_t size) {
	struct rv_seat *seat = get_seat(seat_v1);
	if (strcmp(name, "sola-cursor-hidden") == 0) {
		rv_cursor_set_hidden(&seat->cursor, true);
		return;
	}
	if (strcmp(name, "sola-cursor-visible") == 0) {
		rv_cursor_set_hidden(&seat->cursor, false);
		return;
	}
	if (!rv_cursor_set_theme(&seat->cursor, name, &size)) {
		// error.OutOfMemory
		wl_client_post_no_memory(wl_resource_get_client(seat_v1));
		rv_err(RV_SCOPE_INPUT, "out of memory");
		return;
	}
}

static void request_pointer_warp(struct wl_client *client, struct wl_resource *seat_v1, int32_t x, int32_t y) {
	struct rv_seat *seat = get_seat(seat_v1);
	if (!rv_window_manager_ensure_windowing(&server.wm)) {
		return;
	}
	seat->wm_requested.has_pointer_warp = true;
	seat->wm_requested.pointer_warp = (struct rv_seat_wm_requested_pointer_warp){ .x = x, .y = y };
}

static const struct river_seat_v1_interface seat_impl = {
	.destroy = request_destroy,
	.focus_window = request_focus_window,
	.focus_shell_surface = request_focus_shell_surface,
	.clear_focus = request_clear_focus,
	.op_start_pointer = request_op_start_pointer,
	.op_end = request_op_end,
	.get_pointer_binding = request_get_pointer_binding,
	.set_xcursor_theme = request_set_xcursor_theme,
	.pointer_warp = request_pointer_warp,
};

void rv_seat_manage_finish(struct rv_seat *seat) {
	rv_xkb_bindings_seat_manage_finish(&seat->xkb_bindings_seat);

	if (server.lock_manager.state != RV_LOCK_MANAGER_STATE_UNLOCKED) {
		return;
	}

	switch (seat->layer_shell.sent.focus.tag) {
	case RV_LAYER_SHELL_SEAT_FOCUS_EXCLUSIVE: {
		struct rv_layer_surface *layer_surface = rv_layer_surface_ref_get(seat->layer_shell.sent.focus.exclusive);
		if (layer_surface != NULL) {
			rv_seat_focus(seat, (struct rv_seat_focus){
				.tag = RV_SEAT_FOCUS_LAYER_SURFACE,
				.layer_surface = layer_surface,
			});
		}
		break;
	}
	case RV_LAYER_SHELL_SEAT_FOCUS_NON_EXCLUSIVE:
	case RV_LAYER_SHELL_SEAT_FOCUS_NONE:
		if (seat->wm_requested.focus.tag == RV_SEAT_WM_REQUESTED_FOCUS_NONE) {
			switch (seat->layer_shell.sent.focus.tag) {
			case RV_LAYER_SHELL_SEAT_FOCUS_EXCLUSIVE:
				abort(); // unreachable
			case RV_LAYER_SHELL_SEAT_FOCUS_NON_EXCLUSIVE: {
				struct rv_layer_surface *layer_surface =
					rv_layer_surface_ref_get(seat->layer_shell.sent.focus.non_exclusive);
				if (layer_surface != NULL) {
					rv_seat_focus(seat, (struct rv_seat_focus){
						.tag = RV_SEAT_FOCUS_LAYER_SURFACE,
						.layer_surface = layer_surface,
					});
				}
				break;
			}
			case RV_LAYER_SHELL_SEAT_FOCUS_NONE:
				break;
			}
		} else {
			switch (seat->wm_requested.focus.tag) {
			case RV_SEAT_WM_REQUESTED_FOCUS_NONE:
				abort(); // unreachable
			case RV_SEAT_WM_REQUESTED_FOCUS_CLEAR:
				rv_seat_focus(seat, (struct rv_seat_focus){ .tag = RV_SEAT_FOCUS_NONE });
				break;
			case RV_SEAT_WM_REQUESTED_FOCUS_WINDOW: {
				struct rv_window *window = rv_window_ref_get(seat->wm_requested.focus.window);
				if (window != NULL) {
					rv_seat_focus(seat, (struct rv_seat_focus){
						.tag = RV_SEAT_FOCUS_WINDOW,
						.window = window,
					});
				}
				break;
			}
			case RV_SEAT_WM_REQUESTED_FOCUS_SHELL_SURFACE:
				rv_seat_focus(seat, (struct rv_seat_focus){
					.tag = RV_SEAT_FOCUS_SHELL_SURFACE,
					.shell_surface = seat->wm_requested.focus.shell_surface,
				});
				break;
			}
			switch (seat->layer_shell.sent.focus.tag) {
			case RV_LAYER_SHELL_SEAT_FOCUS_EXCLUSIVE:
				abort(); // unreachable
			case RV_LAYER_SHELL_SEAT_FOCUS_NON_EXCLUSIVE:
				seat->layer_shell.scheduled.focus = (struct rv_layer_shell_seat_focus){
					.tag = RV_LAYER_SHELL_SEAT_FOCUS_NONE,
				};
				rv_window_manager_dirty_windowing(&server.wm);
				break;
			case RV_LAYER_SHELL_SEAT_FOCUS_NONE:
				break;
			}
		}
		break;
	}
	seat->wm_requested.focus = (struct rv_seat_wm_requested_focus){ .tag = RV_SEAT_WM_REQUESTED_FOCUS_NONE };

	switch (seat->wm_requested.op) {
	case RV_SEAT_WM_REQUESTED_OP_NONE:
		break;
	case RV_SEAT_WM_REQUESTED_OP_START_POINTER:
		if (!seat->has_op) {
			rv_debug(RV_SCOPE_INPUT, "start seat op pointer");
			seat->has_op = true;
			seat->op = (struct rv_seat_op){
				.sent_release = false,
				.input = RV_SEAT_OP_INPUT_POINTER,
				.start_x = (int32_t)seat->cursor.wlr_cursor->x,
				.start_y = (int32_t)seat->cursor.wlr_cursor->y,
				.x = (int32_t)seat->cursor.wlr_cursor->x,
				.y = (int32_t)seat->cursor.wlr_cursor->y,
			};
			rv_cursor_op_start_pointer(&seat->cursor);
		}
		break;
	case RV_SEAT_WM_REQUESTED_OP_END:
		rv_seat_op_end(seat);
		break;
	}
	seat->wm_requested.op = RV_SEAT_WM_REQUESTED_OP_NONE;

	if (seat->wm_requested.has_pointer_warp) {
		struct rv_seat_wm_requested_pointer_warp target = seat->wm_requested.pointer_warp;
		seat->wm_requested.has_pointer_warp = false;
		wlr_cursor_warp_closest(seat->cursor.wlr_cursor, NULL, (double)target.x, (double)target.y);
		rv_cursor_update_state(&seat->cursor);
	}
}

void rv_seat_focus(struct rv_seat *seat, struct rv_seat_focus new_focus) {
	// If the target is already focused, do nothing
	if (focus_eql(new_focus, seat->focused)) {
		return;
	}

	struct wlr_surface *target_surface = rv_seat_focus_surface(new_focus);

	// First clear the current focus
	switch (seat->focused.tag) {
	case RV_SEAT_FOCUS_WINDOW:
		rv_window_destroy_popups(seat->focused.window);
		break;
	case RV_SEAT_FOCUS_LAYER_SURFACE:
		rv_layer_surface_destroy_popups(seat->focused.layer_surface);
		break;
	case RV_SEAT_FOCUS_SHELL_SURFACE:
	case RV_SEAT_FOCUS_OVERRIDE_REDIRECT:
	case RV_SEAT_FOCUS_LOCK_SURFACE:
	case RV_SEAT_FOCUS_NONE:
		break;
	}

	// Set the new focus
	switch (new_focus.tag) {
	case RV_SEAT_FOCUS_WINDOW:
	case RV_SEAT_FOCUS_SHELL_SURFACE:
	case RV_SEAT_FOCUS_LAYER_SURFACE:
		assert(server.lock_manager.state != RV_LOCK_MANAGER_STATE_LOCKED);
		break;
	case RV_SEAT_FOCUS_LOCK_SURFACE:
		assert(server.lock_manager.state != RV_LOCK_MANAGER_STATE_UNLOCKED);
		break;
	case RV_SEAT_FOCUS_OVERRIDE_REDIRECT:
	case RV_SEAT_FOCUS_NONE:
		break;
	}
	seat->focused = new_focus;

	if (seat->cursor.constraint != NULL) {
		struct rv_pointer_constraint *constraint = seat->cursor.constraint;
		if (constraint->wlr_constraint->surface != target_surface) {
			if (constraint->state.tag == RV_POINTER_CONSTRAINT_STATE_ACTIVE) {
				rv_info(RV_SCOPE_INPUT, "deactivating pointer constraint for surface, keyboard focus lost");
				rv_pointer_constraint_deactivate(constraint);
			}
			seat->cursor.constraint = NULL;
		}
	}

	rv_seat_keyboard_enter_or_leave(seat, target_surface);
	rv_input_relay_focus(&seat->relay, target_surface);

	if (target_surface != NULL) {
		struct wlr_pointer_constraints_v1 *pointer_constraints = server.input_manager.pointer_constraints;
		struct wlr_pointer_constraint_v1 *wlr_constraint =
			wlr_pointer_constraints_v1_constraint_for_surface(pointer_constraints, target_surface, seat->wlr_seat);
		if (wlr_constraint != NULL) {
			if (seat->cursor.constraint != NULL) {
				assert(seat->cursor.constraint->wlr_constraint == wlr_constraint);
			} else {
				seat->cursor.constraint = wlr_constraint->data;
				assert(seat->cursor.constraint != NULL);
			}
		}
	}
}

// Send keyboard enter/leave events and handle pointer constraints
// This should never normally be called from outside of focus(), but we make an exception for
// XwaylandOverrideRedirect surfaces as they don't conform to the Wayland focus model.
void rv_seat_keyboard_enter_or_leave(struct rv_seat *seat, struct wlr_surface *target_surface) {
	if (target_surface != NULL) {
		keyboard_notify_enter(seat, target_surface);
	} else {
		wlr_seat_keyboard_notify_clear_focus(seat->wlr_seat);
	}
}

static void keyboard_notify_enter(struct rv_seat *seat, struct wlr_surface *wlr_surface) {
	struct wlr_keyboard *wlr_keyboard = wlr_seat_get_keyboard(seat->wlr_seat);
	if (wlr_keyboard != NULL) {
		struct rv_keyboard_group *group = wlr_keyboard->data;

		uint32_t keycodes[RV_KEYBOARD_GROUP_PRESSED_COUNT_MAX];
		size_t keycodes_len = 0;
		for (uint32_t i = 0; i < group->pressed.len; i++) {
			if (group->pressed.values[i].consumer.tag == RV_KEYBOARD_GROUP_KEY_CONSUMER_FOCUS) {
				assert(keycodes_len < RV_KEYBOARD_GROUP_PRESSED_COUNT_MAX); // appendAssumeCapacity
				keycodes[keycodes_len++] = group->pressed.keys[i];
			}
		}

		wlr_seat_keyboard_notify_enter(seat->wlr_seat, wlr_surface, keycodes, keycodes_len,
			&group->state.modifiers);
	} else {
		wlr_seat_keyboard_notify_enter(seat->wlr_seat, wlr_surface, NULL, 0, NULL);
	}
}

void rv_seat_handle_activity(struct rv_seat *seat) {
	wlr_idle_notifier_v1_notify_activity(server.input_manager.idle_notifier, seat->wlr_seat);
}

// Handle any user-defined mapping for passed keycode, modifiers and keyboard state
// Returns true if a mapping was run
struct rv_xkb_binding *rv_seat_match_xkb_binding(struct rv_seat *seat, xkb_keycode_t keycode, uint32_t modifiers,
		struct xkb_state *xkb_state) {
	// It is possible for more than one binding to be matched due to the
	// existence of layout-independent bindings. It is also possible due to
	// translation by xkbcommon consuming modifiers. On the swedish layout
	// for example, translating Super+Shift+Space may consume the Shift
	// modifier and confict with a binding for Super+Space. For this reason,
	// matching wihout xkbcommon translation is done first and after a match
	// has been found all further matches are ignored.
	struct rv_xkb_binding *found = NULL;

	// First check for matches without translating keysyms with xkbcommon.
	// That is, if the physical keys Mod+Shift+1 are pressed on a US layout don't
	// translate the keysym 1 to an exclamation mark. This behavior is generally
	// what is desired.
	{
		struct rv_xkb_binding *binding, *tmp;
		wl_list_for_each_safe(binding, tmp, &seat->xkb_bindings, link) {
			if (rv_xkb_binding_match(binding, keycode, modifiers, xkb_state,
					RV_XKB_BINDING_MATCH_METHOD_NO_TRANSLATE)) {
				if (found == NULL) {
					found = binding;
				} else {
					rv_debug(RV_SCOPE_INPUT, "already found a matching xkb_binding, ignoring additional match");
				}
			}
		}
	}

	// There are however some cases where it is necessary to translate keysyms
	// with xkbcommon for intuitive behavior. For example, layouts may require
	// translation with the numlock modifier to obtain keypad number keysyms
	// (e.g. KP_1).
	{
		struct rv_xkb_binding *binding, *tmp;
		wl_list_for_each_safe(binding, tmp, &seat->xkb_bindings, link) {
			if (rv_xkb_binding_match(binding, keycode, modifiers, xkb_state, RV_XKB_BINDING_MATCH_METHOD_TRANSLATE)) {
				if (found == NULL) {
					found = binding;
				} else {
					rv_debug(RV_SCOPE_INPUT, "already found a matching xkb_binding, ignoring additional match");
				}
			}
		}
	}

	return found;
}

struct rv_pointer_binding *rv_seat_match_pointer_binding(struct rv_seat *seat, uint32_t button) {
	struct wlr_keyboard *wlr_keyboard = wlr_seat_get_keyboard(seat->wlr_seat);
	if (wlr_keyboard == NULL) {
		return NULL;
	}
	uint32_t modifiers = wlr_keyboard_get_modifiers(wlr_keyboard);

	struct rv_pointer_binding *found = NULL;
	{
		struct rv_pointer_binding *binding, *tmp;
		wl_list_for_each_safe(binding, tmp, &seat->pointer_bindings, link) {
			if (rv_pointer_binding_match(binding, button, modifiers)) {
				if (found == NULL) {
					found = binding;
				} else {
					rv_debug(RV_SCOPE_INPUT, "already found a matching pointer binding, ignoring additional match");
				}
			}
		}
	}

	return found;
}

void rv_seat_op_update(struct rv_seat *seat, int32_t x, int32_t y) {
	assert(seat->has_op); // seat.op.?
	struct rv_seat_op *op = &seat->op;
	op->x = x;
	op->y = y;
	rv_window_manager_dirty_windowing_lazy(&server.wm);
}

void rv_seat_op_end(struct rv_seat *seat) {
	if (seat->has_op) {
		struct rv_seat_op op = seat->op;
		rv_debug(RV_SCOPE_INPUT, "end seat op");
		seat->has_op = false;
		switch (op.input) {
		case RV_SEAT_OP_INPUT_POINTER:
			rv_cursor_op_end_pointer(&seat->cursor);
			break;
		}
	}
}

// Zig !?*InputDevice: returns false on OOM, else stores the device (NULL for
// unsupported device types) in *device.
static bool create_device(struct rv_seat *seat, struct wlr_input_device *wlr_device, bool virtual,
		struct rv_input_device **device) {
	switch (wlr_device->type) {
	case WLR_INPUT_DEVICE_KEYBOARD: {
		struct rv_keyboard *keyboard = rv_keyboard_create(seat, wlr_device, virtual);
		if (keyboard == NULL) {
			return false;
		}
		*device = &keyboard->device;
		return true;
	}
	case WLR_INPUT_DEVICE_POINTER:
	case WLR_INPUT_DEVICE_TOUCH: {
		struct rv_input_device *new_device = calloc(1, sizeof(*new_device));
		if (new_device == NULL) {
			return false;
		}
		if (!rv_input_device_init(new_device, seat, wlr_device, virtual)) {
			free(new_device);
			return false;
		}
		*device = new_device;
		return true;
	}
	case WLR_INPUT_DEVICE_TABLET: {
		struct rv_tablet *tablet = rv_tablet_create(seat, wlr_device, virtual);
		if (tablet == NULL) {
			return false;
		}
		*device = &tablet->device;
		return true;
	}
	case WLR_INPUT_DEVICE_SWITCH:
	case WLR_INPUT_DEVICE_TABLET_PAD:
		*device = NULL; // unsupported
		return true;
	}
	abort();
}

void rv_seat_attach_new_device(struct rv_seat *seat, struct wlr_input_device *wlr_device, bool virtual) {
	struct rv_input_device *device = NULL;
	if (!create_device(seat, wlr_device, virtual, &device)) {
		// error.OutOfMemory
		rv_err(RV_SCOPE_INPUT, "out of memory");
		return;
	}
	if (device != NULL) {
		rv_seat_attach_device(seat, device);
		rv_seat_update_capabilities(seat);
	}
}

void rv_seat_attach_device(struct rv_seat *seat, struct rv_input_device *device) {
	device->seat = seat;
	switch (device->wlr_device->type) {
	case WLR_INPUT_DEVICE_KEYBOARD: {
		struct rv_keyboard *keyboard = wl_container_of(device, keyboard, device);
		rv_keyboard_set_group(keyboard);
		if (keyboard->group != NULL) {
			wlr_seat_set_keyboard(seat->wlr_seat, &keyboard->group->state);
			struct wlr_surface *wlr_surface = seat->wlr_seat->keyboard_state.focused_surface;
			if (wlr_surface != NULL) {
				keyboard_notify_enter(seat, wlr_surface);
			}
		}
		break;
	}
	// River implements pointer mappings without help from wlroots
	case WLR_INPUT_DEVICE_POINTER:
		wlr_cursor_attach_input_device(seat->cursor.wlr_cursor, device->wlr_device);
		break;
	case WLR_INPUT_DEVICE_TOUCH:
	case WLR_INPUT_DEVICE_TABLET:
		wlr_cursor_attach_input_device(seat->cursor.wlr_cursor, device->wlr_device);
		wlr_cursor_map_input_to_output(seat->cursor.wlr_cursor, device->wlr_device, device->config.map_to_output);
		wlr_cursor_map_input_to_region(seat->cursor.wlr_cursor, device->wlr_device, &device->config.map_to_rectangle);
		break;
	case WLR_INPUT_DEVICE_SWITCH:
	case WLR_INPUT_DEVICE_TABLET_PAD:
	default:
		abort(); // unreachable, unsupported
	}
}

void rv_seat_detach_device(struct rv_seat *seat, struct rv_input_device *device) {
	wlr_cursor_detach_input_device(seat->cursor.wlr_cursor, device->wlr_device);

	if (device->wlr_device->type == WLR_INPUT_DEVICE_KEYBOARD) {
		struct rv_keyboard *keyboard = wl_container_of(device, keyboard, device);
		if (keyboard->group != NULL) {
			rv_keyboard_group_unref(keyboard->group, keyboard->pressed.keys, keyboard->pressed.len);
			keyboard->group = NULL;
		}
	}
}

void rv_seat_update_capabilities(struct rv_seat *seat) {
	// Currently a cursor is always drawn even if there are no pointer input devices.
	// TODO Don't draw a cursor if there are no input devices.
	uint32_t capabilities = WL_SEAT_CAPABILITY_POINTER;

	struct rv_input_device *device, *tmp;
	wl_list_for_each_safe(device, tmp, &server.input_manager.devices, link) {
		if (device->seat == seat) {
			switch (device->wlr_device->type) {
			case WLR_INPUT_DEVICE_KEYBOARD:
				capabilities |= WL_SEAT_CAPABILITY_KEYBOARD;
				break;
			case WLR_INPUT_DEVICE_TOUCH:
				capabilities |= WL_SEAT_CAPABILITY_TOUCH;
				break;
			case WLR_INPUT_DEVICE_POINTER:
			case WLR_INPUT_DEVICE_SWITCH:
			case WLR_INPUT_DEVICE_TABLET:
				break;
			case WLR_INPUT_DEVICE_TABLET_PAD:
			default:
				abort(); // unreachable
			}
		}
	}

	wlr_seat_set_capabilities(seat->wlr_seat, capabilities);
}

static void handle_request_set_selection(struct wl_listener *listener, void *data) {
	struct rv_seat *seat = wl_container_of(listener, seat, request_set_selection);
	struct wlr_seat_request_set_selection_event *event = data;
	wlr_seat_set_selection(seat->wlr_seat, event->source, event->serial);
}

static void handle_request_start_drag(struct wl_listener *listener, void *data) {
	struct rv_seat *seat = wl_container_of(listener, seat, request_start_drag);
	struct wlr_seat_request_start_drag_event *event = data;

	// The start_drag request is ignored by wlroots if a drag is currently in progress.
	assert(seat->drag == RV_SEAT_DRAG_NONE);

	if (wlr_seat_validate_pointer_grab_serial(seat->wlr_seat, event->origin, event->serial)) {
		rv_debug(RV_SCOPE_INPUT, "starting pointer drag");
		wlr_seat_start_pointer_drag(seat->wlr_seat, event->drag, event->serial);
		return;
	}

	struct wlr_touch_point *point = NULL;
	if (wlr_seat_validate_touch_grab_serial(seat->wlr_seat, event->origin, event->serial, &point)) {
		rv_debug(RV_SCOPE_INPUT, "starting touch drag");
		wlr_seat_start_touch_drag(seat->wlr_seat, event->drag, event->serial, point);
		return;
	}

	rv_debug(RV_SCOPE_INPUT, "ignoring request to start drag, "
		"failed to validate pointer or touch serial %u", event->serial);
	if (event->drag->source != NULL) {
		wlr_data_source_destroy(event->drag->source);
	}
}

static void handle_start_drag(struct wl_listener *listener, void *data) {
	struct rv_seat *seat = wl_container_of(listener, seat, start_drag);
	struct wlr_drag *wlr_drag = data;

	assert(seat->drag == RV_SEAT_DRAG_NONE);
	switch (wlr_drag->grab_type) {
	case WLR_DRAG_GRAB_KEYBOARD_POINTER:
		seat->drag = RV_SEAT_DRAG_POINTER;
		seat->cursor.mode = (struct rv_cursor_mode){ .tag = RV_CURSOR_MODE_DRAG };
		break;
	case WLR_DRAG_GRAB_KEYBOARD_TOUCH:
		seat->drag = RV_SEAT_DRAG_TOUCH;
		break;
	case WLR_DRAG_GRAB_KEYBOARD:
	default:
		abort(); // unreachable
	}
	wl_signal_add(&wlr_drag->events.destroy, &seat->drag_destroy);

	if (wlr_drag->icon != NULL) {
		if (!rv_drag_icon_create(wlr_drag->icon, &seat->cursor)) {
			rv_err(RV_SCOPE_INPUT, "out of memory");
			wl_client_post_no_memory(wlr_drag->seat_client->client);
			return;
		}
	}
}

static void handle_drag_destroy(struct wl_listener *listener, void *data) {
	struct rv_seat *seat = wl_container_of(listener, seat, drag_destroy);
	wl_list_remove(&seat->drag_destroy.link);

	switch (seat->drag) {
	case RV_SEAT_DRAG_NONE:
		abort(); // unreachable
	case RV_SEAT_DRAG_POINTER:
		rv_cursor_update_state(&seat->cursor);
		break;
	case RV_SEAT_DRAG_TOUCH:
		break;
	}
	seat->drag = RV_SEAT_DRAG_NONE;
}

static void handle_request_set_primary_selection(struct wl_listener *listener, void *data) {
	struct rv_seat *seat = wl_container_of(listener, seat, request_set_primary_selection);
	struct wlr_seat_request_set_primary_selection_event *event = data;
	wlr_seat_set_primary_selection(seat->wlr_seat, event->source, event->serial);
}
