// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Seat.zig, KeyboardGroup.zig, Keyboard.zig,
// InputManager.zig, InputDevice.zig, XkbBinding.zig, XkbBindingsSeat.zig,
// XkbBindings.zig and PointerBinding.zig for Oath, 2026-10-18.
//
// Owned by the seat area. Included only from rivulet.h, after config.h
// (struct rv_input_device embeds struct rv_libinput_device and struct
// rv_xkb_keyboard), output.h (struct rv_seat embeds struct rv_layer_shell_seat),
// window.h (struct rv_window_ref) and cursor.h (struct rv_cursor and struct
// rv_input_relay).
//
// This header also carries the definition of struct rv_tablet, which belongs to
// the cursor area (Tablet.zig): it embeds struct rv_input_device by value and
// cursor.h has to come before this header. See cursor.h for its prototypes.

#ifndef RV_SEAT_H
#define RV_SEAT_H

// ---------------------------------------------------------------------------
// XkbBindingsSeat.zig (embedded by value in struct rv_seat as xkb_bindings_seat)
// ---------------------------------------------------------------------------

// XkbBindingsSeat.scheduled.mods_update payload
struct rv_xkb_bindings_seat_scheduled_mods_update {
	uint32_t old; // river.SeatV1.Modifiers: RIVER_SEAT_V1_MODIFIERS_* bits
	uint32_t new; // river.SeatV1.Modifiers: RIVER_SEAT_V1_MODIFIERS_* bits
};

struct rv_xkb_bindings_seat_scheduled {
	bool ate_unbound_key; // default: false
	// ?struct { old, new }, default null
	bool has_mods_update;
	struct rv_xkb_bindings_seat_scheduled_mods_update mods_update;
};

enum rv_xkb_bindings_seat_requested_next_key_change {
	RV_XKB_BINDINGS_SEAT_REQUESTED_NEXT_KEY_CHANGE_NONE,
	RV_XKB_BINDINGS_SEAT_REQUESTED_NEXT_KEY_CHANGE_ENSURE_EATEN,
	RV_XKB_BINDINGS_SEAT_REQUESTED_NEXT_KEY_CHANGE_CANCEL_ENSURE_EATEN,
};

// XkbBindingsSeat.requested. Its private `init` (.none, .{}) is all-zero.
struct rv_xkb_bindings_seat_requested {
	enum rv_xkb_bindings_seat_requested_next_key_change next_key_change;
	uint32_t mods_watched; // river.SeatV1.Modifiers: RIVER_SEAT_V1_MODIFIERS_* bits
};

struct rv_xkb_bindings_seat {
	struct wl_resource *object; // river_xkb_bindings_seat_v1, nullable

	struct rv_xkb_bindings_seat_scheduled scheduled; // default: .{}
	struct rv_xkb_bindings_seat_requested requested; // default: .init

	bool ensure_next_key_eaten; // default: false
};

void rv_xkb_bindings_seat_create_object(struct rv_xkb_bindings_seat *bindings_seat, struct wl_client *client,
	uint32_t version, uint32_t id);
void rv_xkb_bindings_seat_make_inert(struct rv_xkb_bindings_seat *bindings_seat);
void rv_xkb_bindings_seat_manage_start(struct rv_xkb_bindings_seat *bindings_seat);
void rv_xkb_bindings_seat_manage_finish(struct rv_xkb_bindings_seat *bindings_seat);

// ---------------------------------------------------------------------------
// Seat.Event
// ---------------------------------------------------------------------------

// Seat.Event.keyboard_key payload
struct rv_seat_event_keyboard_key {
	struct rv_keyboard *keyboard;
	struct wlr_keyboard_key_event key;
};

// Seat.Event.keyboard_modifiers payload
struct rv_seat_event_keyboard_modifiers {
	struct rv_keyboard *keyboard;
	struct wlr_keyboard_modifiers modifiers;
};

// Seat.Event.keyboard_keymap payload. This event is really just for virtual
// keyboards, which set their own keymaps. The queue holds a keymap reference.
struct rv_seat_event_keyboard_keymap {
	struct rv_keyboard *keyboard;
	struct xkb_keymap *keymap;
};

// Seat.Event.PointerMotionRelative
struct rv_seat_event_pointer_motion_relative {
	struct wlr_box mapping;
	uint32_t time_msec;
	double delta_x;
	double delta_y;
	double unaccel_dx;
	double unaccel_dy;
};

// Seat.Event.PointerMotionAbsolute
struct rv_seat_event_pointer_motion_absolute {
	struct wlr_box mapping;
	uint32_t time_msec;
	double x;
	double y;
};

