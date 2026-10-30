// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Server.zig for Oath, 2026-10-18.

#include <signal.h>

#include "rivulet.h"

#if WLR_HAS_XWAYLAND
// struct wlr_xwayland_shell_v1 (xwayland->shell_v1->global in globalFilter)
#include <wlr/xwayland/shell.h>
#endif

static int terminate(int signal_number, void *data);
static bool global_filter(const struct wl_client *client, const struct wl_global *global, void *data);
static void handle_renderer_lost(struct wl_listener *listener, void *data);
static struct wlr_renderer *renderer_create(struct wlr_backend *backend);
static void handle_new_xdg_toplevel(struct wl_listener *listener, void *data);
static void handle_new_toplevel_decoration(struct wl_listener *listener, void *data);
#if WLR_HAS_XWAYLAND
static void handle_new_xwayland_surface(struct wl_listener *listener, void *data);
#endif
static void handle_request_activate(struct wl_listener *listener, void *data);
static void handle_request_set_cursor_shape(struct wl_listener *listener, void *data);
static void handle_toplevel_capture_request(struct wl_listener *listener, void *data);

// Each failure sets *error_name to the Zig error that server.init() would
// return (main logs it as "error: <name>"). The init functions of the other
// areas only return false, so their failures are named after the init call.
#define TRY_CREATE(field, expr, name) \
	do { \
		server->field = (expr); \
		if (server->field == NULL) { \
			*error_name = (name); \
			return false; \
		} \
	} while (0)

#define TRY_INIT(expr, name) \
	do { \
		if (!(expr)) { \
			*error_name = (name); \
			return false; \
		} \
	} while (0)

