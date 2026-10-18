// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Window.zig, XdgToplevel.zig, XdgPopup.zig,
// XdgDecoration.zig, XwaylandWindow.zig and XwaylandOverrideRedirect.zig for
// Oath, 2026-10-18.
//
// Owned by the window area. Included only from river.h, after core.h (struct
// rv_window embeds struct rv_wm_node and struct rv_scene_saveable_surfaces)
// and before seat.h (struct rv_seat embeds struct rv_window_ref).
//
// Window.impl holds struct rv_xdg_toplevel / struct rv_xwayland_window by value,
// so those are defined before struct rv_window below.

#ifndef RV_WINDOW_H
#define RV_WINDOW_H

// ---------------------------------------------------------------------------
// Window.zig: small value types
// ---------------------------------------------------------------------------

// Window.Dimensions
struct rv_window_dimensions {
	int32_t width; // u31
	int32_t height; // u31
};

// Window.DimensionsHint (all fields default to 0; compare field by field for meta.eql)
struct rv_window_dimensions_hint {
	int32_t min_width; // u31
	int32_t max_width; // u31
	int32_t min_height; // u31
	int32_t max_height; // u31
};

// Window.Ref: a key into server.wm.windows. Packed in Zig to make == work;
// compare with rv_window_ref_eql(). A ?Window.Ref is `bool has_x; struct rv_window_ref x;`
// and two optionals are equal if both are null or both are set with equal keys.
struct rv_window_ref {
	struct rv_slotmap_key key;
};

static inline bool rv_window_ref_eql(struct rv_window_ref a, struct rv_window_ref b) {
	return rv_slotmap_key_eql(a.key, b.key);
}

// Window.Ref.get(): server.wm.windows.get(ref.key); NULL if the window is gone.
struct rv_window *rv_window_ref_get(struct rv_window_ref ref);

// ---------------------------------------------------------------------------
// XdgDecoration.zig (lives by value in XdgToplevel.decoration)
// ---------------------------------------------------------------------------

struct rv_xdg_decoration {
	struct wlr_xdg_toplevel_decoration_v1 *wlr_decoration;

	struct wl_listener destroy; // default: .init(handleDestroy)
	struct wl_listener request_mode; // default: .init(handleRequestMode)
};

// Stores the decoration in the toplevel (wlr_decoration->toplevel->base->data)
// and links its listeners there.
void rv_xdg_decoration_init(struct wlr_xdg_toplevel_decoration_v1 *wlr_decoration);
// Unlinks the listeners and sets the toplevel's decoration to null.
void rv_xdg_decoration_deinit(struct rv_xdg_decoration *decoration);

// ---------------------------------------------------------------------------
// XdgToplevel.zig (lives by value in Window.impl.toplevel)
// ---------------------------------------------------------------------------

enum rv_xdg_toplevel_configure_state_tag {
	// No configure has been sent since the last configure was acked.
	RV_XDG_TOPLEVEL_CONFIGURE_STATE_IDLE,
	// A configure was sent with the given serial but has not yet been acked.
	RV_XDG_TOPLEVEL_CONFIGURE_STATE_INFLIGHT,
	// A configure was acked but the surface has not yet been committed.
	RV_XDG_TOPLEVEL_CONFIGURE_STATE_ACKED,
	// A configure was acked and the surface was committed.
	RV_XDG_TOPLEVEL_CONFIGURE_STATE_COMMITTED,
	// A configure was sent but not acked before the transaction timed out.
	RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT,
	// A configure was sent and acked but not committed before the transaction timed out.
	RV_XDG_TOPLEVEL_CONFIGURE_STATE_TIMED_OUT_ACKED,
};

// XdgToplevel.configure_state: union(enum) { idle, inflight: u32, acked, committed,
// timed_out: u32, timed_out_acked }
struct rv_xdg_toplevel_configure_state {
	enum rv_xdg_toplevel_configure_state_tag tag;
	union {
		uint32_t inflight; // serial
		uint32_t timed_out; // serial
	};
};