// Seat.Event.PointerButton
struct rv_seat_event_pointer_button {
	uint32_t time_msec;
	uint32_t button;
	enum wl_pointer_button_state state;
};

// Seat.Event.PointerAxis
struct rv_seat_event_pointer_axis {
	uint32_t time_msec;
	enum wl_pointer_axis_source source;
	enum wl_pointer_axis orientation;
	enum wl_pointer_axis_relative_direction relative_direction;
	double delta;
	int32_t delta_discrete;
};

// Seat.Event.PointerSwipeBegin
struct rv_seat_event_pointer_swipe_begin {
	uint32_t time_msec;
	uint32_t fingers;
};

// Seat.Event.PointerSwipeUpdate
struct rv_seat_event_pointer_swipe_update {
	uint32_t time_msec;
	uint32_t fingers;
	double dx;
	double dy;
};

// Seat.Event.PointerSwipeEnd
struct rv_seat_event_pointer_swipe_end {
	uint32_t time_msec;
	bool cancelled;
};

// Seat.Event.PointerPinchBegin
struct rv_seat_event_pointer_pinch_begin {
	uint32_t time_msec;
	uint32_t fingers;
};

// Seat.Event.PointerPinchUpdate
struct rv_seat_event_pointer_pinch_update {
	uint32_t time_msec;
	uint32_t fingers;
	double dx;
	double dy;
	double scale;
	double rotation;
};

// Seat.Event.PointerPinchEnd
struct rv_seat_event_pointer_pinch_end {
	uint32_t time_msec;
	bool cancelled;
};

// Seat.Event.PointerHoldBegin
struct rv_seat_event_pointer_hold_begin {
	uint32_t time_msec;
	uint32_t fingers;
};

// Seat.Event.PointerHoldEnd
struct rv_seat_event_pointer_hold_end {
	uint32_t time_msec;
	bool cancelled;
};

// Zig order; @tagName(event) is the lower-case name after RV_SEAT_EVENT_
// (e.g. "pointer_motion_relative").
enum rv_seat_event_tag {
	RV_SEAT_EVENT_KEYBOARD_KEY,
	RV_SEAT_EVENT_KEYBOARD_MODIFIERS,
	RV_SEAT_EVENT_KEYBOARD_KEYMAP,

	RV_SEAT_EVENT_POINTER_MOTION_RELATIVE,
	RV_SEAT_EVENT_POINTER_MOTION_ABSOLUTE,
	RV_SEAT_EVENT_POINTER_BUTTON,
	RV_SEAT_EVENT_POINTER_AXIS,
	RV_SEAT_EVENT_POINTER_FRAME, // void payload

	RV_SEAT_EVENT_POINTER_SWIPE_BEGIN,
	RV_SEAT_EVENT_POINTER_SWIPE_UPDATE,
	RV_SEAT_EVENT_POINTER_SWIPE_END,

	RV_SEAT_EVENT_POINTER_PINCH_BEGIN,
	RV_SEAT_EVENT_POINTER_PINCH_UPDATE,
	RV_SEAT_EVENT_POINTER_PINCH_END,

	RV_SEAT_EVENT_POINTER_HOLD_BEGIN,
	RV_SEAT_EVENT_POINTER_HOLD_END,
};

// Seat.Event: union(enum)
struct rv_seat_event {
	enum rv_seat_event_tag tag;
	union {
		struct rv_seat_event_keyboard_key keyboard_key;
		struct rv_seat_event_keyboard_modifiers keyboard_modifiers;
		struct rv_seat_event_keyboard_keymap keyboard_keymap;

		struct rv_seat_event_pointer_motion_relative pointer_motion_relative;
		struct rv_seat_event_pointer_motion_absolute pointer_motion_absolute;
		struct rv_seat_event_pointer_button pointer_button;
		struct rv_seat_event_pointer_axis pointer_axis;

		struct rv_seat_event_pointer_swipe_begin pointer_swipe_begin;
		struct rv_seat_event_pointer_swipe_update pointer_swipe_update;
		struct rv_seat_event_pointer_swipe_end pointer_swipe_end;

		struct rv_seat_event_pointer_pinch_begin pointer_pinch_begin;
		struct rv_seat_event_pointer_pinch_update pointer_pinch_update;
		struct rv_seat_event_pointer_pinch_end pointer_pinch_end;

		struct rv_seat_event_pointer_hold_begin pointer_hold_begin;
		struct rv_seat_event_pointer_hold_end pointer_hold_end;
	};
};

