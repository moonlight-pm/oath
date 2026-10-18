// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Cursor.zig, TabletTool.zig, Tablet.zig,
// PointerConstraint.zig, InputRelay.zig, InputPopup.zig, TextInput.zig,
// DragIcon.zig, IdleInhibitManager.zig and IdleInhibitor.zig for Oath, 2026-10-18.
//
// Owned by the cursor area. Included only from river.h, before seat.h:
// struct rv_seat embeds struct rv_cursor and struct rv_input_relay by value.
//
// By-value cycle: struct rv_tablet (Tablet.zig, this area) embeds struct
// rv_input_device (seat area), while seat.h must come after this header.
// The struct rv_tablet definition therefore lives in seat.h right after struct
// rv_input_device; its prototypes stay here. The cursor area still owns it.

#ifndef RV_CURSOR_H
#define RV_CURSOR_H

// ---------------------------------------------------------------------------
// Cursor.zig (embedded by value in struct rv_seat as `cursor`; recover the seat
// with cursor->seat)
// ---------------------------------------------------------------------------

enum rv_cursor_mode_tag {
	RV_CURSOR_MODE_PASSTHROUGH,
	// This mode is entered when a binding is triggered and exited when there
	// are no longer any buttons pressed.
	RV_CURSOR_MODE_IGNORE,
	// A drag and drop is in progress.
	RV_CURSOR_MODE_DRAG,
	RV_CURSOR_MODE_DOWN,
	RV_CURSOR_MODE_OP,
};

// Cursor.Mode.down payload
struct rv_cursor_mode_down {
	// TODO: To handle the surface with pointer focus being moved during
	// down mode we need to store the starting location of the surface as
	// well and take that into account. This is currently not at all easy
	// to do, but moing to the wlroots scene graph will allow us to fix this.

	// Initial cursor position in layout coordinates
	double lx;
	double ly;
	// Initial cursor position in surface-local coordinates
	double sx;
	double sy;
};

// Cursor.Mode.op payload
struct rv_cursor_mode_op {
	// Window coordinates are stored as i32s as they are in logical pixels.
	// However, it is possible to move the cursor by a fraction of a
	// logical pixel and this happens in practice with low dpi, high
	// polling rate mice. Therefore we must accumulate the current
	// fractional offset of the mouse to avoid rounding down tiny
	// motions to 0.
	double delta_x; // default: 0
	double delta_y; // default: 0
};

// Cursor.Mode: union(enum) { passthrough, ignore, drag, down: struct, op: struct }
struct rv_cursor_mode {
	enum rv_cursor_mode_tag tag;
	union {
		struct rv_cursor_mode_down down;
		struct rv_cursor_mode_op op;
	};
};

enum rv_cursor_image_tag {
	// No cursor image
	RV_CURSOR_IMAGE_NONE,
	// Name of the current Xcursor shape
	RV_CURSOR_IMAGE_XCURSOR,
	// Cursor surface configured by the client
	RV_CURSOR_IMAGE_CLIENT,
};

// Cursor.Image.client payload
struct rv_cursor_image_client {
	struct wlr_surface *surface;
	int32_t hotspot_x;
	int32_t hotspot_y;
};

// Cursor.Image: union(enum) { none, xcursor: [*:0]const u8, client: struct }
struct rv_cursor_image {
	enum rv_cursor_image_tag tag;
	union {
		const char *xcursor;
		struct rv_cursor_image_client client;
	};
};

// Cursor.LayoutPoint
struct rv_cursor_layout_point {
	double lx;
	double ly;
};

// One entry of Cursor.pressed (std.AutoHashMapUnmanaged(u32, ?*PointerBinding)).
// The map is never iterated, so entry order does not matter.
struct rv_cursor_pressed_entry {
	uint32_t button;
	struct rv_pointer_binding *binding; // nullable
};