struct rv_xdg_toplevel {
	// TODO(zig): get rid of this and use @fieldParentPtr(), https://github.com/ziglang/zig/issues/6611
	struct rv_window *window;

	struct wlr_xdg_toplevel *wlr_toplevel;

	// ?XdgDecoration, default null. wlr_toplevel->base->data points at this
	// struct rv_xdg_toplevel, so &toplevel->decoration is stable.
	bool has_decoration;
	struct rv_xdg_decoration decoration;

	struct wlr_box geometry; // default: { 0, 0, 0, 0 }

	struct rv_xdg_toplevel_configure_state configure_state; // default: .idle

	// Listeners that are always active over the window's lifetime
	struct wl_listener destroy; // default: .init(handleDestroy)
	struct wl_listener ack_configure; // default: .init(handleAckConfigure)
	struct wl_listener map; // default: .init(handleMap)
	struct wl_listener unmap; // default: .init(handleUnmap)
	struct wl_listener commit; // default: .init(handleCommit)
	struct wl_listener new_popup; // default: .init(handleNewPopup)
	struct wl_listener request_show_window_menu; // default: .init(handleRequestShowWindowMenu)
	struct wl_listener request_fullscreen; // default: .init(handleRequestFullscreen)
	struct wl_listener request_maximize; // default: .init(handleRequestMaximize)
	struct wl_listener request_minimize; // default: .init(handleRequestMinimize)
	struct wl_listener request_move; // default: .init(handleRequestMove)
	struct wl_listener request_resize; // default: .init(handleRequestResize)
	struct wl_listener set_parent; // default: .init(handleSetParent)
	struct wl_listener set_title; // default: .init(handleSetTitle)
	struct wl_listener set_app_id; // default: .init(handleSetAppId)
};

// Zig error{OutOfMemory}!void: returns false on OOM.
bool rv_xdg_toplevel_create(struct wlr_xdg_toplevel *wlr_toplevel);
// Send a configure event, return true if the configure should be tracked
// and current surfaces saved for frame perfection.
bool rv_xdg_toplevel_configure(struct rv_xdg_toplevel *toplevel);
void rv_xdg_toplevel_destroy_popups(struct rv_xdg_toplevel *toplevel);

// ---------------------------------------------------------------------------
// XwaylandWindow.zig (lives by value in Window.impl.xwayland)
// ---------------------------------------------------------------------------

#if WLR_HAS_XWAYLAND
struct rv_xwayland_window {
	// TODO(zig): get rid of this and use @fieldParentPtr(), https://github.com/ziglang/zig/issues/6611
	struct rv_window *window;

	struct wlr_xwayland_surface *xsurface;
	// Created on map and destroyed on unmap
	struct wlr_scene_tree *surface_tree; // nullable

	// Active over entire lifetime
	struct wl_listener destroy; // default: .init(handleDestroy)
	struct wl_listener request_configure; // default: .init(handleRequestConfigure)
	struct wl_listener set_override_redirect; // default: .init(handleSetOverrideRedirect)
	struct wl_listener associate; // default: .init(handleAssociate)
	struct wl_listener dissociate; // default: .init(handleDissociate)
	struct wl_listener set_size_hints; // default: .init(handleSetSizeHints)
	struct wl_listener set_title; // default: .init(handleSetTitle)
	struct wl_listener set_class; // default: .init(handleSetClass)
	struct wl_listener set_parent; // default: .init(handleSetParent)
	struct wl_listener set_decorations; // default: .init(handleSetDecorations)
	struct wl_listener request_maximize; // default: .init(handleRequestMaximize)
	struct wl_listener request_fullscreen; // default: .init(handleRequestFullscreen)
	struct wl_listener request_minimize; // default: .init(handleRequestMinimize)

	// Active while the xsurface is associated with a wlr_surface
	struct wl_listener map; // default: .init(handleMap)
	struct wl_listener unmap; // default: .init(handleUnmap)
};