// Seat.event_queue: std.Deque(Event) created with initCapacity(1024) and only
// ever filled with pushBackBounded(), so the capacity never grows. A ring
// buffer: items[(head + i) % cap] for i in [0, len). pushBackBounded fails
// when len == cap; popFront returns items[head] and advances head.
#define RV_SEAT_EVENT_QUEUE_CAPACITY 1024

struct rv_seat_event_queue {
	struct rv_seat_event *items; // cap entries, allocated in Seat.create
	size_t head;
	size_t len;
	size_t cap;
};

// ---------------------------------------------------------------------------
// Seat.Focus
// ---------------------------------------------------------------------------

enum rv_seat_focus_tag {
	RV_SEAT_FOCUS_NONE,
	RV_SEAT_FOCUS_WINDOW,
	RV_SEAT_FOCUS_SHELL_SURFACE,
	// Only ever set when WLR_HAS_XWAYLAND (Zig: noreturn payload otherwise).
	RV_SEAT_FOCUS_OVERRIDE_REDIRECT,
	RV_SEAT_FOCUS_LOCK_SURFACE,
	RV_SEAT_FOCUS_LAYER_SURFACE,
};

// Seat.Focus: union(enum). std.meta.eql on two of these compares the tag and
// the active pointer.
struct rv_seat_focus {
	enum rv_seat_focus_tag tag;
	union {
		struct rv_window *window;
		struct rv_shell_surface *shell_surface;
		struct rv_xwayland_override_redirect *override_redirect;
		struct rv_lock_surface *lock_surface;
		struct rv_layer_surface *layer_surface;
	};
};

// Seat.Focus.surface()
struct wlr_surface *rv_seat_focus_surface(struct rv_seat_focus target);

// ---------------------------------------------------------------------------
// InputDevice.zig (embedded by value in struct rv_keyboard and struct
// rv_tablet as `device`; pointer and touch devices are allocated bare)
// ---------------------------------------------------------------------------

// InputDevice.config
struct rv_input_device_config {
	double scroll_factor; // default: 1.0 (NOT zero)
	struct wlr_output *map_to_output; // nullable
	struct wlr_box map_to_rectangle; // default: { 0, 0, 0, 0 }
};

struct rv_input_device {
	struct rv_seat *seat;
	struct wlr_input_device *wlr_device;
	bool virtual;
	struct wl_list objects; // river_input_device_v1 resources, linked with wl_resource_get_link()

	// Only initialised for non-virtual devices that have a libinput handle.
	struct rv_libinput_device libinput;
	// Only initialised for non-virtual keyboards.
	struct rv_xkb_keyboard xkb_keyboard;

	struct wl_listener remove; // default: .init(handleRemove)

	struct rv_input_device_config config; // default: .{}

	// InputManager.devices
	struct wl_list link;
};

// Zig !void, but the body cannot fail: always returns true.
bool rv_input_device_init(struct rv_input_device *device, struct rv_seat *seat, struct wlr_input_device *wlr_device,
	bool virtual);
// im_v1 is a river_input_manager_v1 resource.
void rv_input_device_create_object(struct rv_input_device *device, struct wl_resource *im_v1);
void rv_input_device_deinit(struct rv_input_device *device);
void rv_input_device_assign_to_seat(struct rv_input_device *device, struct rv_seat *new);
// Retuns the curretly active mapping for the device, or an empty box if
// the movement of the device is unrestricted.
struct wlr_box rv_input_device_active_mapping(const struct rv_input_device *device);

// ---------------------------------------------------------------------------
// Keyboard.zig
// ---------------------------------------------------------------------------

// KeyboardGroup.pressed_count_max.
// wlroots uses a buffer of length 32 to track pressed keys and does not track pressed
// keys beyond that limit. It seems likely that this can cause some inconsistency within
// wlroots in the case that someone has 32 fingers and the hardware supports N-key rollover.
//
// Furthermore, wlroots will continue to forward key press/release events to river if more
// than 32 keys are pressed. Therefore river chooses to ignore keypresses that would take
// the keyboard beyond 32 simultaneously pressed keys.
#define RV_KEYBOARD_GROUP_PRESSED_COUNT_MAX 32
_Static_assert(RV_KEYBOARD_GROUP_PRESSED_COUNT_MAX == WLR_KEYBOARD_KEYS_CAP,
	"pressed_count_max must match the length of wlr_keyboard.keycodes");