bool rv_server_init(struct rv_server *server, bool runtime_xwayland, const char **error_name) {
	// We intentionally don't try to prevent memory leaks on error in this function
	// since river will exit during initialization anyway if there is an error.
	// This keeps the code simpler and more readable.

	struct wl_display *wl_server = wl_display_create();
	if (wl_server == NULL) {
		*error_name = "ServerCreateFailed";
		return false;
	}
	struct wl_event_loop *loop = wl_display_get_event_loop(wl_server);

	struct wlr_session *session = NULL;
	struct wlr_backend *backend = wlr_backend_autocreate(loop, &session);
	if (backend == NULL) {
		*error_name = "BackendCreateFailed";
		return false;
	}
	struct wlr_renderer *renderer = renderer_create(backend);
	if (renderer == NULL) {
		*error_name = "RendererCreateFailed";
		return false;
	}

	struct wlr_compositor *compositor = wlr_compositor_create(wl_server, 6, renderer);
	if (compositor == NULL) {
		*error_name = "OutOfMemory";
		return false;
	}

	struct wlr_xdg_foreign_registry *xdg_foreign_registry = wlr_xdg_foreign_registry_create(wl_server);
	if (xdg_foreign_registry == NULL) {
		*error_name = "OutOfMemory";
		return false;
	}

	// server.* = .{ ... }: every field not listed is undefined in Zig (the
	// managers, initialised below) or takes its default (null).
	*server = (struct rv_server){ 0 };
	server->wl_server = wl_server;
	TRY_CREATE(sigint_source, wl_event_loop_add_signal(loop, SIGINT, terminate, wl_server), "AddSignalFailed");
	TRY_CREATE(sigterm_source, wl_event_loop_add_signal(loop, SIGTERM, terminate, wl_server), "AddSignalFailed");

	TRY_CREATE(fixes, wlr_fixes_create(wl_server, 1), "OutOfMemory");

	server->backend = backend;
	server->session = session;
	server->renderer = renderer;
	TRY_CREATE(allocator, wlr_allocator_autocreate(backend, renderer), "AllocatorCreateFailed");

	TRY_CREATE(security_context_manager, wlr_security_context_manager_v1_create(wl_server), "OutOfMemory");

	TRY_CREATE(shm, wlr_shm_create_with_renderer(wl_server, 2, renderer), "OutOfMemory");
	TRY_CREATE(single_pixel_buffer_manager, wlr_single_pixel_buffer_manager_v1_create(wl_server), "OutOfMemory");
	TRY_CREATE(alpha_modifier, wlr_alpha_modifier_v1_create(wl_server), "OutOfMemory");

	TRY_CREATE(color_representation_manager,
		wlr_color_representation_manager_v1_create_with_renderer(wl_server, 1, renderer),
		"ColorRepresentationManagerV1CreateFailed");

	TRY_CREATE(viewporter, wlr_viewporter_create(wl_server), "OutOfMemory");
	TRY_CREATE(fractional_scale_manager, wlr_fractional_scale_manager_v1_create(wl_server, 1), "OutOfMemory");
	server->compositor = compositor;
	TRY_CREATE(subcompositor, wlr_subcompositor_create(wl_server), "OutOfMemory");
	TRY_CREATE(cursor_shape_manager, wlr_cursor_shape_manager_v1_create(wl_server, 2), "OutOfMemory");

	TRY_CREATE(xdg_shell, wlr_xdg_shell_create(wl_server, 5), "OutOfMemory");
	TRY_CREATE(xdg_decoration_manager, wlr_xdg_decoration_manager_v1_create(wl_server), "OutOfMemory");
	TRY_CREATE(xdg_activation, wlr_xdg_activation_v1_create(wl_server), "OutOfMemory");
	server->xdg_foreign_registry = xdg_foreign_registry;
	TRY_CREATE(xdg_foreign_v2, wlr_xdg_foreign_v2_create(wl_server, xdg_foreign_registry), "OutOfMemory");

	TRY_CREATE(data_device_manager, wlr_data_device_manager_create(wl_server), "OutOfMemory");
	TRY_CREATE(primary_selection_manager, wlr_primary_selection_v1_device_manager_create(wl_server), "OutOfMemory");
	TRY_CREATE(data_control_manager, wlr_ext_data_control_manager_v1_create(wl_server, 1), "OutOfMemory");
	TRY_CREATE(wlr_data_control_manager, wlr_data_control_manager_v1_create(wl_server), "OutOfMemory");

	TRY_CREATE(export_dmabuf_manager, wlr_export_dmabuf_manager_v1_create(wl_server), "OutOfMemory");
	TRY_CREATE(screencopy_manager, wlr_screencopy_manager_v1_create(wl_server), "OutOfMemory");

	TRY_CREATE(image_copy_capture_manager, wlr_ext_image_copy_capture_manager_v1_create(wl_server, 1),
		"OutOfMemory");
	TRY_CREATE(output_image_capture_source_manager,
		wlr_ext_output_image_capture_source_manager_v1_create(wl_server, 1), "OutOfMemory");

	TRY_CREATE(wlr_foreign_toplevel_manager, wlr_foreign_toplevel_manager_v1_create(wl_server), "OutOfMemory");
	TRY_CREATE(foreign_toplevel_list, wlr_ext_foreign_toplevel_list_v1_create(wl_server, 1), "OutOfMemory");
	TRY_CREATE(toplevel_capture_source_manager,
		wlr_ext_foreign_toplevel_image_capture_source_manager_v1_create(wl_server, 1), "OutOfMemory");

	TRY_CREATE(tearing_control_manager, wlr_tearing_control_manager_v1_create(wl_server, 1), "OutOfMemory");

	// Listener defaults (.init(handleX)) from the struct literal.
#if WLR_HAS_XWAYLAND
	server->new_xsurface.notify = handle_new_xwayland_surface;
#endif
	server->renderer_lost.notify = handle_renderer_lost;
	server->new_xdg_toplevel.notify = handle_new_xdg_toplevel;
	server->new_toplevel_decoration.notify = handle_new_toplevel_decoration;
	server->request_activate.notify = handle_request_activate;
	server->request_set_cursor_shape.notify = handle_request_set_cursor_shape;
	server->toplevel_capture_request.notify = handle_toplevel_capture_request;

	if (wlr_renderer_get_texture_formats(renderer, WLR_BUFFER_CAP_DMABUF) != NULL) {
		TRY_CREATE(linux_dmabuf, wlr_linux_dmabuf_v1_create_with_renderer(wl_server, 5, renderer), "OutOfMemory");
	}
	if (renderer->features.timeline && backend->features.timeline) {
		const int drm_fd = wlr_renderer_get_drm_fd(renderer);
		if (drm_fd >= 0) {
			server->linux_drm_syncobj_manager = wlr_linux_drm_syncobj_manager_v1_create(wl_server, 1, drm_fd);
		}
	}

	if (renderer->features.input_color_transform) {
		static const enum wp_color_manager_v1_render_intent render_intents[] = {
			WP_COLOR_MANAGER_V1_RENDER_INTENT_PERCEPTUAL,
		};
		size_t transfer_functions_len = 0;
		enum wp_color_manager_v1_transfer_function *transfer_functions =
			wlr_color_manager_v1_transfer_function_list_from_renderer(renderer, &transfer_functions_len);
		if (transfer_functions == NULL) {
			transfer_functions_len = 0;
		}
		size_t primaries_len = 0;
		enum wp_color_manager_v1_primaries *primaries =
			wlr_color_manager_v1_primaries_list_from_renderer(renderer, &primaries_len);
		if (primaries == NULL) {
			primaries_len = 0;
		}
		const struct wlr_color_manager_v1_options options = {
			.features = {
				.parametric = true,
				.set_mastering_display_primaries = true,
			},
			.render_intents = render_intents,
			.render_intents_len = sizeof(render_intents) / sizeof(render_intents[0]),
			.transfer_functions = transfer_functions,
			.transfer_functions_len = transfer_functions_len,
			.primaries = primaries,
			.primaries_len = primaries_len,
		};
		server->color_manager = wlr_color_manager_v1_create(wl_server, 2, &options);
		free(transfer_functions);
		free(primaries);
		if (server->color_manager == NULL) {
			*error_name = "ColorManagerV1CreateFailed";
			return false;
		}
	}

#if WLR_HAS_XWAYLAND
	if (runtime_xwayland) {
		TRY_CREATE(xwayland, wlr_xwayland_create(wl_server, compositor, false), "XwaylandCreateFailed");
		wl_signal_add(&server->xwayland->events.new_surface, &server->new_xsurface);
	}
#endif

	TRY_INIT(rv_window_manager_init(&server->wm), "WindowManagerInitFailed");
	TRY_INIT(rv_xkb_bindings_init(&server->xkb_bindings), "XkbBindingsInitFailed");
	TRY_INIT(rv_layer_shell_init(&server->layer_shell), "LayerShellInitFailed");
	TRY_INIT(rv_scene_init(&server->scene), "OutOfMemory");
	TRY_INIT(rv_output_manager_init(&server->om), "OutputManagerInitFailed");
	TRY_INIT(rv_input_manager_init(&server->input_manager), "InputManagerInitFailed");
	TRY_INIT(rv_libinput_config_init(&server->libinput_config), "LibinputConfigInitFailed");
	TRY_INIT(rv_xkb_config_init(&server->xkb_config), "XkbConfigInitFailed");
	TRY_INIT(rv_idle_inhibit_manager_init(&server->idle_inhibit_manager), "IdleInhibitManagerInitFailed");
	TRY_INIT(rv_lock_manager_init(&server->lock_manager), "LockManagerInitFailed");

	wl_signal_add(&server->renderer->events.lost, &server->renderer_lost);
	wl_signal_add(&server->xdg_shell->events.new_toplevel, &server->new_xdg_toplevel);
	wl_signal_add(&server->xdg_decoration_manager->events.new_toplevel_decoration,
		&server->new_toplevel_decoration);
	wl_signal_add(&server->xdg_activation->events.request_activate, &server->request_activate);
	wl_signal_add(&server->cursor_shape_manager->events.request_set_shape, &server->request_set_cursor_shape);
	wl_signal_add(&server->toplevel_capture_source_manager->events.new_request, &server->toplevel_capture_request);

	wl_display_set_global_filter(wl_server, global_filter, server);
	return true;
}