// Zig error{OutOfMemory}!void: returns false on OOM.
bool rv_xwayland_window_create(struct wlr_xwayland_surface *xsurface);
// Always returns false as we do not care about frame perfection for Xwayland windows.
bool rv_xwayland_window_configure(struct rv_xwayland_window *xwindow);
// pub fn handleMap(listener: *wl.Listener(void)): the `map` listener's notify;
// also called directly as rv_xwayland_window_handle_map(&xwindow->map, NULL).
void rv_xwayland_window_handle_map(struct wl_listener *listener, void *data);
#endif

// ---------------------------------------------------------------------------
// Window.zig
// ---------------------------------------------------------------------------

enum rv_window_impl_tag {
	RV_WINDOW_IMPL_TOPLEVEL,
	// Only ever set when WLR_HAS_XWAYLAND (Zig: noreturn payload otherwise).
	RV_WINDOW_IMPL_XWAYLAND,
	// This state is assigned during destruction after the xdg toplevel
	// has been destroyed but while the transaction system is still rendering
	// saved surfaces of the window.
	RV_WINDOW_IMPL_DESTROYING,
};

// Window.Impl: union(enum) { toplevel: XdgToplevel, xwayland: XwaylandWindow, destroying }.
// Assigning Zig `.destroying` only sets the tag.
struct rv_window_impl {
	enum rv_window_impl_tag tag;
	union {
		struct rv_xdg_toplevel toplevel;
#if WLR_HAS_XWAYLAND
		struct rv_xwayland_window xwayland;
#endif
	};
};

enum rv_window_fullscreen_request_tag {
	RV_WINDOW_FULLSCREEN_REQUEST_NO_REQUEST,
	RV_WINDOW_FULLSCREEN_REQUEST_FULLSCREEN,
	RV_WINDOW_FULLSCREEN_REQUEST_EXIT,
};

// Window.FullscreenRequest: union(enum) { no_request, fullscreen: ?*Output, exit }
struct rv_window_fullscreen_request {
	enum rv_window_fullscreen_request_tag tag;
	union {
		struct rv_output *fullscreen; // nullable (output hint)
	};
};

// Window.Border (all fields default to 0). Field order is Zig's: r, b, g, a.
struct rv_window_border {
	uint32_t edges; // river.WindowV1.Edges: RIVER_WINDOW_V1_EDGES_* bits; default: .{} (0)
	int32_t width; // u31
	uint32_t r;
	uint32_t b;
	uint32_t g;
	uint32_t a;
};

// Window.WmRequested: windowing state requested by the wm.
struct rv_window_wm_requested {
	// ?Dimensions
	bool has_dimensions;
	struct rv_window_dimensions dimensions;
	struct rv_window_dimensions bounds;
	bool ssd;
	uint32_t tiled; // river.WindowV1.Edges: RIVER_WINDOW_V1_EDGES_* bits
	uint32_t capabilities; // river.WindowV1.Capabilities: RIVER_WINDOW_V1_CAPABILITIES_* bits
	bool resizing;
	bool maximized;
	struct rv_output *fullscreen; // nullable
	bool inform_fullscreen;
	bool close;
};

// Window.WmRequested.init: { .dimensions = null, .bounds = 0x0, .ssd = false, .tiled = 0,
// .capabilities = window_menu | maximize | fullscreen | minimize, everything else false/null }.
// NOT all-zero (capabilities). Defined in window.c.
extern const struct rv_window_wm_requested rv_window_wm_requested_init;

// Window.Configure
struct rv_window_configure {
	// ?u31
	bool has_width;
	int32_t width;
	// ?u31
	bool has_height;
	int32_t height;
	struct rv_window_dimensions bounds;
	// True if the window has keyboard focus from at least one seat.
	bool activated;
	bool ssd;
	uint32_t tiled; // river.WindowV1.Edges: RIVER_WINDOW_V1_EDGES_* bits
	uint32_t capabilities; // river.WindowV1.Capabilities: RIVER_WINDOW_V1_CAPABILITIES_* bits
	bool maximized;
	bool inform_fullscreen;
	bool resizing;
};