// Keyboard.Config
struct rv_keyboard_config {
	struct xkb_keymap *keymap; // nullable; a reference is held
	// Repeat rate in characters per second
	int32_t repeat_rate; // u31; default: 40 (NOT zero)
	// Repeat delay in milliseconds
	int32_t repeat_delay; // u31; default: 400 (NOT zero)
};

// Keyboard.pressed: std.AutoArrayHashMapUnmanaged(u32, void) with capacity
// pressed_count_max ensured up front (put only while len < max), so it is a
// fixed array in insertion order. keys() is keys[0..len]; swapRemove(k) moves
// the last key into k's slot.
struct rv_keyboard_pressed {
	uint32_t keys[RV_KEYBOARD_GROUP_PRESSED_COUNT_MAX];
	uint32_t len; // count()
};

struct rv_keyboard {
	struct rv_input_device device;
	bool device_destroyed; // default: false
	uint32_t queued_events; // default: 0

	struct rv_keyboard_config config;

	// Set of pressed keys that have been processed by processKey().
	// Not equivalent to wlr_keyboard.keycodes.
	// This state is necessary to handle removing keyboards from groups properly.
	struct rv_keyboard_pressed pressed; // default: .empty
	// Only null during initialization or due to allocation failure.
	struct rv_keyboard_group *group; // nullable

	struct wl_listener key; // default: .init(queueKey)
	struct wl_listener modifiers; // default: .init(queueModifiers)
	struct wl_listener keymap; // default: .init(queueKeymap)
};

// Zig !*Keyboard: returns NULL on OOM.
struct rv_keyboard *rv_keyboard_create(struct rv_seat *seat, struct wlr_input_device *wlr_device, bool virtual);
void rv_keyboard_set_group(struct rv_keyboard *keyboard);
void rv_keyboard_set_repeat_info(struct rv_keyboard *keyboard, int32_t rate, int32_t delay);
void rv_keyboard_set_keymap(struct rv_keyboard *keyboard, struct xkb_keymap *keymap);
void rv_keyboard_device_destroy(struct rv_keyboard *keyboard);
void rv_keyboard_drop_event(struct rv_keyboard *keyboard);
void rv_keyboard_process_key(struct rv_keyboard *keyboard, const struct wlr_keyboard_key_event *key);
void rv_keyboard_process_modifiers(struct rv_keyboard *keyboard, struct wlr_keyboard_modifiers modifiers);
// Consumes the keymap reference held by the queued event.
void rv_keyboard_process_keymap(struct rv_keyboard *keyboard, struct xkb_keymap *keymap);

// ---------------------------------------------------------------------------
// Tablet.zig (cursor area; defined here because of the by-value cycle described
// at the top of this file and in cursor.h)
// ---------------------------------------------------------------------------

struct rv_tablet {
	struct rv_input_device device;
	struct wlr_tablet_v2_tablet *wp_tablet;
};

// ---------------------------------------------------------------------------
// KeyboardGroup.zig
// ---------------------------------------------------------------------------

enum rv_keyboard_group_key_consumer_tag {
	// Builtin compositor binding, e.g. VT switching
	RV_KEYBOARD_GROUP_KEY_CONSUMER_BUILTIN,
	// A null value indicates that the xkb_binding_v1 was destroyed or that
	// a press event was already sent due to a press on a different keyboard.
	RV_KEYBOARD_GROUP_KEY_CONSUMER_BINDING,
	// The river_xkb_bindings_seat_v1.ensure_next_key_eaten request caused
	// the key to be eaten.
	RV_KEYBOARD_GROUP_KEY_CONSUMER_ENSURE_EATEN,
	RV_KEYBOARD_GROUP_KEY_CONSUMER_IM_GRAB,
	// Seat's focused client
	RV_KEYBOARD_GROUP_KEY_CONSUMER_FOCUS,
};

// KeyboardGroup.KeyConsumer: union(enum) { builtin, binding: ?*XkbBinding, ensure_eaten, im_grab, focus }
struct rv_keyboard_group_key_consumer {
	enum rv_keyboard_group_key_consumer_tag tag;
	union {
		struct rv_xkb_binding *binding; // nullable
	};
};

// KeyboardGroup.Press
struct rv_keyboard_group_press {
	struct rv_keyboard_group_key_consumer consumer;
	uint32_t count;
};

// KeyboardGroup.pressed: std.AutoArrayHashMapUnmanaged(u32, Press) with capacity
// pressed_count_max ensured up front, so it is a fixed array in insertion order.
// Maps from pressed libinput keycode (not xkb keycode) to information
// about where the press event has been sent. keys()/values() are
// keys[0..len]/values[0..len]; fetchSwapRemove(k) moves the last entry into k's slot.
struct rv_keyboard_group_pressed {
	uint32_t keys[RV_KEYBOARD_GROUP_PRESSED_COUNT_MAX];
	struct rv_keyboard_group_press values[RV_KEYBOARD_GROUP_PRESSED_COUNT_MAX];
	uint32_t len; // count()
};

