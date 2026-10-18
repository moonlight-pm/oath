// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/main.zig, process.zig, Scene.zig, SceneNodeData.zig,
// WindowManager.zig, WmNode.zig, ShellSurface.zig and Decoration.zig for
// Oath, 2026-10-18.
//
// Owned by the core area. Included only from rivulet.h, after util.h, config.h
// and output.h. struct rv_window (window.h) embeds struct rv_wm_node and
// struct rv_scene_saveable_surfaces by value, so this header must come before
// window.h. struct rv_server embeds a manager from every area by value and
// therefore lives in server.h (also core-owned), included last.
//
// Logging, timestamps (util.zig) and the slotmap are in util.h.

#ifndef RV_CORE_H
#define RV_CORE_H

// ---------------------------------------------------------------------------
// main.zig (see util.h for rv_log) and wlroots_log_wrapper.c
// ---------------------------------------------------------------------------

// wlroots_log_wrapper.c (copied as is): installs a wlroots log callback that
// formats the message and passes it to rv_wlroots_log_callback().
void rv_init_wlroots_log(enum wlr_log_importance importance);
// main.zig export fn rv_wlroots_log_callback(): logs ptr[0..len] with
// RV_SCOPE_WLROOTS at err/info/debug; WLR_SILENT and WLR_LOG_IMPORTANCE_LAST
// are unreachable. Defined in main.c.
void rv_wlroots_log_callback(enum wlr_log_importance importance, const char *ptr, size_t len);

// ---------------------------------------------------------------------------
// process.zig
// ---------------------------------------------------------------------------

// Ignore SIGPIPE and raise the RLIMIT_NOFILE soft limit to min(4096, hard).
void rv_process_setup(void);
// In the forked child: setsid, clear the signal mask, restore SIGPIPE and the
// original RLIMIT_NOFILE.
void rv_process_cleanup_child(void);

// ---------------------------------------------------------------------------
// SceneNodeData.zig
// ---------------------------------------------------------------------------

enum rv_scene_node_data_data_tag {
	RV_SCENE_NODE_DATA_DATA_WINDOW,
	RV_SCENE_NODE_DATA_DATA_SHELL_SURFACE,
	RV_SCENE_NODE_DATA_DATA_LOCK_SURFACE,
	RV_SCENE_NODE_DATA_DATA_LAYER_SURFACE,
	// Only ever set when WLR_HAS_XWAYLAND (Zig: noreturn payload otherwise).
	RV_SCENE_NODE_DATA_DATA_OVERRIDE_REDIRECT,
};

// SceneNodeData.Data: union(enum)
struct rv_scene_node_data_data {
	enum rv_scene_node_data_data_tag tag;
	union {
		struct rv_window *window;
		struct rv_shell_surface *shell_surface;
		struct rv_lock_surface *lock_surface;
		struct rv_layer_surface *layer_surface;
		struct rv_xwayland_override_redirect *override_redirect;
	};
};

// Stored in wlr_scene_node.data; freed when the node is destroyed.
struct rv_scene_node_data {
	struct wlr_scene_node *node;
	struct rv_scene_node_data_data data;
	struct wl_listener destroy; // default: .init(handleDestroy)
};

// Zig error{OutOfMemory}!void: returns false on OOM.
bool rv_scene_node_data_attach(struct wlr_scene_node *node, struct rv_scene_node_data_data data);
// Walks up the parents of node until one has data; NULL if none.
struct rv_scene_node_data *rv_scene_node_data_from_node(struct wlr_scene_node *node);
// Uses the root surface's data (a struct wlr_scene_node *); NULL if unset.
struct rv_scene_node_data *rv_scene_node_data_from_surface(struct wlr_surface *surface);

// ---------------------------------------------------------------------------
// Scene.zig (embedded by value in struct rv_server as scene)
// ---------------------------------------------------------------------------

// Scene.layers (anonymous struct in Zig): all direct children of the normal_tree scene node
struct rv_scene_layers {
	// Background layer shell layer
	struct wlr_scene_tree *background;
	// Bottom layer shell layer
	struct wlr_scene_tree *bottom;
	// Windows and shell surfaces of the window manager
	struct wlr_scene_tree *wm;
	// Top layer shell layer
	struct wlr_scene_tree *top;
	// Fullscreen windows and river shell surfaces placed above them.
	struct wlr_scene_tree *fullscreen;
	// Overlay layer shell layer
	struct wlr_scene_tree *overlay;
	// Popups from xdg-shell and input-method-v2 clients
	struct wlr_scene_tree *popups;
#if WLR_HAS_XWAYLAND
	// Xwayland override redirect windows are a legacy wart that decide where
	// to place themselves in layout coordinates. Unfortunately this is how
	// X11 decided to make dropdown menus and the like possible.
	struct wlr_scene_tree *override_redirect;
#endif
};

