// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Output.zig, OutputManager.zig, LayerShell.zig,
// LayerSurface.zig, LayerShellOutput.zig, LayerShellSeat.zig, LockManager.zig
// and LockSurface.zig for Oath, 2026-10-18.
//
// Owned by the output area. Included only from rivulet.h, after util.h and
// config.h. Needs nothing by value from other areas; struct rv_seat embeds
// struct rv_layer_shell_seat and struct rv_server embeds the managers, so this
// header comes before seat.h and server.h.

#ifndef RV_OUTPUT_H
#define RV_OUTPUT_H

// ---------------------------------------------------------------------------
// LayerSurface.zig
// ---------------------------------------------------------------------------

// LayerSurface.Ref: only packed in order to make == work; compare with
// rv_layer_surface_ref_eql().
struct rv_layer_surface_ref {
	struct rv_slotmap_key key; // key into server.layer_shell.surfaces
};

static inline bool rv_layer_surface_ref_eql(struct rv_layer_surface_ref a, struct rv_layer_surface_ref b) {
	return rv_slotmap_key_eql(a.key, b.key);
}

struct rv_layer_surface {
	struct rv_layer_surface_ref ref;

	struct wlr_layer_surface_v1 *wlr_layer_surface;
	struct wlr_scene_layer_surface_v1 *scene_layer_surface;
	struct wlr_scene_tree *popup_tree;

	struct wl_listener destroy; // default: .init(handleDestroy)
	struct wl_listener map; // default: .init(handleMap)
	struct wl_listener unmap; // default: .init(handleUnmap)
	struct wl_listener commit; // default: .init(handleCommit)
	struct wl_listener new_popup; // default: .init(handleNewPopup)
};

// LayerSurface.Ref.get(): server.layer_shell.surfaces.get(ref.key); NULL if gone.
struct rv_layer_surface *rv_layer_surface_ref_get(struct rv_layer_surface_ref ref);
// Zig error{OutOfMemory}!void: returns false on OOM.
bool rv_layer_surface_create(struct wlr_layer_surface_v1 *wlr_layer_surface);
void rv_layer_surface_destroy_popups(struct rv_layer_surface *layer_surface);

// ---------------------------------------------------------------------------
// LayerShellOutput.zig
// Embedded by value in struct rv_output (field `layer_shell`); recover the
// output with wl_container_of(shell_output, output, layer_shell).
// ---------------------------------------------------------------------------

struct rv_layer_shell_output_scheduled {
	struct wlr_box non_exclusive_area; // default: { 0, 0, 0, 0 }
};

struct rv_layer_shell_output_sent {
	// ?wlr.Box, default null
	bool has_non_exclusive_area;
	struct wlr_box non_exclusive_area;
};

struct rv_layer_shell_output_requested {
	bool default_; // Zig name: `default` (a C keyword); default: false
};

struct rv_layer_shell_output {
	struct wl_resource *object; // river_layer_shell_output_v1, nullable

	struct rv_layer_shell_output_scheduled scheduled;
	struct rv_layer_shell_output_sent sent;
	struct rv_layer_shell_output_requested requested;
};

void rv_layer_shell_output_create_object(struct rv_layer_shell_output *shell_output, struct wl_client *client,
	uint32_t version, uint32_t id);
void rv_layer_shell_output_make_inert(struct rv_layer_shell_output *shell_output);
void rv_layer_shell_output_arrange(struct rv_layer_shell_output *shell_output);
void rv_layer_shell_output_manage_start(struct rv_layer_shell_output *shell_output);

// ---------------------------------------------------------------------------
// LayerShellSeat.zig
// Embedded by value in struct rv_seat (field `layer_shell`).
// ---------------------------------------------------------------------------

// LayerShellSeat.Focus tag. Declaration order is the Zig order; the values are
// chosen so that 0 is the Zig default (.none). Never rely on the numbers.
enum rv_layer_shell_seat_focus_tag {
	RV_LAYER_SHELL_SEAT_FOCUS_EXCLUSIVE = 1,
	RV_LAYER_SHELL_SEAT_FOCUS_NON_EXCLUSIVE = 2,
	RV_LAYER_SHELL_SEAT_FOCUS_NONE = 0,
};