#undef TRY_CREATE
#undef TRY_INIT

// Free allocated memory and clean up. Note: order is important here
void rv_server_deinit(struct rv_server *server) {
	wl_event_source_remove(server->sigint_source);
	wl_event_source_remove(server->sigterm_source);

	wl_list_remove(&server->renderer_lost.link);
	wl_list_remove(&server->new_xdg_toplevel.link);
	wl_list_remove(&server->new_toplevel_decoration.link);
	wl_list_remove(&server->request_activate.link);
	wl_list_remove(&server->request_set_cursor_shape.link);
	wl_list_remove(&server->toplevel_capture_request.link);

	wl_list_remove(&server->input_manager.new_input.link);
	wl_list_remove(&server->om.new_output.link);

#if WLR_HAS_XWAYLAND
	if (server->xwayland != NULL) {
		wl_list_remove(&server->new_xsurface.link);
		wlr_xwayland_destroy(server->xwayland);
	}
#endif

	wl_display_destroy_clients(server->wl_server);

	wlr_backend_destroy(server->backend);

	// The scene graph needs to be destroyed after the backend but before the renderer
	// Output destruction requires the scene graph to still be around while the scene
	// graph may require the renderer to still be around to destroy textures it seems.
	wlr_scene_node_destroy(&server->scene.wlr_scene->tree.node);

	wlr_renderer_destroy(server->renderer);
	wlr_allocator_destroy(server->allocator);

	rv_output_manager_deinit(&server->om);
	rv_input_manager_deinit(&server->input_manager);
	rv_idle_inhibit_manager_deinit(&server->idle_inhibit_manager);
	rv_lock_manager_deinit(&server->lock_manager);
	rv_layer_shell_deinit(&server->layer_shell);

	wl_display_destroy(server->wl_server);
}