struct rv_keyboard_group {
	uint32_t ref_count; // default: 1 (NOT zero)

	struct rv_seat *seat;
	// Seat.keyboard_groups
	struct wl_list link;

	bool virtual;

	struct rv_keyboard_config config;

	// This is the keyboard that actually gets passed to wlr_seat functions for
	// setting keyboard focus. Initialised with wlr_keyboard_init(); state.data = group.
	struct wlr_keyboard state;
	uint32_t modifiers_old; // wlr.Keyboard.ModifierMask (WLR_MODIFIER_* bits); default: .{} (0)

	struct rv_keyboard_group_pressed pressed; // default: .empty

	struct wl_listener key; // default: .init(handleKey)
	struct wl_listener modifiers; // default: .init(handleModifiers)
};

// Zig !*KeyboardGroup: returns NULL on OOM.
struct rv_keyboard_group *rv_keyboard_group_create(struct rv_seat *seat, struct rv_keyboard_config config,
	bool virtual);
struct rv_keyboard_group *rv_keyboard_group_ref(struct rv_keyboard_group *group);
// to_release is a []u32 slice (usually keyboard->pressed.keys[0..len]).
void rv_keyboard_group_unref(struct rv_keyboard_group *group, const uint32_t *to_release, size_t to_release_len);
bool rv_keyboard_group_match(const struct rv_keyboard_group *group, struct rv_keyboard_config *config);
void rv_keyboard_group_process_key(struct rv_keyboard_group *group, const struct wlr_keyboard_key_event *event);
void rv_keyboard_group_process_modifiers(struct rv_keyboard_group *group, struct wlr_keyboard_modifiers modifiers);
void rv_keyboard_group_process_keymap(struct rv_keyboard_group *group, struct xkb_keymap *keymap);
void rv_keyboard_group_send_state(struct rv_keyboard_group *group);

// ---------------------------------------------------------------------------
// Seat.zig
// ---------------------------------------------------------------------------

enum rv_seat_wm_scheduled_interaction_tag {
	RV_SEAT_WM_SCHEDULED_INTERACTION_NONE,
	RV_SEAT_WM_SCHEDULED_INTERACTION_WINDOW,
	RV_SEAT_WM_SCHEDULED_INTERACTION_SHELL_SURFACE,
};

// Seat.wm_scheduled.interaction: union(enum) { none, window: Window.Ref, shell_surface: *ShellSurface }
struct rv_seat_wm_scheduled_interaction {
	enum rv_seat_wm_scheduled_interaction_tag tag;
	union {
		struct rv_window_ref window;
		struct rv_shell_surface *shell_surface;
	};
};

// State to be sent to the wm in the next manage sequence. All-zero is the Zig default.
struct rv_seat_wm_scheduled {
	// The window entered/hovered by the pointer, if any (?Window.Ref, default null)
	bool has_hovered;
	struct rv_window_ref hovered;
	// The window clicked on, touched, etc.
	struct rv_seat_wm_scheduled_interaction interaction; // default: .none
	bool op_release; // default: false
};

// State sent to the wm in the latest manage sequence. All-zero is the Zig default.
struct rv_seat_wm_sent {
	// The window entered/hovered by the pointer, if any (?Window.Ref, default null)
	bool has_hovered;
	struct rv_window_ref hovered;
	int32_t x; // default: 0
	int32_t y; // default: 0
};

enum rv_seat_wm_requested_focus_tag {
	RV_SEAT_WM_REQUESTED_FOCUS_NONE,
	RV_SEAT_WM_REQUESTED_FOCUS_CLEAR,
	RV_SEAT_WM_REQUESTED_FOCUS_WINDOW,
	RV_SEAT_WM_REQUESTED_FOCUS_SHELL_SURFACE,
};

// Seat.wm_requested.focus: union(enum) { none, clear, window: Window.Ref, shell_surface: *ShellSurface }
struct rv_seat_wm_requested_focus {
	enum rv_seat_wm_requested_focus_tag tag;
	union {
		struct rv_window_ref window;
		struct rv_shell_surface *shell_surface;
	};
};

// Seat.wm_requested.op: union(enum) { none, start_pointer, end } (no payloads)
enum rv_seat_wm_requested_op {
	RV_SEAT_WM_REQUESTED_OP_NONE,
	RV_SEAT_WM_REQUESTED_OP_START_POINTER,
	RV_SEAT_WM_REQUESTED_OP_END,
};