// LayerShellSeat.Focus: union(enum) { exclusive: LayerSurface.Ref, non_exclusive: LayerSurface.Ref, none }
struct rv_layer_shell_seat_focus {
	enum rv_layer_shell_seat_focus_tag tag;
	union {
		struct rv_layer_surface_ref exclusive;
		struct rv_layer_surface_ref non_exclusive;
	};
};

struct rv_layer_shell_seat_scheduled {
	struct rv_layer_shell_seat_focus focus; // default: .none
};

struct rv_layer_shell_seat_sent {
	struct rv_layer_shell_seat_focus focus; // default: .none
};

struct rv_layer_shell_seat {
	struct wl_resource *object; // river_layer_shell_seat_v1, nullable

	struct rv_layer_shell_seat_scheduled scheduled;
	struct rv_layer_shell_seat_sent sent;
	// requested: struct {} = .{} in Zig (empty, never used): omitted.
};

void rv_layer_shell_seat_create_object(struct rv_layer_shell_seat *shell_seat, struct wl_client *client,
	uint32_t version, uint32_t id);
void rv_layer_shell_seat_make_inert(struct rv_layer_shell_seat *shell_seat);
void rv_layer_shell_seat_manage_start(struct rv_layer_shell_seat *shell_seat);

// ---------------------------------------------------------------------------
// LayerShell.zig (embedded by value in struct rv_server as layer_shell)
// ---------------------------------------------------------------------------

struct rv_layer_shell {
	struct wl_global *global;
	struct wlr_layer_shell_v1 *wlr_shell;

	// The layer shell object of the active window manager, if any
	struct wl_list objects; // river_layer_shell_v1 resources, linked with wl_resource_get_link()

	struct rv_slotmap surfaces; // SlotMap(*LayerSurface); default: .empty (rv_slotmap_empty)

	struct wl_listener new_surface; // default: .init(handleNewSurface)
};

// Zig !void: returns false if the global or wlr_layer_shell_v1 could not be created.
bool rv_layer_shell_init(struct rv_layer_shell *layer_shell);
// Use a deinit function rather than listening for the wl_server to be destroyed
// in order to avoid a signal ordering issue. The wlr.LayerShellV1 also listens
// for the wl_server to be destroyed and asserts that the new_surface event has
// no remaining listeners.
void rv_layer_shell_deinit(struct rv_layer_shell *layer_shell);
void rv_layer_shell_check_exclusive_focus(struct rv_layer_shell *layer_shell);

// ---------------------------------------------------------------------------
// Output.zig
// ---------------------------------------------------------------------------

// Output.State.state
enum rv_output_state_state {
	// Powered on and exposed to the window manager
	RV_OUTPUT_STATE_STATE_ENABLED,
	// Powered off and exposed to the window manager
	RV_OUTPUT_STATE_STATE_DISABLED_SOFT,
	// Powered off and hidden from the window manager
	RV_OUTPUT_STATE_STATE_DISABLED_HARD,
	// Corresponding hardware no longer present
	RV_OUTPUT_STATE_STATE_DESTROYING,
};

enum rv_output_state_mode_tag {
	RV_OUTPUT_STATE_MODE_STANDARD,
	RV_OUTPUT_STATE_MODE_CUSTOM,
	// Used before the initial modeset and after the wlr_output is destroyed.
	RV_OUTPUT_STATE_MODE_NONE,
};

struct rv_output_state_mode_custom {
	int32_t width;
	int32_t height;
	int32_t refresh;
};

// Output.State.mode: union(enum) { standard: *wlr.Output.Mode, custom: struct, none }
// The width/height of modes is in physical pixels, not in the
// compositors logical coordinate space.
struct rv_output_state_mode {
	enum rv_output_state_mode_tag tag;
	union {
		struct wlr_output_mode *standard;
		struct rv_output_state_mode_custom custom;
	};
};

// Output.State (no field defaults in Zig; always built explicitly)
struct rv_output_state {
	enum rv_output_state_state state;
	// Logical coordinate space
	int32_t x;
	// Logical coordinate space
	int32_t y;
	struct rv_output_state_mode mode;
	float scale;
	enum wl_output_transform transform;
	bool adaptive_sync;
	bool auto_layout;
};