// Returns true if the global is allowlisted for security contexts
static bool allowlist(struct rv_server *server, const struct wl_global *global) {
	if (server->linux_dmabuf != NULL) {
		if (global == server->linux_dmabuf->global) {
			return true;
		}
	}
	if (server->linux_drm_syncobj_manager != NULL) {
		if (global == server->linux_drm_syncobj_manager->global) {
			return true;
		}
	}
	if (server->color_manager != NULL) {
		if (global == server->color_manager->global) {
			return true;
		}
	}

	// We must use the getInterface() approach for dynamically created globals
	// such as wl_output and wl_seat since the wl_global_create() function will
	// advertise the global to clients and invoke this filter before returning
	// the new global pointer.
	if (strcmp(wl_global_get_interface(global)->name, "wl_output") == 0 ||
		strcmp(wl_global_get_interface(global)->name, "wl_seat") == 0) {
		return true;
	}

	// For other globals I like the current pointer comparison approach as it
	// should catch river accidentally exposing multiple copies of e.g. wl_shm
	// with an assertion failure.
	return global == server->fixes->global ||
		global == server->shm->global ||
		global == server->single_pixel_buffer_manager->global ||
		global == server->alpha_modifier->global ||
		global == server->color_representation_manager->global ||
		global == server->viewporter->global ||
		global == server->fractional_scale_manager->global ||
		global == server->compositor->global ||
		global == server->subcompositor->global ||
		global == server->cursor_shape_manager->global ||
		global == server->xdg_shell->global ||
		global == server->xdg_decoration_manager->global ||
		global == server->xdg_activation->global ||
		global == server->xdg_foreign_v2->exporter.global ||
		global == server->xdg_foreign_v2->importer.global ||
		global == server->data_device_manager->global ||
		global == server->primary_selection_manager->global ||
		global == server->tearing_control_manager->global ||
		global == server->om.presentation->global ||
		global == server->om.xdg_output_manager->global ||
		global == server->input_manager.relative_pointer_manager->global ||
		global == server->input_manager.pointer_constraints->global ||
		global == server->input_manager.text_input_manager->global ||
		global == server->input_manager.tablet_manager->wl_global ||
		global == server->input_manager.pointer_gestures->global ||
		global == server->idle_inhibit_manager.wlr_manager->global;
}

