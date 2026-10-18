// SPDX-FileCopyrightText: © 2024 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/Tablet.zig for Oath, 2026-10-18.
//
// struct rv_tablet is defined in seat.h (see cursor.h for why).

#include "river.h"

struct rv_tablet *rv_tablet_create(struct rv_seat *seat, struct wlr_input_device *wlr_device, bool virtual) {
	assert(wlr_device->type == WLR_INPUT_DEVICE_TABLET);

	struct rv_tablet *tablet = calloc(1, sizeof(*tablet));
	if (tablet == NULL) {
		return NULL;
	}

	struct wlr_tablet_manager_v2 *tablet_manager = server.input_manager.tablet_manager;

	struct wlr_tablet_v2_tablet *wp_tablet = wlr_tablet_create(tablet_manager, seat->wlr_seat, wlr_device);
	if (wp_tablet == NULL) {
		free(tablet);
		return NULL;
	}
	tablet->wp_tablet = wp_tablet;
	if (!rv_input_device_init(&tablet->device, seat, wlr_device, virtual)) {
		free(tablet);
		return NULL;
	}

	return tablet;
}

void rv_tablet_destroy(struct rv_tablet *tablet) {
	rv_input_device_deinit(&tablet->device);
	free(tablet);
}
