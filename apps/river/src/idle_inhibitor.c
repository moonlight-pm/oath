// SPDX-FileCopyrightText: © 2022 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/IdleInhibitor.zig for Oath, 2026-10-18.

#include "river.h"

static void handle_destroy(struct wl_listener *listener, void *data);

bool rv_idle_inhibitor_create(struct wlr_idle_inhibitor_v1 *wlr_inhibitor,
		struct rv_idle_inhibit_manager *inhibit_manager) {
	struct rv_idle_inhibitor *inhibitor = calloc(1, sizeof(*inhibitor));
	if (inhibitor == NULL) {
		return false;
	}

	*inhibitor = (struct rv_idle_inhibitor){
		.inhibit_manager = inhibit_manager,
		.wlr_inhibitor = wlr_inhibitor,
	};
	inhibitor->listen_destroy.notify = handle_destroy;
	wl_signal_add(&wlr_inhibitor->events.destroy, &inhibitor->listen_destroy);

	wl_list_insert(inhibit_manager->inhibitors.prev, &inhibitor->link);

	rv_idle_inhibit_manager_check_active(inhibit_manager);

	return true;
}

void rv_idle_inhibitor_destroy(struct rv_idle_inhibitor *inhibitor) {
	wl_list_remove(&inhibitor->listen_destroy.link);

	wl_list_remove(&inhibitor->link);

	rv_idle_inhibit_manager_check_active(inhibitor->inhibit_manager);

	free(inhibitor);
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_idle_inhibitor *inhibitor = wl_container_of(listener, inhibitor, listen_destroy);

	rv_idle_inhibitor_destroy(inhibitor);
}