// Returns true if the global is blocked for security contexts
static bool blocklist(struct rv_server *server, const struct wl_global *global) {
	return global == server->security_context_manager->global ||
		global == server->wm.global ||
		global == server->layer_shell.global ||
		global == server->layer_shell.wlr_shell->global ||
		global == server->xkb_bindings.global ||
		global == server->screencopy_manager->global ||
		global == server->image_copy_capture_manager->global ||
		global == server->output_image_capture_source_manager->global ||
		global == server->wlr_foreign_toplevel_manager->global ||
		global == server->foreign_toplevel_list->global ||
		global == server->toplevel_capture_source_manager->global ||
		global == server->export_dmabuf_manager->global ||
		global == server->data_control_manager->global ||
		global == server->wlr_data_control_manager->global ||
		global == server->om.wlr_output_manager->global ||
		global == server->om.power_manager->global ||
		global == server->om.gamma_control_manager->global ||
		global == server->libinput_config.global ||
		global == server->xkb_config.global ||
		global == server->input_manager.global ||
		global == server->input_manager.idle_notifier->global ||
		global == server->input_manager.virtual_pointer_manager->global ||
		global == server->input_manager.virtual_keyboard_manager->global ||
		global == server->input_manager.input_method_manager->global ||
		global == server->input_manager.shortcuts_inhibit_manager->global ||
		global == server->lock_manager.wlr_manager->global;
}

static bool global_filter(const struct wl_client *client, const struct wl_global *global, void *data) {
	struct rv_server *server = data;

	// Only expose the xwalyand_shell_v1 global to the Xwayland process.
#if WLR_HAS_XWAYLAND
	if (server->xwayland != NULL) {
		struct wlr_xwayland *xwayland = server->xwayland;
		if (global == xwayland->shell_v1->global) {
			if (xwayland->server != NULL) {
				return client == xwayland->server->client;
			}
			return false;
		}
	}
#endif

	// User-configurable allow/block lists are TODO
	const bool allowed = allowlist(server, global);
	const bool blocked = blocklist(server, global);
	assert(allowed != blocked);

	if (wlr_security_context_manager_v1_lookup_client(server->security_context_manager, client) != NULL) {
		return allowed;
	} else {
		return true;
	}
}

// Handle SIGINT and SIGTERM by gracefully stopping the server
static int terminate(int signal_number, void *data) {
	struct wl_display *wl_server = data;
	wl_display_terminate(wl_server);
	return 0;
}

static void gpu_reset_recover_idle(void *data);

// Oath: wlroots' renderer, or the pixman one when that fails and no
// WLR_RENDERER asked for it (a GPU Mesa has no driver for, such as QEMU's
// virtio-gpu without 3D). River 0.4.5 exits instead.
static struct wlr_renderer *renderer_create(struct wlr_backend *backend) {
	struct wlr_renderer *renderer = wlr_renderer_autocreate(backend);
	if (renderer == NULL && getenv("WLR_RENDERER") == NULL) {
		rv_err(RV_SCOPE_DEFAULT, "no GPU renderer, falling back to pixman (software)");
		renderer = wlr_pixman_renderer_create();
	}
	return renderer;
}

static void handle_renderer_lost(struct wl_listener *listener, void *data) {
	struct rv_server *server = wl_container_of(listener, server, renderer_lost);
	if (server->gpu_reset_recover != NULL) {
		rv_info(RV_SCOPE_DEFAULT, "ignoring GPU reset event, recovery already scheduled");
		return;
	}
	rv_info(RV_SCOPE_DEFAULT, "received GPU reset event, scheduling recovery");
	// There's a design wart in this wlroots API: calling wlr_renderer_destroy()
	// from inside this listener for the renderer lost event causes the assertion
	// that all listener lists are empty in wlr_renderer_destroy() to fail. This
	// happens even if river has already called server.renderer_lost.link.remove()
	// since wlroots uses wl_signal_emit_mutable(), which is implemented by adding
	// temporary links to the list during iteration.
	// Using an idle callback is the most straightforward way to work around this
	// design wart.
	struct wl_event_loop *event_loop = wl_display_get_event_loop(server->wl_server);
	server->gpu_reset_recover = wl_event_loop_add_idle(event_loop, gpu_reset_recover_idle, server);
	if (server->gpu_reset_recover == NULL) {
		rv_err(RV_SCOPE_DEFAULT, "out of memory");
		return;
	}
}

