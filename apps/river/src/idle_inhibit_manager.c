// SPDX-FileCopyrightText: © 2022 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/IdleInhibitManager.zig for Oath, 2026-10-18.

#include "river.h"

static void handle_new_idle_inhibitor(struct wl_listener *listener, void *data);

bool rv_idle_inhibit_manager_init(struct rv_idle_inhibit_manager *inhibit_manager) {
	struct wlr_idle_inhibit_manager_v1 *wlr_manager = wlr_idle_inhibit_v1_create(server.wl_server);
	if (wlr_manager == NULL) {
		return false;
	}
	*inhibit_manager = (struct rv_idle_inhibit_manager){
		.wlr_manager = wlr_manager,
	};
	inhibit_manager->new_idle_inhibitor.notify = handle_new_idle_inhibitor;
	wl_list_init(&inhibit_manager->inhibitors);

	wl_signal_add(&inhibit_manager->wlr_manager->events.new_inhibitor, &inhibit_manager->new_idle_inhibitor);

	return true;
}

void rv_idle_inhibit_manager_deinit(struct rv_idle_inhibit_manager *inhibit_manager) {
	while (!wl_list_empty(&inhibit_manager->inhibitors)) {
		struct rv_idle_inhibitor *inhibitor = wl_container_of(inhibit_manager->inhibitors.next, inhibitor, link);
		rv_idle_inhibitor_destroy(inhibitor);
	}
	wl_list_remove(&inhibit_manager->new_idle_inhibitor.link);
}

void rv_idle_inhibit_manager_check_active(struct rv_idle_inhibit_manager *inhibit_manager) {
	bool inhibited = false;
	struct rv_idle_inhibitor *inhibitor, *tmp;
	wl_list_for_each_safe(inhibitor, tmp, &inhibit_manager->inhibitors, link) {
		struct rv_scene_node_data *node_data = rv_scene_node_data_from_surface(inhibitor->wlr_inhibitor->surface);
		if (node_data == NULL) {
			continue;
		}
		switch (node_data->data.tag) {
		// TODO be stricter here so that clients don't abuse the protocol
		case RV_SCENE_NODE_DATA_DATA_WINDOW:
		case RV_SCENE_NODE_DATA_DATA_SHELL_SURFACE:
		case RV_SCENE_NODE_DATA_DATA_LOCK_SURFACE:
		case RV_SCENE_NODE_DATA_DATA_LAYER_SURFACE:
		case RV_SCENE_NODE_DATA_DATA_OVERRIDE_REDIRECT:
			inhibited = true;
			break;
		}
		if (inhibited) {
			break;
		}
	}

	wlr_idle_notifier_v1_set_inhibited(server.input_manager.idle_notifier, inhibited);
}

static void handle_new_idle_inhibitor(struct wl_listener *listener, void *data) {
	struct rv_idle_inhibit_manager *inhibit_manager = wl_container_of(listener, inhibit_manager, new_idle_inhibitor);
	struct wlr_idle_inhibitor_v1 *inhibitor = data;
	if (!rv_idle_inhibitor_create(inhibitor, inhibit_manager)) {
		rv_err(RV_SCOPE_DEFAULT, "out of memory");
		return;
	}
}