// One entry of Cursor.touch_points (std.AutoHashMapUnmanaged(i32, LayoutPoint)).
// The map is never iterated, so entry order does not matter.
struct rv_cursor_touch_point {
	int32_t touch_id;
	struct rv_cursor_layout_point point;
};

struct rv_cursor {
	// Current cursor mode as well as any state needed to implement that mode
	struct rv_cursor_mode mode; // default: .passthrough

	struct rv_seat *seat;
	struct wlr_cursor *wlr_cursor;

	// Xcursor manager for the currently configured Xcursor theme.
	struct wlr_xcursor_manager *xcursor_manager;
	// The currently rendered cursor image
	struct rv_cursor_image image; // default: .none
	// When true, the pointer is tracked but not drawn (sola-scope capture).
	bool hidden; // default: false
	struct wl_listener image_surface_destroy; // default: .init(handleImageSurfaceDestroy)
	// The most recent cursor image set by the window manager client
	struct rv_cursor_image wm_image; // default: .{ .xcursor = "default" } (NOT zero)
	struct wl_listener wm_image_surface_destroy; // default: .init(handleWmImageSurfaceDestroy)

	// The set of currently pressed pointer buttons and the corresponding pointer mapping if any.
	// std.AutoHashMapUnmanaged(u32, ?*PointerBinding) as an unordered array (getOrPut,
	// getPtr, fetchRemove, count == len). default: empty
	RV_ARRAY(struct rv_cursor_pressed_entry) pressed;

	// The pointer constraint for the surface that currently has keyboard focus, if any.
	// This constraint is not necessarily active, activation only occurs once the cursor
	// has been moved inside the constraint region.
	struct rv_pointer_constraint *constraint; // nullable

	// Keeps track of the last known location of all touch points in layout coordinates.
	// This information is necessary for proper touch dnd support if there are multiple touch points.
	// std.AutoHashMapUnmanaged(i32, LayoutPoint) as an unordered array. default: empty
	RV_ARRAY(struct rv_cursor_touch_point) touch_points;

	struct wl_listener request_set_cursor; // default: .init(handleRequestSetCursor)

	struct wl_listener motion_relative; // default: .init(queueMotionRelative)
	struct wl_listener motion_absolute; // default: .init(queueMotionAbsolute)
	struct wl_listener button; // default: .init(queueButton)
	struct wl_listener axis; // default: .init(queueAxis)
	struct wl_listener frame; // default: .init(queueFrame)

	struct wl_listener swipe_begin; // default: .init(queueSwipeBegin)
	struct wl_listener swipe_update; // default: .init(queueSwipeUpdate)
	struct wl_listener swipe_end; // default: .init(queueSwipeEnd)

	struct wl_listener pinch_begin; // default: .init(queuePinchBegin)
	struct wl_listener pinch_update; // default: .init(queuePinchUpdate)
	struct wl_listener pinch_end; // default: .init(queuePinchEnd)

	struct wl_listener hold_begin; // default: .init(queueHoldBegin)
	struct wl_listener hold_end; // default: .init(queueHoldEnd)

	struct wl_listener touch_down; // default: .init(handleTouchDown)
	struct wl_listener touch_motion; // default: .init(handleTouchMotion)
	struct wl_listener touch_up; // default: .init(handleTouchUp)
	struct wl_listener touch_cancel; // default: .init(handleTouchCancel)
	struct wl_listener touch_frame; // default: .init(handleTouchFrame)

	struct wl_listener tablet_tool_axis; // default: .init(handleTabletToolAxis)
	struct wl_listener tablet_tool_proximity; // default: .init(handleTabletToolProximity)
	struct wl_listener tablet_tool_tip; // default: .init(handleTabletToolTip)
	struct wl_listener tablet_tool_button; // default: .init(handleTabletToolButton)
};