struct rv_scene {
	struct wlr_scene *wlr_scene;
	// All windows, status bars, drowdown menus, etc. that can recieve pointer events and similar.
	struct wlr_scene_tree *interactive_tree;
	// Drag icons, which cannot recieve e.g. pointer events and are therefore kept
	// in a separate tree from the interactive tree.
	struct wlr_scene_tree *drag_icons;
	// Always disabled, used for staging changes
	// TODO can this be refactored away?
	struct wlr_scene_tree *hidden_tree;
	// Direct child of interactive_tree, disabled when the session is locked
	struct wlr_scene_tree *normal_tree;
	// Direct child of interactive_tree, enabled when the session is locked
	struct wlr_scene_tree *locked_tree;

	// All direct children of the normal_tree scene node
	struct rv_scene_layers layers;
};

// Scene.AtResult
struct rv_scene_at_result {
	struct wlr_scene_node *node;
	struct wlr_surface *surface; // nullable
	double sx;
	double sy;
	struct rv_scene_node_data_data data;
};

// Scene.SaveableSurfaces (embedded by value in ShellSurface, Decoration and Window)
struct rv_scene_saveable_surfaces {
	bool enabled;
	bool saved;
	struct wlr_scene_tree *tree;
	struct wlr_scene_tree *saved_tree;
};

// Zig !void: returns false on OOM.
bool rv_scene_init(struct rv_scene *scene);
// Return information about what is currently rendered in the interactive_tree
// tree at the given layout coordinates, taking surface input regions into account.
// Zig returns ?AtResult: returns false for null, else fills *result.
bool rv_scene_at(const struct rv_scene *scene, double lx, double ly, struct rv_scene_at_result *result);
// layer must be background, bottom, top or overlay (anything else is unreachable).
struct wlr_scene_tree *rv_scene_layer_surface_tree(struct rv_scene *scene, enum zwlr_layer_shell_v1_layer layer);

// Scene.SaveableSurfaces.init(parent): Zig returns !SaveableSurfaces.
// Returns false on OOM, else fills *surfaces ({ .enabled = true, .saved = false, ... })
// and syncs the enabled state of both trees.
bool rv_scene_saveable_surfaces_init(struct rv_scene_saveable_surfaces *surfaces, struct wlr_scene_tree *parent);
void rv_scene_saveable_surfaces_set_enabled(struct rv_scene_saveable_surfaces *surfaces, bool enabled);
void rv_scene_saveable_surfaces_save(struct rv_scene_saveable_surfaces *surfaces);
void rv_scene_saveable_surfaces_drop_saved(struct rv_scene_saveable_surfaces *surfaces);

// ---------------------------------------------------------------------------
// WmNode.zig (embedded by value in struct rv_window and struct rv_shell_surface
// as `node`; WmNode.get() recovers the parent with wl_container_of(node, x, node))
// ---------------------------------------------------------------------------

// WmNode.Tag (the tag type of WmNode.Type)
enum rv_wm_node_tag {
	RV_WM_NODE_TAG_WINDOW,
	RV_WM_NODE_TAG_SHELL_SURFACE,
};

// WmNode.Type: union(enum) { window: *Window, shell_surface: *ShellSurface }
struct rv_wm_node_type {
	enum rv_wm_node_tag tag;
	union {
		struct rv_window *window;
		struct rv_shell_surface *shell_surface;
	};
};

struct rv_wm_node {
	enum rv_wm_node_tag tag;
	struct wl_resource *object; // river_node_v1, nullable

	// WindowManager.rendering_requested.list
	struct wl_list link;
};

void rv_wm_node_init(struct rv_wm_node *node, enum rv_wm_node_tag tag);
void rv_wm_node_deinit(struct rv_wm_node *node);
struct rv_wm_node_type rv_wm_node_get(struct rv_wm_node *node);
void rv_wm_node_create_object(struct rv_wm_node *node, struct wl_client *client, uint32_t version, uint32_t id);
void rv_wm_node_make_inert(struct rv_wm_node *node);

// ---------------------------------------------------------------------------
// WindowManager.zig (embedded by value in struct rv_server as wm)
// ---------------------------------------------------------------------------

enum rv_window_manager_state_tag {
	RV_WINDOW_MANAGER_STATE_IDLE,
	// Waiting on the window manager client to send manage_finish.
	RV_WINDOW_MANAGER_STATE_MANAGE,
	// The number of configures sent that have not yet been acked
	RV_WINDOW_MANAGER_STATE_INFLIGHT_CONFIGURES,
	// Waiting on the window manager client to send render_finish.
	RV_WINDOW_MANAGER_STATE_RENDER,
};

// WindowManager.state: union(enum) { idle, manage, inflight_configures: u32, render }
struct rv_window_manager_state {
	enum rv_window_manager_state_tag tag;
	union {
		uint32_t inflight_configures;
	};
};

// State to be sent to the wm in the next manage sequence.
struct rv_window_manager_scheduled {
	// State has been modified since the last manage sequence.
	// Prevents processing further input events until a manage sequence is completed.
	bool dirty; // default: false
	// A manage sequence should be started when idle, but don't prevent processing
	// further input events.
	bool dirty_lazy; // default: false