enum gpu_reset_recover_error {
	GPU_RESET_RECOVER_OK,
	GPU_RESET_RECOVER_RENDERER_CREATE_FAILED,
	GPU_RESET_RECOVER_ALLOCATOR_CREATE_FAILED,
};

static enum gpu_reset_recover_error gpu_reset_recover(struct rv_server *server) {
	rv_info(RV_SCOPE_DEFAULT, "recovering from GPU reset");
	struct wlr_renderer *new_renderer = renderer_create(server->backend);
	if (new_renderer == NULL) {
		return GPU_RESET_RECOVER_RENDERER_CREATE_FAILED;
	}

	struct wlr_allocator *new_allocator = wlr_allocator_autocreate(server->backend, new_renderer);
	if (new_allocator == NULL) {
		wlr_renderer_destroy(new_renderer);
		return GPU_RESET_RECOVER_ALLOCATOR_CREATE_FAILED;
	}
	// no failure allowed after this point

	wl_list_remove(&server->renderer_lost.link);
	wl_signal_add(&new_renderer->events.lost, &server->renderer_lost);

	wlr_compositor_set_renderer(server->compositor, new_renderer);

	{
		struct rv_output *output, *tmp;
		wl_list_for_each_safe(output, tmp, &server->om.outputs, link) {
			if (output->wlr_output != NULL) {
				// This should never fail here as failure with this combination of
				// renderer, allocator, and backend should have prevented creating
				// the output in the first place.
				(void)wlr_output_init_render(output->wlr_output, new_allocator, new_renderer);
			}
		}
	}

	wlr_renderer_destroy(server->renderer);
	server->renderer = new_renderer;

	wlr_allocator_destroy(server->allocator);
	server->allocator = new_allocator;

	return GPU_RESET_RECOVER_OK;
}

static void gpu_reset_recover_idle(void *data) {
	struct rv_server *server = data;
	server->gpu_reset_recover = NULL;
	// There's not much that can be done if creating a new renderer or allocator fails.
	// With luck there might be another GPU reset after which we try again and succeed.
	switch (gpu_reset_recover(server)) {
	case GPU_RESET_RECOVER_OK:
		break;
	case GPU_RESET_RECOVER_RENDERER_CREATE_FAILED:
		rv_err(RV_SCOPE_DEFAULT, "failed to create new renderer after GPU reset");
		break;
	case GPU_RESET_RECOVER_ALLOCATOR_CREATE_FAILED:
		rv_err(RV_SCOPE_DEFAULT, "failed to create new allocator after GPU reset");
		break;
	}
}

static void handle_new_xdg_toplevel(struct wl_listener *listener, void *data) {
	struct wlr_xdg_toplevel *xdg_toplevel = data;
	if (!rv_xdg_toplevel_create(xdg_toplevel)) {
		rv_err(RV_SCOPE_DEFAULT, "out of memory");
		wl_resource_post_no_memory(xdg_toplevel->resource);
		return;
	}
}

static void handle_new_toplevel_decoration(struct wl_listener *listener, void *data) {
	struct wlr_xdg_toplevel_decoration_v1 *wlr_decoration = data;
	rv_xdg_decoration_init(wlr_decoration);
}

#if WLR_HAS_XWAYLAND
static void handle_new_xwayland_surface(struct wl_listener *listener, void *data) {
	struct wlr_xwayland_surface *xsurface = data;
	if (xsurface->override_redirect) {
		if (!rv_xwayland_override_redirect_create(xsurface)) {
			rv_err(RV_SCOPE_DEFAULT, "out of memory");
			return;
		}
	} else {
		if (!rv_xwayland_window_create(xsurface)) {
			rv_err(RV_SCOPE_DEFAULT, "out of memory");
			return;
		}
	}
}
#endif