// Output.RenderingState (file-private in Zig); its private init is all-zero (tearing = false).
struct rv_output_rendering_state {
	bool tearing;
};

// Output.lock_render_state. Tracks the currently presented frame on the output
// as it pertains to ext-session-lock. Declaration order is the Zig order; the
// values are chosen so that 0 is the Zig default (.blanked). Never rely on the numbers.
enum rv_output_lock_render_state {
	// Submitted an unlocked buffer but the buffer has not yet been presented.
	RV_OUTPUT_LOCK_RENDER_STATE_PENDING_UNLOCK = 1,
	// Normal, "unlocked" content may be visible.
	RV_OUTPUT_LOCK_RENDER_STATE_UNLOCKED = 2,
	// Submitted a blank buffer but the buffer has not yet been presented.
	// Normal, "unlocked" content may be visible.
	RV_OUTPUT_LOCK_RENDER_STATE_PENDING_BLANK = 3,
	// A blank buffer has been presented.
	RV_OUTPUT_LOCK_RENDER_STATE_BLANKED = 0,
	// Submitted the lock surface buffer but the buffer has not yet been presented.
	// Normal, "unlocked" content may be visible.
	RV_OUTPUT_LOCK_RENDER_STATE_PENDING_LOCK_SURFACE = 4,
	// The lock surface buffer has been presented.
	RV_OUTPUT_LOCK_RENDER_STATE_LOCK_SURFACE = 5,
};

struct rv_output {
	// Set to null when the wlr_output is destroyed.
	struct wlr_output *wlr_output; // nullable
	struct wlr_scene_output *scene_output; // nullable

	struct wl_resource *object; // river_output_v1, nullable
	struct rv_layer_shell_output layer_shell; // default: .{}

	// The output is initially considered blanked:
	// If using the DRM backend it will be blanked with the initial modeset.
	// If using the Wayland or X11 backend nothing will be visible until the first frame is rendered.
	enum rv_output_lock_render_state lock_render_state; // default: .blanked

	// Root.outputs (OutputManager.outputs)
	struct wl_list link;

	// State to be sent to the wm in the next manage sequence.
	struct rv_output_state scheduled;
	// State sent to the wm in the latest manage sequence.
	struct rv_output_state sent;
	struct wl_list link_sent; // WindowManager.sent.outputs
	bool sent_wl_output; // default: false
	// Rendering state requested by the window manager.
	struct rv_output_rendering_state rendering_requested; // default: .init
	// State applied to the wlr_output and rendered.
	struct rv_output_state current;
	struct rv_output_rendering_state rendering_current; // default: .init

	struct wl_listener destroy; // default: .init(handleDestroy)
	struct wl_listener request_state; // default: .init(handleRequestState)
	struct wl_listener frame; // default: .init(handleFrame)
	struct wl_listener present; // default: .init(handlePresent)
};

// Output.State.fromHeadState(); asserts state->enabled.
struct rv_output_state rv_output_state_from_head_state(const struct wlr_output_head_v1_state *state);
// Output.State.dimensions(): width/height in the logical coordinate space
// (Zig returns the tuple struct { u31, u31 }).
void rv_output_state_dimensions(const struct rv_output_state *state, int32_t *width, int32_t *height);
struct wlr_box rv_output_state_box(const struct rv_output_state *state);
void rv_output_state_apply_no_modeset(const struct rv_output_state *state, struct wlr_output_state *wlr_state);
void rv_output_state_apply_modeset(const struct rv_output_state *state, struct wlr_output_state *wlr_state);

// Zig !void (error.OutOfMemory, error.InitRenderFailed): returns false on error.
// *init_render_failed is set to true for InitRenderFailed and false for
// OutOfMemory (OutputManager logs a different message for each).
bool rv_output_create(struct wlr_output *wlr_output, bool *init_render_failed);
void rv_output_manage_start(struct rv_output *output);
void rv_output_make_inert(struct rv_output *output);

// ---------------------------------------------------------------------------
// OutputManager.zig (embedded by value in struct rv_server as om)
// ---------------------------------------------------------------------------

struct rv_output_manager {
	// The very first modeset is different in that if it fails we exit river.
	bool first_modeset; // default: true