// Seat.wm_requested.pointer_warp payload
struct rv_seat_wm_requested_pointer_warp {
	int32_t x;
	int32_t y;
};

// Windowing state requested by the wm. All-zero is the Zig default.
struct rv_seat_wm_requested {
	struct rv_seat_wm_requested_focus focus; // default: .none
	enum rv_seat_wm_requested_op op; // default: .none
	// ?struct { x: i32, y: i32 }, default null
	bool has_pointer_warp;
	struct rv_seat_wm_requested_pointer_warp pointer_warp;
};

enum rv_seat_op_input {
	RV_SEAT_OP_INPUT_POINTER,
};

// Seat.op payload (?struct, default null)
struct rv_seat_op {
	bool sent_release; // default: false
	enum rv_seat_op_input input;
	// Coordinates of the cursor/touch point/etc. at the start of the operation.
	int32_t start_x;
	int32_t start_y;
	int32_t x;
	int32_t y;
};

// The currently in progress drag operation type.
enum rv_seat_drag {
	RV_SEAT_DRAG_NONE,
	RV_SEAT_DRAG_POINTER,
	RV_SEAT_DRAG_TOUCH,
};

struct rv_seat {
	struct wlr_seat *wlr_seat; // wlr_seat->data = this seat

	struct wl_list link; // InputManager.seats

	bool destroying; // default: false

	struct wl_resource *object; // river_seat_v1, nullable
	struct rv_layer_shell_seat layer_shell; // default: .{} (focus .none, which is 0)
	struct rv_xkb_bindings_seat xkb_bindings_seat; // default: .{}

	struct rv_seat_event_queue event_queue;

	// State to be sent to the wm in the next manage sequence.
	struct rv_seat_wm_scheduled wm_scheduled; // default: .{}

	// State sent to the wm in the latest manage sequence.
	struct rv_seat_wm_sent wm_sent; // default: .{}
	struct wl_list link_sent; // WindowManager.sent.seats

	// Windowing state requested by the wm.
	struct rv_seat_wm_requested wm_requested; // default: .{}

	struct wl_list xkb_bindings; // struct rv_xkb_binding.link
	struct wl_list pointer_bindings; // struct rv_pointer_binding.link

	// Multiple physical mice are handled by the same Cursor
	struct rv_cursor cursor;

	// ?struct, default null
	bool has_op;
	struct rv_seat_op op;

	struct rv_input_relay relay;

	struct wl_list keyboard_groups; // struct rv_keyboard_group.link

	struct rv_seat_focus focused; // default: .none

	// The currently in progress drag operation type.
	enum rv_seat_drag drag; // default: .none

	struct wl_listener request_set_selection; // default: .init(handleRequestSetSelection)
	struct wl_listener request_start_drag; // default: .init(handleRequestStartDrag)
	struct wl_listener start_drag; // default: .init(handleStartDrag)
	struct wl_listener drag_destroy; // default: .init(handleDragDestroy)
	struct wl_listener request_set_primary_selection; // default: .init(handleRequestSetPrimarySelection)
};

// Zig !void: returns false on OOM (seat, event queue, wlr_seat, cursor).
bool rv_seat_create(const char *name);
void rv_seat_destroy(struct rv_seat *seat);
// Zig !void (error.QueueFull, after logging "dropping {s} event, no space in
// event queue"): returns false if the queue is full.
bool rv_seat_queue_event(struct rv_seat *seat, struct rv_seat_event event);
void rv_seat_process_events(struct rv_seat *seat);
// Send pointer_position without starting a manage sequence.
// Motion alone must not dirty windowing (protocol); tools like sola-scope
// still need live layout coordinates.
void rv_seat_send_live_pointer_position(struct rv_seat *seat);
void rv_seat_manage_start(struct rv_seat *seat);
void rv_seat_make_inert(struct rv_seat *seat);
void rv_seat_manage_finish(struct rv_seat *seat);
void rv_seat_focus(struct rv_seat *seat, struct rv_seat_focus new_focus);
// Send keyboard enter/leave events and handle pointer constraints
// This should never normally be called from outside of focus(), but we make an exception for
// XwaylandOverrideRedirect surfaces as they don't conform to the Wayland focus model.
// target_surface may be NULL.
void rv_seat_keyboard_enter_or_leave(struct rv_seat *seat, struct wlr_surface *target_surface);
void rv_seat_handle_activity(struct rv_seat *seat);
// Oath: keyboard-shortcuts-inhibit. An inhibitor is active while its surface
// has the seat's keyboard focus; update makes that so after focus changes.
void rv_seat_update_shortcuts_inhibitors(struct rv_seat *seat);
// The keyboard focus holds an active inhibitor: keys skip the window
// manager's bindings (the builtin VT switch still runs).
bool rv_seat_shortcuts_inhibited(struct rv_seat *seat);
// It also has the pointer locked or confined: buttons skip the pointer
// bindings too, so a client capturing input (a KVM, a VM viewer) gets it all.
bool rv_seat_input_captured(struct rv_seat *seat);
// Handle any user-defined mapping for passed keycode, modifiers and keyboard state
// Returns the matched binding, if any. modifiers is a wlr.Keyboard.ModifierMask.
struct rv_xkb_binding *rv_seat_match_xkb_binding(struct rv_seat *seat, xkb_keycode_t keycode, uint32_t modifiers,
	struct xkb_state *xkb_state);