// Zig !void: returns false on failure (OOM / wlr object creation / setTheme).
bool rv_cursor_init(struct rv_cursor *cursor, struct rv_seat *seat);
void rv_cursor_deinit(struct rv_cursor *cursor);
// Set the cursor theme for the given seat, as well as the xwayland theme if
// this is the default seat. Either argument may be null, in which case a
// default will be used. size is ?u32: pass NULL for null.
// Zig !void (error.OutOfMemory): returns false on OOM.
bool rv_cursor_set_theme(struct rv_cursor *cursor, const char *theme, const uint32_t *size);
void rv_cursor_set_hidden(struct rv_cursor *cursor, bool hidden);
void rv_cursor_set_image(struct rv_cursor *cursor, struct rv_cursor_image image);
void rv_cursor_set_wm_image(struct rv_cursor *cursor, struct rv_cursor_image wm_image);
void rv_cursor_op_start_pointer(struct rv_cursor *cursor);
void rv_cursor_op_end_pointer(struct rv_cursor *cursor);
void rv_cursor_process_motion_relative(struct rv_cursor *cursor,
	const struct rv_seat_event_pointer_motion_relative *event);
void rv_cursor_process_motion_absolute(struct rv_cursor *cursor,
	const struct rv_seat_event_pointer_motion_absolute *event);
void rv_cursor_process_button(struct rv_cursor *cursor, const struct rv_seat_event_pointer_button *event);
void rv_cursor_process_axis(struct rv_cursor *cursor, const struct rv_seat_event_pointer_axis *event);
void rv_cursor_update_state(struct rv_cursor *cursor);

// ---------------------------------------------------------------------------
// TabletTool.zig
// ---------------------------------------------------------------------------

enum rv_tablet_tool_mode_tag {
	RV_TABLET_TOOL_MODE_PASSTHROUGH,
	RV_TABLET_TOOL_MODE_DOWN,
};

// TabletTool.Mode.down payload
struct rv_tablet_tool_mode_down {
	// Initial cursor position in layout coordinates
	double lx;
	double ly;
	// Initial cursor position in surface-local coordinates
	double sx;
	double sy;
};

// TabletTool.Mode: union(enum) { passthrough, down: struct }
struct rv_tablet_tool_mode {
	enum rv_tablet_tool_mode_tag tag;
	union {
		struct rv_tablet_tool_mode_down down;
	};
};

struct rv_tablet_tool {
	struct wlr_tablet_v2_tablet_tool *wp_tool;

	struct wlr_cursor *wlr_cursor;

	struct rv_tablet_tool_mode mode; // default: .passthrough

	// A wlroots event may notify us of a change on one of these axes but not
	// include the value of the other. We must always send both values to the
	// client, which means we need to track this state.
	double tilt_x; // default: 0
	double tilt_y; // default: 0

	struct wl_listener destroy; // default: .init(handleDestroy)
	struct wl_listener set_cursor; // default: .init(handleSetCursor)
};

// Returns the tool stored in wlr_tool->data, creating it if needed.
// Zig error{OutOfMemory}!*TabletTool: returns NULL on OOM.
struct rv_tablet_tool *rv_tablet_tool_get(struct wlr_seat *wlr_seat, struct wlr_tablet_tool *wlr_tool);
bool rv_tablet_tool_allow_set_cursor(struct rv_tablet_tool *tool, struct wlr_seat_client *seat_client,
	uint32_t serial);
void rv_tablet_tool_axis(struct rv_tablet_tool *tool, struct rv_tablet *tablet,
	struct wlr_tablet_tool_axis_event *event);
void rv_tablet_tool_proximity(struct rv_tablet_tool *tool, struct rv_tablet *tablet,
	struct wlr_tablet_tool_proximity_event *event);
void rv_tablet_tool_tip(struct rv_tablet_tool *tool, struct rv_tablet *tablet,
	struct wlr_tablet_tool_tip_event *event);
void rv_tablet_tool_button(struct rv_tablet_tool *tool, struct rv_tablet *tablet,
	struct wlr_tablet_tool_button_event *event);

// ---------------------------------------------------------------------------
// Tablet.zig (struct rv_tablet is defined in seat.h, see the top of this file)
// ---------------------------------------------------------------------------