// @tagName of a SceneNodeData.Data tag.
static const char *scene_node_data_tag_name(enum rv_scene_node_data_data_tag tag) {
	switch (tag) {
	case RV_SCENE_NODE_DATA_DATA_WINDOW:
		return "window";
	case RV_SCENE_NODE_DATA_DATA_SHELL_SURFACE:
		return "shell_surface";
	case RV_SCENE_NODE_DATA_DATA_LOCK_SURFACE:
		return "lock_surface";
	case RV_SCENE_NODE_DATA_DATA_LAYER_SURFACE:
		return "layer_surface";
	case RV_SCENE_NODE_DATA_DATA_OVERRIDE_REDIRECT:
		return "override_redirect";
	}
	abort(); // unreachable
}

static void handle_request_activate(struct wl_listener *listener, void *data) {
	struct wlr_xdg_activation_v1_request_activate_event *event = data;
	struct rv_scene_node_data *node_data = rv_scene_node_data_from_surface(event->surface);
	if (node_data == NULL) {
		return;
	}
	switch (node_data->data.tag) {
	case RV_SCENE_NODE_DATA_DATA_WINDOW:
		break; // TODO support xdg-activation with a rwm extension protocol
	default:
		rv_info(RV_SCOPE_DEFAULT, "ignoring xdg-activation-v1 activate request of %s surface",
			scene_node_data_tag_name(node_data->data.tag));
		break;
	}
}

static void handle_request_set_cursor_shape(struct wl_listener *listener, void *data) {
	struct rv_server *server = wl_container_of(listener, server, request_set_cursor_shape);
	struct wlr_cursor_shape_manager_v1_request_set_shape_event *event = data;
	struct rv_seat *seat = event->seat_client->seat->data;

	const char *name = wlr_cursor_shape_v1_name(event->shape);

	if (event->tablet_tool != NULL) {
		struct wlr_tablet_v2_tablet_tool *wp_tool = event->tablet_tool;
		assert(event->device_type == WLR_CURSOR_SHAPE_MANAGER_V1_DEVICE_TYPE_TABLET_TOOL);

		struct rv_tablet_tool *tool = rv_tablet_tool_get(event->seat_client->seat, wp_tool->wlr_tool);
		if (tool == NULL) {
			return;
		}
		if (rv_tablet_tool_allow_set_cursor(tool, event->seat_client, event->serial)) {
			wlr_cursor_set_xcursor(tool->wlr_cursor, seat->cursor.xcursor_manager, name);
		}
	} else {
		assert(event->device_type == WLR_CURSOR_SHAPE_MANAGER_V1_DEVICE_TYPE_POINTER);

		// Only the client with pointer focus is allowed to set the cursor
		struct wlr_seat_client *focused_client = event->seat_client->seat->pointer_state.focused_client;
		if (event->seat_client == focused_client) {
			rv_cursor_set_image(&seat->cursor, (struct rv_cursor_image){
				.tag = RV_CURSOR_IMAGE_XCURSOR,
				.xcursor = name,
			});
		}
		// Except for the window manager client
		if (server->wm.object != NULL) {
			struct wl_resource *object = server->wm.object;
			if (event->seat_client->client == wl_resource_get_client(object) &&
				wl_resource_get_version(object) >= 4) {
				rv_cursor_set_wm_image(&seat->cursor, (struct rv_cursor_image){
					.tag = RV_CURSOR_IMAGE_XCURSOR,
					.xcursor = name,
				});
			}
		}
	}
}

static void handle_toplevel_capture_request(struct wl_listener *listener, void *data) {
	struct rv_server *server = wl_container_of(listener, server, toplevel_capture_request);
	struct wlr_ext_foreign_toplevel_image_capture_source_manager_v1_request *request = data;
	struct rv_window *window = request->toplevel_handle->data;
	if (window == NULL) {
		return;
	}

	struct wlr_ext_image_capture_source_v1 *capture_source = window->capture_source;
	if (capture_source == NULL) {
		capture_source = wlr_ext_image_capture_source_v1_create_with_scene_node(
			&window->capture_scene->tree.node,
			wl_display_get_event_loop(server->wl_server),
			server->allocator,
			server->renderer);
		if (capture_source == NULL) {
			rv_err(RV_SCOPE_DEFAULT, "failed to create ext image capture source");
			return;
		}
	}

	window->capture_source = capture_source;

	(void)wlr_ext_foreign_toplevel_image_capture_source_manager_v1_request_accept(request, capture_source);
}
