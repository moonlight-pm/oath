// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Server.zig for Oath, 2026-10-18.
//
// Owned by the core area (split out of core.h). struct rv_server embeds a
// manager from every area by value (Scene, InputManager, LibinputConfig,
// XkbConfig, OutputManager, IdleInhibitManager, LockManager, WindowManager,
// XkbBindings, LayerShell), so rivulet.h includes this header last.
// The single instance is `server` (main.zig `pub var server`), declared in
// rivulet.h and defined in main.c.

#ifndef RV_SERVER_H
#define RV_SERVER_H

struct rv_server {
	struct wl_display *wl_server;

	struct wl_event_source *sigint_source;
	struct wl_event_source *sigterm_source;

	struct wlr_fixes *fixes;

	struct wlr_backend *backend;
	struct wlr_session *session; // nullable

	struct wlr_renderer *renderer;
	struct wlr_allocator *allocator;
	struct wl_event_source *gpu_reset_recover; // nullable; default: null

	struct wlr_security_context_manager_v1 *security_context_manager;

	struct wlr_shm *shm;
	struct wlr_linux_dmabuf_v1 *linux_dmabuf; // nullable; default: null
	struct wlr_linux_drm_syncobj_manager_v1 *linux_drm_syncobj_manager; // nullable; default: null
	struct wlr_single_pixel_buffer_manager_v1 *single_pixel_buffer_manager;
	struct wlr_alpha_modifier_v1 *alpha_modifier;

	struct wlr_color_manager_v1 *color_manager; // nullable; default: null
	struct wlr_color_representation_manager_v1 *color_representation_manager;

	struct wlr_viewporter *viewporter;
	struct wlr_fractional_scale_manager_v1 *fractional_scale_manager;
	struct wlr_compositor *compositor;
	struct wlr_subcompositor *subcompositor;
	struct wlr_cursor_shape_manager_v1 *cursor_shape_manager;

	struct wlr_xdg_shell *xdg_shell;
	struct wlr_xdg_decoration_manager_v1 *xdg_decoration_manager;
	struct wlr_xdg_activation_v1 *xdg_activation;
	struct wlr_xdg_foreign_registry *xdg_foreign_registry;
	struct wlr_xdg_foreign_v2 *xdg_foreign_v2;

	struct wlr_data_device_manager *data_device_manager;
	struct wlr_primary_selection_v1_device_manager *primary_selection_manager;
	struct wlr_ext_data_control_manager_v1 *data_control_manager;
	struct wlr_data_control_manager_v1 *wlr_data_control_manager;

	struct wlr_export_dmabuf_manager_v1 *export_dmabuf_manager;
	struct wlr_screencopy_manager_v1 *screencopy_manager;

	struct wlr_ext_image_copy_capture_manager_v1 *image_copy_capture_manager;
	struct wlr_ext_output_image_capture_source_manager_v1 *output_image_capture_source_manager;

	struct wlr_foreign_toplevel_manager_v1 *wlr_foreign_toplevel_manager;
	struct wlr_ext_foreign_toplevel_list_v1 *foreign_toplevel_list;
	struct wlr_ext_foreign_toplevel_image_capture_source_manager_v1 *toplevel_capture_source_manager;

	struct wlr_tearing_control_manager_v1 *tearing_control_manager;

	struct rv_scene scene;
	struct rv_input_manager input_manager;
	struct rv_libinput_config libinput_config;
	struct rv_xkb_config xkb_config;
	struct rv_output_manager om;
	struct rv_idle_inhibit_manager idle_inhibit_manager;
	struct rv_lock_manager lock_manager;
	struct rv_window_manager wm;
	struct rv_xkb_bindings xkb_bindings;
	struct rv_layer_shell layer_shell;

#if WLR_HAS_XWAYLAND
	struct wlr_xwayland *xwayland; // nullable; default: null (stays null with -no-xwayland)
	struct wl_listener new_xsurface; // default: .init(handleNewXwaylandSurface)
#endif

	struct wl_listener renderer_lost; // default: .init(handleRendererLost)
	struct wl_listener new_xdg_toplevel; // default: .init(handleNewXdgToplevel)
	struct wl_listener new_toplevel_decoration; // default: .init(handleNewToplevelDecoration)
	struct wl_listener request_activate; // default: .init(handleRequestActivate)
	struct wl_listener request_set_cursor_shape; // default: .init(handleRequestSetCursorShape)
	struct wl_listener toplevel_capture_request; // default: .init(handleToplevelCaptureRequest)
};

// Zig !void: returns false on any initialisation failure (river then exits;
// memory is intentionally not freed on that path) and sets *error_name to the
// name of the Zig error, which main logs as "error: <name>" (Zig's start code
// does that when main returns an error).
bool rv_server_init(struct rv_server *server, bool runtime_xwayland, const char **error_name);
// Free allocated memory and clean up. Note: order is important here
void rv_server_deinit(struct rv_server *server);

#endif