// Window.Configure.init: width/height null, everything else 0/false (all-zero). Defined in window.c.
extern const struct rv_window_configure rv_window_configure_init;

// Window.RenderingRequested: rendering state requested by the wm.
struct rv_window_rendering_requested {
	int32_t x;
	int32_t y;
	bool hidden;
	struct rv_window_border border;
	struct wlr_box clip;
	struct wlr_box content_clip;
};

// Window.RenderingRequested.init: all-zero. Defined in window.c.
extern const struct rv_window_rendering_requested rv_window_rendering_requested_init;

enum rv_window_state {
	// Initial state, also returned to after closed event is sent.
	RV_WINDOW_STATE_INIT,
	// The window is ready to be configured.
	// The river_window_v1 will be created in the next manage sequence.
	RV_WINDOW_STATE_READY,
	// The first configure has been sent but the window is not yet mapped.
	RV_WINDOW_STATE_INITIALIZED,
	// The window is mapped.
	RV_WINDOW_STATE_MAPPED,
	// The closed event will be sent in the next manage sequence.
	RV_WINDOW_STATE_CLOSING,
};

// Window.wm_scheduled.show_window_menu_requested payload
struct rv_window_wm_scheduled_show_window_menu_requested {
	int32_t x;
	int32_t y;
};

enum rv_window_wm_scheduled_maximize_requested {
	RV_WINDOW_WM_SCHEDULED_MAXIMIZE_REQUESTED_NO_REQUEST,
	RV_WINDOW_WM_SCHEDULED_MAXIMIZE_REQUESTED_MAXIMIZE,
	RV_WINDOW_WM_SCHEDULED_MAXIMIZE_REQUESTED_UNMAXIMIZE,
};

// Window.wm_scheduled.pointer_resize_requested payload
struct rv_window_wm_scheduled_pointer_resize_requested {
	struct rv_seat *seat;
	uint32_t edges; // river.WindowV1.Edges: RIVER_WINDOW_V1_EDGES_* bits
};

// State to be sent to the wm in the next manage sequence. All-zero is the Zig default.
struct rv_window_wm_scheduled {
	struct rv_window_dimensions_hint dimensions_hint; // default: .{}
	enum river_window_v1_decoration_hint decoration_hint; // default: .only_supports_csd (0)
	// ?struct { x: i32, y: i32 }, default null
	bool has_show_window_menu_requested;
	struct rv_window_wm_scheduled_show_window_menu_requested show_window_menu_requested;
	// Set back to no_request at the end of each update sequence
	struct rv_window_fullscreen_request fullscreen_requested; // default: .no_request
	enum rv_window_wm_scheduled_maximize_requested maximize_requested; // default: .no_request
	bool minimize_requested;
	bool dirty_app_id;
	bool dirty_title;
	struct rv_seat *pointer_move_requested; // nullable
	// ?struct { seat: *Seat, edges: river.WindowV1.Edges }, default null
	bool has_pointer_resize_requested;
	struct rv_window_wm_scheduled_pointer_resize_requested pointer_resize_requested;
};

// State sent to the wm in the latest manage sequence.
// This state is only kept around in order to avoid sending redundant events
// to the wm. All-zero is the Zig default.
struct rv_window_wm_sent {
	struct rv_window_dimensions_hint dimensions_hint; // default: .{}
	enum river_window_v1_decoration_hint decoration_hint; // default: .only_supports_csd (0)
	// ?Window.Ref, default null
	bool has_parent;
	struct rv_window_ref parent;
};

// State to be sent to the wm in the next render sequence. All-zero is the Zig default.
struct rv_window_rendering_scheduled {
	// Dimensions committed by the window.
	int32_t width; // u31
	int32_t height; // u31
	// Send dimensions even if they are unchanged.
	bool resend_dimensions;
};

// State sent to the wm in the latest render sequence. All-zero is the Zig default.
struct rv_window_rendering_sent {
	int32_t width; // u31
	int32_t height; // u31
	enum river_output_v1_presentation_mode presentation_hint; // default: .vsync (0)
};

struct rv_window {
	struct rv_window_ref ref;