// Zig !*Tablet: returns NULL on OOM / wp tablet creation failure.
struct rv_tablet *rv_tablet_create(struct rv_seat *seat, struct wlr_input_device *wlr_device, bool virtual);
void rv_tablet_destroy(struct rv_tablet *tablet);

// ---------------------------------------------------------------------------
// PointerConstraint.zig
// ---------------------------------------------------------------------------

enum rv_pointer_constraint_state_tag {
	RV_POINTER_CONSTRAINT_STATE_INACTIVE,
	RV_POINTER_CONSTRAINT_STATE_ACTIVE,
};

// PointerConstraint.state.active payload
struct rv_pointer_constraint_state_active {
	// Node of the active constraint surface in the scene graph.
	struct wlr_scene_node *node;
	// Coordinates of the pointer on activation in the surface coordinate system.
	double sx;
	double sy;
};

// PointerConstraint.state: union(enum) { inactive, active: struct }
struct rv_pointer_constraint_state {
	enum rv_pointer_constraint_state_tag tag;
	union {
		struct rv_pointer_constraint_state_active active;
	};
};

struct rv_pointer_constraint {
	struct wlr_pointer_constraint_v1 *wlr_constraint;

	struct rv_pointer_constraint_state state; // default: .inactive

	struct wl_listener destroy; // default: .init(handleDestroy)
	struct wl_listener commit; // default: .init(handleCommit)

	struct wl_listener node_destroy; // default: .init(handleNodeDestroy)
};

// Zig error{OutOfMemory}!void: returns false on OOM.
bool rv_pointer_constraint_create(struct wlr_pointer_constraint_v1 *wlr_constraint);
void rv_pointer_constraint_maybe_activate(struct rv_pointer_constraint *constraint);
// Called when the cursor position or content in the scene graph changes
void rv_pointer_constraint_update_state(struct rv_pointer_constraint *constraint);
void rv_pointer_constraint_confine(struct rv_pointer_constraint *constraint, double *dx, double *dy);
void rv_pointer_constraint_deactivate(struct rv_pointer_constraint *constraint);

// ---------------------------------------------------------------------------
// InputRelay.zig (embedded by value in struct rv_seat as `relay`; recover the
// seat with wl_container_of(relay, seat, relay))
// ---------------------------------------------------------------------------

struct rv_input_relay {
	// List of all text input objects for the seat.
	// Multiple text input objects may be created per seat, even multiple from the same client.
	// However, only one text input per seat may be enabled at a time.
	struct wl_list text_inputs; // struct rv_text_input.link

	// The input method currently in use for this seat.
	// Only one input method per seat may be used at a time and if one is
	// already in use new input methods are ignored.
	// If this is null, no text input enter events will be sent.
	struct wlr_input_method_v2 *input_method; // nullable
	struct wl_list input_popups; // struct rv_input_popup.link
	// The currently enabled text input for the currently focused surface.
	// Always null if there is no input method.
	struct rv_text_input *text_input; // nullable

	struct wl_listener input_method_commit; // default: .init(handleInputMethodCommit)
	struct wl_listener grab_keyboard; // default: .init(handleInputMethodGrabKeyboard)
	struct wl_listener input_method_destroy; // default: .init(handleInputMethodDestroy)
	struct wl_listener input_method_new_popup; // default: .init(handleInputMethodNewPopup)

	struct wl_listener grab_keyboard_destroy; // default: .init(handleInputMethodGrabKeyboardDestroy)
};

void rv_input_relay_init(struct rv_input_relay *relay);
void rv_input_relay_new_input_method(struct rv_input_relay *relay, struct wlr_input_method_v2 *input_method);
void rv_input_relay_disable_text_input(struct rv_input_relay *relay);
void rv_input_relay_send_input_method_state(struct rv_input_relay *relay);
// new_focus may be NULL.
void rv_input_relay_focus(struct rv_input_relay *relay, struct wlr_surface *new_focus);