	struct wlr_output_configuration_v1 *output_config; // nullable
};

// State sent to the wm in the latest update sequence.
struct rv_window_manager_sent {
	bool session_locked; // default: false

	struct wl_list outputs; // struct rv_output.link_sent
	struct wlr_output_configuration_v1 *output_config; // nullable

	struct wl_list seats; // struct rv_seat.link_sent
};

// Rendering state to be sent to the wm in the next render sequence.
struct rv_window_manager_rendering_scheduled {
	// Rendering state has been modified since the last render sequence.
	bool dirty; // default: false
};

// The list is in rendering order, the last node in the list is rendered on top.
struct rv_window_manager_rendering_requested {
	struct wl_list list; // struct rv_wm_node.link
	uint64_t order_hash; // default: 0
};

struct rv_window_manager {
	struct wl_global *global;
	struct wl_listener server_destroy; // default: .init(handleServerDestroy)

	// The protocol object of the active window manager, if any.
	struct wl_resource *object; // river_window_manager_v1, nullable

	struct rv_window_manager_state state; // default: .idle

	struct rv_slotmap windows; // SlotMap(*Window); default: .empty (rv_slotmap_empty)

	// State to be sent to the wm in the next manage sequence.
	struct rv_window_manager_scheduled scheduled;

	// State sent to the wm in the latest update sequence.
	struct rv_window_manager_sent sent;

	// Rendering state to be sent to the wm in the next render sequence.
	struct rv_window_manager_rendering_scheduled rendering_scheduled;

	// The list is in rendering order, the last node in the list is rendered on top.
	struct rv_window_manager_rendering_requested rendering_requested;

	struct wl_event_source *dirty_idle; // nullable

	struct wl_event_source *timeout;
};

// Zig !void: returns false if the timer or global could not be created.
bool rv_window_manager_init(struct rv_window_manager *wm);
bool rv_window_manager_ensure_windowing(struct rv_window_manager *wm);
bool rv_window_manager_ensure_rendering(struct rv_window_manager *wm);
void rv_window_manager_dirty_windowing(struct rv_window_manager *wm);
void rv_window_manager_dirty_windowing_lazy(struct rv_window_manager *wm);
void rv_window_manager_clean_windowing(struct rv_window_manager *wm);
void rv_window_manager_dirty_rendering(struct rv_window_manager *wm);
void rv_window_manager_clean_rendering(struct rv_window_manager *wm);
void rv_window_manager_manage_finish(struct rv_window_manager *wm);
void rv_window_manager_notify_configured(struct rv_window_manager *wm);

// ---------------------------------------------------------------------------
// ShellSurface.zig
// ---------------------------------------------------------------------------

struct rv_shell_surface_rendering_requested {
	int32_t x; // default: 0
	int32_t y; // default: 0
	bool sync_next_commit; // default: false
};

struct rv_shell_surface {
	struct wl_resource *object; // river_shell_surface_v1
	struct wlr_surface *surface;
	struct wlr_scene_tree *tree;
	struct rv_scene_saveable_surfaces surfaces;
	struct wlr_scene_tree *popup_tree;
	struct rv_wm_node node;

	struct rv_shell_surface_rendering_requested rendering_requested;
};

// Zig !void (OutOfMemory, ResourceCreateFailed): returns false on either. Returns
// true without creating anything if the surface already has another role.
bool rv_shell_surface_create(struct wl_client *client, uint32_t version, uint32_t id, struct wlr_surface *surface);
void rv_shell_surface_render_finish(struct rv_shell_surface *shell_surface);

// ---------------------------------------------------------------------------
// Decoration.zig
// ---------------------------------------------------------------------------

struct rv_decoration_rendering_requested {
	int32_t offset_x; // default: 0
	int32_t offset_y; // default: 0
	bool sync_next_commit; // default: false
};

struct rv_decoration {
	struct wl_resource *object; // river_decoration_v1, nullable
	struct wlr_surface *surface;
	struct wlr_scene_tree *tree;
	struct rv_scene_saveable_surfaces surfaces;
	// Window.decorations_above/below
	struct wl_list link;

	struct rv_decoration_rendering_requested rendering_requested;
};

// Zig !*Decoration (OutOfMemory, ResourceCreateFailed, AlreadyHasRole): returns
// NULL on error and sets *already_has_role to true for AlreadyHasRole (the
// caller then returns silently) and false otherwise (the caller posts no_memory).
struct rv_decoration *rv_decoration_create(struct wl_client *client, uint32_t version, uint32_t id,
	struct wlr_surface *surface, struct wlr_scene_tree *parent, bool *already_has_role);
void rv_decoration_destroy(struct rv_decoration *decoration);
void rv_decoration_make_inert(struct rv_decoration *decoration);
void rv_decoration_render_finish(struct rv_decoration *decoration, const struct wlr_box *window_clip);

#endif