	// The window management protocol object for this window
	// Created in manageStart() when state is .ready
	// Set to null in manageStart() when state is .closing
	struct wl_resource *object; // river_window_v1, nullable
	struct rv_wm_node node;

	enum rv_window_state state; // default: .init

	// The implementation of this window
	struct rv_window_impl impl;

	// This is the root scene tree for the window.
	// The trees in the following fields are in rendering order.
	struct wlr_scene_tree *tree;

	// Opaque black rectangle used as the background while this window is rendered fullscreen.
	// TODO consider using one of these per output rather than one per window to save memory
	// if the complexity tradeoff is worth it.
	struct wlr_scene_rect *fullscreen_background;

	struct wl_list decorations_below; // struct rv_decoration.link
	struct wlr_scene_tree *decorations_below_tree;

	struct rv_scene_saveable_surfaces surfaces;

	// Anonymous in Zig too; a named type would clash with struct rv_window_border (Window.Border).
	struct {
		struct wlr_scene_rect *left;
		struct wlr_scene_rect *right;
		struct wlr_scene_rect *top;
		struct wlr_scene_rect *bottom;
	} border;

	struct wl_list decorations_above; // struct rv_decoration.link
	struct wlr_scene_tree *decorations_above_tree;

	struct wlr_scene_tree *popup_tree;

	struct wlr_scene *capture_scene;
	struct wlr_ext_image_capture_source_v1 *capture_source; // nullable

	// State to be sent to the wm in the next manage sequence.
	struct rv_window_wm_scheduled wm_scheduled; // default: .{}

	// State sent to the wm in the latest manage sequence.
	// This state is only kept around in order to avoid sending redundant events
	// to the wm.
	struct rv_window_wm_sent wm_sent; // default: .{}

	// Windowing state requested by the wm.
	struct rv_window_wm_requested wm_requested; // default: .init (rv_window_wm_requested_init)

	// State to be sent to the window in the next configure.
	struct rv_window_configure configure_scheduled; // default: .init
	// State sent to the window in the latest configure.
	struct rv_window_configure configure_sent; // default: .init

	// State to be sent to the wm in the next render sequence.
	struct rv_window_rendering_scheduled rendering_scheduled; // default: .{}

	// State sent to the wm in the latest render sequence.
	struct rv_window_rendering_sent rendering_sent; // default: .{}

	// Rendering state requested by the wm.
	struct rv_window_rendering_requested rendering_requested; // default: .init

	// The currently rendered position/dimensions of the window in the scene graph
	struct wlr_box box; // default: { 0, 0, 0, 0 }

	struct wlr_ext_foreign_toplevel_handle_v1 *foreign_toplevel_handle; // nullable
	struct wlr_foreign_toplevel_handle_v1 *wlr_toplevel_handle; // nullable
};