// ---------------------------------------------------------------------------
// InputPopup.zig
// ---------------------------------------------------------------------------

struct rv_input_popup {
	struct wl_list link; // InputRelay.input_popups
	struct rv_input_relay *input_relay;

	struct wlr_input_popup_surface_v2 *wlr_popup;
	struct wlr_scene_tree *surface_tree;

	struct wl_listener destroy; // default: .init(handleDestroy)
	struct wl_listener map; // default: .init(handleMap)
	struct wl_listener unmap; // default: .init(handleUnmap)
	struct wl_listener commit; // default: .init(handleCommit)
};

// Zig !void (OutOfMemory): returns false on OOM.
bool rv_input_popup_create(struct wlr_input_popup_surface_v2 *wlr_popup, struct rv_input_relay *input_relay);
void rv_input_popup_update(struct rv_input_popup *input_popup);

// ---------------------------------------------------------------------------
// TextInput.zig
// ---------------------------------------------------------------------------

struct rv_text_input {
	struct wl_list link; // InputRelay.text_inputs

	struct wlr_text_input_v3 *wlr_text_input;

	struct wl_listener enable; // default: .init(handleEnable)
	struct wl_listener commit; // default: .init(handleCommit)
	struct wl_listener disable; // default: .init(handleDisable)
	struct wl_listener destroy; // default: .init(handleDestroy)
};

// Zig !void (OutOfMemory): returns false on OOM.
bool rv_text_input_create(struct wlr_text_input_v3 *wlr_text_input);

// ---------------------------------------------------------------------------
// DragIcon.zig
// ---------------------------------------------------------------------------

// Stored in scene_drag_icon->node.data (server.scene.drag_icons children).
struct rv_drag_icon {
	struct wlr_drag_icon *wlr_drag_icon;
	struct wlr_scene_tree *scene_drag_icon;

	struct wl_listener destroy; // default: .init(handleDestroy)
};

// Zig error{OutOfMemory}!void: returns false on OOM.
bool rv_drag_icon_create(struct wlr_drag_icon *wlr_drag_icon, struct rv_cursor *cursor);
void rv_drag_icon_update_position(struct rv_drag_icon *drag_icon, struct rv_cursor *cursor);

// ---------------------------------------------------------------------------
// IdleInhibitManager.zig (embedded by value in struct rv_server as idle_inhibit_manager)
// ---------------------------------------------------------------------------

struct rv_idle_inhibit_manager {
	struct wlr_idle_inhibit_manager_v1 *wlr_manager;
	struct wl_listener new_idle_inhibitor; // default: .init(handleNewIdleInhibitor)
	struct wl_list inhibitors; // struct rv_idle_inhibitor.link
};

// Zig !void: returns false if the wlr manager could not be created.
bool rv_idle_inhibit_manager_init(struct rv_idle_inhibit_manager *inhibit_manager);
void rv_idle_inhibit_manager_deinit(struct rv_idle_inhibit_manager *inhibit_manager);
void rv_idle_inhibit_manager_check_active(struct rv_idle_inhibit_manager *inhibit_manager);

// ---------------------------------------------------------------------------
// IdleInhibitor.zig
// ---------------------------------------------------------------------------

struct rv_idle_inhibitor {
	struct rv_idle_inhibit_manager *inhibit_manager;
	struct wlr_idle_inhibitor_v1 *wlr_inhibitor;

	struct wl_listener listen_destroy; // default: .init(handleDestroy)

	struct wl_list link; // IdleInhibitManager.inhibitors
};

// Zig !void (OutOfMemory): returns false on OOM.
bool rv_idle_inhibitor_create(struct wlr_idle_inhibitor_v1 *wlr_inhibitor,
	struct rv_idle_inhibit_manager *inhibit_manager);
void rv_idle_inhibitor_destroy(struct rv_idle_inhibitor *inhibitor);

#endif