struct rv_pointer_binding *rv_seat_match_pointer_binding(struct rv_seat *seat, uint32_t button);
void rv_seat_op_update(struct rv_seat *seat, int32_t x, int32_t y);
void rv_seat_op_end(struct rv_seat *seat);
void rv_seat_attach_new_device(struct rv_seat *seat, struct wlr_input_device *wlr_device, bool virtual);
void rv_seat_attach_device(struct rv_seat *seat, struct rv_input_device *device);
void rv_seat_detach_device(struct rv_seat *seat, struct rv_input_device *device);
void rv_seat_update_capabilities(struct rv_seat *seat);

// ---------------------------------------------------------------------------
// InputManager.zig (embedded by value in struct rv_server as input_manager)
// ---------------------------------------------------------------------------

struct rv_input_manager {
	struct wl_global *global;
	struct wl_list objects; // river_input_manager_v1 resources, linked with wl_resource_get_link()

	struct wl_listener new_input; // default: .init(handleNewInput)

	struct wlr_idle_notifier_v1 *idle_notifier;
	struct wlr_relative_pointer_manager_v1 *relative_pointer_manager;
	struct wlr_pointer_gestures_v1 *pointer_gestures;
	struct wlr_virtual_pointer_manager_v1 *virtual_pointer_manager;
	struct wlr_virtual_keyboard_manager_v1 *virtual_keyboard_manager;
	struct wlr_pointer_constraints_v1 *pointer_constraints;
	struct wlr_input_method_manager_v2 *input_method_manager;
	struct wlr_text_input_manager_v3 *text_input_manager;
	struct wlr_tablet_manager_v2 *tablet_manager;
	// Oath: keyboard-shortcuts-inhibit-unstable-v1, which River 0.4.5 lacks.
	struct wlr_keyboard_shortcuts_inhibit_manager_v1 *shortcuts_inhibit_manager;

	struct wl_list devices; // struct rv_input_device.link
	struct wl_list seats; // struct rv_seat.link

	struct wl_listener new_virtual_pointer; // default: .init(handleNewVirtualPointer)
	struct wl_listener new_virtual_keyboard; // default: .init(handleNewVirtualKeyboard)
	struct wl_listener new_constraint; // default: .init(handleNewConstraint)
	struct wl_listener new_input_method; // default: .init(handleNewInputMethod)
	struct wl_listener new_text_input; // default: .init(handleNewTextInput)
	struct wl_listener new_shortcuts_inhibitor; // Oath
};

// Zig !void: returns false if a global/wlroots object or the default seat could not be created.
bool rv_input_manager_init(struct rv_input_manager *input_manager);
void rv_input_manager_deinit(struct rv_input_manager *input_manager);
// The first seat in input_manager->seats (the "default" seat); never NULL.
struct rv_seat *rv_input_manager_default_seat(struct rv_input_manager *input_manager);
void rv_input_manager_process_events(struct rv_input_manager *input_manager);

// ---------------------------------------------------------------------------
// XkbBinding.zig
// ---------------------------------------------------------------------------

enum rv_xkb_binding_wm_scheduled_state_change {
	RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_NONE,
	RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_PRESSED,
	RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_STOP_REPEAT,
	RV_XKB_BINDING_WM_SCHEDULED_STATE_CHANGE_RELEASED,
};

struct rv_xkb_binding_wm_scheduled {
	enum rv_xkb_binding_wm_scheduled_state_change state_change; // default: .none
};