// Zig error{OutOfMemory}!*Window: returns NULL on OOM. impl is copied into the
// window (its tag must not be RV_WINDOW_IMPL_DESTROYING); the caller sets the
// impl's listener notify fields in the value it passes (the Zig .init defaults),
// then fixes up impl.toplevel.window / impl.xwayland.window and links the
// listeners at &window->impl.* (never at the caller's copy).
struct rv_window *rv_window_create(struct rv_window_impl impl);
// It's safe to destroy the window after we no longer need the saved buffers
// for frame perfection. We no longer need the saved buffers after the manage
// sequence in which the closed event was sent is completed and the following
// render sequence is completed as well.
void rv_window_destroy(struct rv_window *window);
void rv_window_set_dimensions_hint(struct rv_window *window, struct rv_window_dimensions_hint hint);
void rv_window_set_dimensions(struct rv_window *window, int32_t width, int32_t height);
void rv_window_set_decoration_hint(struct rv_window *window, enum river_window_v1_decoration_hint hint);
// Send dirty state as part of a manage sequence.
void rv_window_manage_start(struct rv_window *window);
void rv_window_make_inert(struct rv_window *window);
// Applies window management state from the window manager and sends a configure
// to the window if necessary.
// Returns true if the configure should be waited for by the transaction system.
bool rv_window_manage_finish(struct rv_window *window);
void rv_window_render_start(struct rv_window *window);
void rv_window_render_finish(struct rv_window *window);
// Returns null if the window is currently being destroyed and no longer has
// an associated surface.
// May also return null for Xwayland windows that are not currently mapped.
struct wlr_surface *rv_window_root_surface(struct rv_window *window);
void rv_window_send_frame_done(struct rv_window *window);
void rv_window_close(struct rv_window *window);
void rv_window_destroy_popups(struct rv_window *window);
struct rv_window *rv_window_get_parent(struct rv_window *window);
int32_t rv_window_unreliable_pid(struct rv_window *window);
// Return the current title of the window if any (NULL for Zig null).
const char *rv_window_get_title(struct rv_window *window);
// Return the current app_id of the window if any (NULL for Zig null).
const char *rv_window_get_app_id(struct rv_window *window);
// Called by the impl when the surface is ready to be displayed.
// Zig !void, but the body cannot fail: always returns true. Callers keep their
// Zig catch path (log "out of memory", post no_memory) for a false return.
bool rv_window_map(struct rv_window *window);
// Called by the impl when the surface will no longer be displayed
void rv_window_unmap(struct rv_window *window);
void rv_window_notify_title(struct rv_window *window);
void rv_window_notify_app_id(struct rv_window *window);

// ---------------------------------------------------------------------------
// XdgPopup.zig
// ---------------------------------------------------------------------------

struct rv_xdg_popup {
	struct wlr_xdg_popup *wlr_popup;
	// The root of the surface tree, i.e. the Window's popup_tree
	struct wlr_scene_tree *root;

	struct wlr_scene_tree *tree;
	struct wlr_scene_tree *capture_tree; // nullable

	struct wl_listener destroy; // default: .init(handleDestroy)
	struct wl_listener commit; // default: .init(handleCommit)
	struct wl_listener new_popup; // default: .init(handleNewPopup)
	struct wl_listener reposition; // default: .init(handleReposition)
};

// TODO check if popup is set_reactive and reposition on parent movement.
// capture_parent may be NULL. Zig error{OutOfMemory}!void: returns false on OOM.
bool rv_xdg_popup_create(struct wlr_xdg_popup *wlr_popup, struct wlr_scene_tree *root,
	struct wlr_scene_tree *parent, struct wlr_scene_tree *capture_parent);

// ---------------------------------------------------------------------------
// XwaylandOverrideRedirect.zig
// ---------------------------------------------------------------------------

#if WLR_HAS_XWAYLAND
struct rv_xwayland_override_redirect {
	struct wlr_xwayland_surface *xsurface;
	struct wlr_scene_tree *surface_tree; // nullable

	// Active over entire lifetime
	struct wl_listener request_configure; // default: .init(handleRequestConfigure)
	struct wl_listener destroy; // default: .init(handleDestroy)
	struct wl_listener set_override_redirect; // default: .init(handleSetOverrideRedirect)
	struct wl_listener associate; // default: .init(handleAssociate)
	struct wl_listener dissociate; // default: .init(handleDissociate)

	// Active while the xsurface is associated with a wlr_surface
	struct wl_listener map; // default: .init(handleMap)
	struct wl_listener unmap; // default: .init(handleUnmap)

	// Active while mapped
	struct wl_listener set_geometry; // default: .init(handleSetGeometry)
};

// Zig error{OutOfMemory}!void: returns false on OOM.
bool rv_xwayland_override_redirect_create(struct wlr_xwayland_surface *xsurface);
// pub fn handleMap(listener: *wl.Listener(void)): the `map` listener's notify;
// also called directly as rv_xwayland_override_redirect_handle_map(&override_redirect->map, NULL).
void rv_xwayland_override_redirect_handle_map(struct wl_listener *listener, void *data);
void rv_xwayland_override_redirect_focus_if_desired(struct rv_xwayland_override_redirect *override_redirect);
#endif

#endif