	struct wl_listener new_output; // default: .init(handleNewOutput)

	struct wlr_output_layout *output_layout;

	struct wlr_presentation *presentation;
	struct wlr_xdg_output_manager_v1 *xdg_output_manager;

	struct wlr_output_manager_v1 *wlr_output_manager;
	struct wl_listener manager_apply; // default: .init(handleManagerApply)
	struct wl_listener manager_test; // default: .init(handleManagerTest)

	struct wlr_output_power_manager_v1 *power_manager;
	struct wl_listener power_manager_set_mode; // default: .init(handlePowerManagerSetMode)

	struct wlr_gamma_control_manager_v1 *gamma_control_manager;

	// All Outputs that have a corresponding wlr_output.
	struct wl_list outputs; // struct rv_output.link
};

// Zig !void: returns false if any wlroots object could not be created.
bool rv_output_manager_init(struct rv_output_manager *om);
void rv_output_manager_deinit(struct rv_output_manager *om);
// Returns null if there are no outputs in the output layout
struct wlr_output *rv_output_manager_output_at(struct rv_output_manager *om, double lx, double ly);
void rv_output_manager_auto_layout(struct rv_output_manager *om);
void rv_output_manager_commit_output_state(struct rv_output_manager *om);

// ---------------------------------------------------------------------------
// LockManager.zig (embedded by value in struct rv_server as lock_manager)
// ---------------------------------------------------------------------------

enum rv_lock_manager_state {
	// No lock request has been made and the session is unlocked.
	RV_LOCK_MANAGER_STATE_UNLOCKED,
	// A lock request has been made and river is waiting for all outputs to have
	// rendered a lock surface before sending the locked event.
	RV_LOCK_MANAGER_STATE_WAITING_FOR_LOCK_SURFACES,
	// A lock request has been made but waiting for a lock surface to be rendered
	// on all outputs timed out. Now river is waiting only for all outputs to at
	// least be blanked before sending the locked event.
	RV_LOCK_MANAGER_STATE_WAITING_FOR_BLANK,
	// All outputs are either blanked or have a lock surface rendered and the
	// locked event has been sent.
	RV_LOCK_MANAGER_STATE_LOCKED,
};

struct rv_lock_manager {
	struct wlr_session_lock_manager_v1 *wlr_manager;

	enum rv_lock_manager_state state; // default: .unlocked
	struct wlr_session_lock_v1 *lock; // nullable

	// Limit on how long the locked event will be delayed to wait for
	// lock surfaces to be created and rendered. If this times out, then
	// the locked event will be sent immediately after all outputs have
	// been blanked.
	struct wl_event_source *lock_surfaces_timer;

	struct wl_listener new_lock; // default: .init(handleLock)
	struct wl_listener unlock; // default: .init(handleUnlock)
	struct wl_listener destroy; // default: .init(handleDestroy)
	struct wl_listener new_surface; // default: .init(handleSurface)
};

// Zig !void: returns false if the timer or wlr manager could not be created.
bool rv_lock_manager_init(struct rv_lock_manager *manager);
void rv_lock_manager_deinit(struct rv_lock_manager *manager);
void rv_lock_manager_maybe_lock(struct rv_lock_manager *manager);
struct rv_lock_surface *rv_lock_manager_lock_surface_from_output(struct rv_lock_manager *manager,
	struct rv_output *output);

// ---------------------------------------------------------------------------
// LockSurface.zig
// ---------------------------------------------------------------------------

struct rv_lock_surface {
	struct wlr_session_lock_surface_v1 *wlr_lock_surface;
	struct wlr_session_lock_v1 *lock;
	struct wlr_scene_tree *tree;

	struct wl_event_source *idle_update_focus; // nullable

	struct wl_listener map; // default: .init(handleMap)
	struct wl_listener surface_destroy; // default: .init(handleDestroy)
};

// Zig error{OutOfMemory}!void: returns false on OOM.
bool rv_lock_surface_create(struct wlr_session_lock_surface_v1 *wlr_lock_surface, struct wlr_session_lock_v1 *lock);
struct rv_output *rv_lock_surface_get_output(struct rv_lock_surface *lock_surface);
void rv_lock_surface_configure(struct rv_lock_surface *lock_surface);

#endif