struct rv_xkb_binding_wm_requested {
	bool enabled; // default: false
	// This is set for mappings with layout-pinning
	// If set, the layout with this index is always used to translate the given keycode
	// (?u32, default null)
	bool has_layout;
	uint32_t layout;
};

struct rv_xkb_binding {
	struct rv_seat *seat;
	struct wl_resource *object; // river_xkb_binding_v1

	xkb_keysym_t keysym;
	uint32_t modifiers; // river.SeatV1.Modifiers: RIVER_SEAT_V1_MODIFIERS_* bits

	struct rv_xkb_binding_wm_scheduled wm_scheduled; // default: .{}
	struct rv_xkb_binding_wm_requested wm_requested; // default: .{}

	// This bit of state is used to ensure that multiple simultaneous
	// presses across multiple keyboards do not cause multiple press
	// events to be sent to the window manager.
	bool sent_pressed; // default: false

	// Seat.xkb_bindings
	struct wl_list link;
};

// XkbBinding.match() `method: enum { no_translate, translate }`
enum rv_xkb_binding_match_method {
	RV_XKB_BINDING_MATCH_METHOD_NO_TRANSLATE,
	RV_XKB_BINDING_MATCH_METHOD_TRANSLATE,
};

// Zig !void (OutOfMemory, ResourceCreateFailed): returns false on either.
bool rv_xkb_binding_create(struct rv_seat *seat, struct wl_client *client, uint32_t version, uint32_t id,
	xkb_keysym_t keysym, uint32_t modifiers);
void rv_xkb_binding_destroy(struct rv_xkb_binding *binding);
void rv_xkb_binding_pressed(struct rv_xkb_binding *binding);
void rv_xkb_binding_stop_repeat(struct rv_xkb_binding *binding);
void rv_xkb_binding_released(struct rv_xkb_binding *binding);
// Compare binding with given keycode, modifiers (wlr.Keyboard.ModifierMask) and keyboard state
bool rv_xkb_binding_match(const struct rv_xkb_binding *binding, xkb_keycode_t keycode, uint32_t modifiers,
	struct xkb_state *xkb_state, enum rv_xkb_binding_match_method method);

// ---------------------------------------------------------------------------
// XkbBindings.zig (embedded by value in struct rv_server as xkb_bindings)
// ---------------------------------------------------------------------------

struct rv_xkb_bindings {
	struct wl_global *global;

	struct wl_listener server_destroy; // default: .init(handleServerDestroy)
};

// Zig !void: returns false if the global could not be created.
bool rv_xkb_bindings_init(struct rv_xkb_bindings *bindings);

// ---------------------------------------------------------------------------
// PointerBinding.zig
// ---------------------------------------------------------------------------

enum rv_pointer_binding_wm_scheduled_state_change {
	RV_POINTER_BINDING_WM_SCHEDULED_STATE_CHANGE_NONE,
	RV_POINTER_BINDING_WM_SCHEDULED_STATE_CHANGE_PRESSED,
	RV_POINTER_BINDING_WM_SCHEDULED_STATE_CHANGE_RELEASED,
};

struct rv_pointer_binding_wm_scheduled {
	enum rv_pointer_binding_wm_scheduled_state_change state_change; // default: .none
};

struct rv_pointer_binding_wm_requested {
	bool enabled; // default: false
};

struct rv_pointer_binding {
	struct rv_seat *seat;
	struct wl_resource *object; // river_pointer_binding_v1

	uint32_t button;
	uint32_t modifiers; // river.SeatV1.Modifiers: RIVER_SEAT_V1_MODIFIERS_* bits

	struct rv_pointer_binding_wm_scheduled wm_scheduled; // default: .{}
	struct rv_pointer_binding_wm_requested wm_requested; // default: .{}

	// This bit of state is used to ensure that multiple simultaneous
	// presses across multiple keyboards do not cause multiple press
	// events to be sent to the window manager.
	bool sent_pressed; // default: false

	// Seat.pointer_bindings
	struct wl_list link;
};

// Zig !void (OutOfMemory, ResourceCreateFailed): returns false on either.
bool rv_pointer_binding_create(struct rv_seat *seat, struct wl_client *client, uint32_t version, uint32_t id,
	uint32_t button, uint32_t modifiers);
void rv_pointer_binding_destroy(struct rv_pointer_binding *binding);
void rv_pointer_binding_pressed(struct rv_pointer_binding *binding);
void rv_pointer_binding_released(struct rv_pointer_binding *binding);
// modifiers is a wlr.Keyboard.ModifierMask.
bool rv_pointer_binding_match(const struct rv_pointer_binding *binding, uint32_t button, uint32_t modifiers);

#endif
