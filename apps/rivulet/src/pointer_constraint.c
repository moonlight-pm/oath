// SPDX-FileCopyrightText: © 2023 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/PointerConstraint.zig for Oath, 2026-10-18.

#include "rivulet.h"

static void handle_destroy(struct wl_listener *listener, void *data);
static void handle_commit(struct wl_listener *listener, void *data);
static void handle_node_destroy(struct wl_listener *listener, void *data);
static void warp_to_hint_if_set(struct rv_pointer_constraint *constraint);

bool rv_pointer_constraint_create(struct wlr_pointer_constraint_v1 *wlr_constraint) {
	struct rv_seat *seat = wlr_constraint->seat->data;

	struct rv_pointer_constraint *constraint = calloc(1, sizeof(*constraint));
	if (constraint == NULL) {
		return false;
	}

	*constraint = (struct rv_pointer_constraint){
		.wlr_constraint = wlr_constraint,
		.state = { .tag = RV_POINTER_CONSTRAINT_STATE_INACTIVE },
	};
	constraint->destroy.notify = handle_destroy;
	constraint->commit.notify = handle_commit;
	constraint->node_destroy.notify = handle_node_destroy;
	wlr_constraint->data = constraint;

	wl_signal_add(&wlr_constraint->events.destroy, &constraint->destroy);
	wl_signal_add(&wlr_constraint->surface->events.commit, &constraint->commit);

	if (seat->wlr_seat->keyboard_state.focused_surface != NULL) {
		struct wlr_surface *surface = seat->wlr_seat->keyboard_state.focused_surface;
		if (surface == wlr_constraint->surface) {
			assert(seat->cursor.constraint == NULL);
			seat->cursor.constraint = constraint;
			rv_pointer_constraint_maybe_activate(constraint);
		}
	}

	return true;
}

void rv_pointer_constraint_maybe_activate(struct rv_pointer_constraint *constraint) {
	struct rv_seat *seat = constraint->wlr_constraint->seat->data;

	assert(seat->cursor.constraint == constraint);

	if (constraint->state.tag == RV_POINTER_CONSTRAINT_STATE_ACTIVE) {
		return;
	}

	if (seat->cursor.mode.tag == RV_CURSOR_MODE_OP) {
		return;
	}

	struct rv_scene_at_result result;
	if (!rv_scene_at(&server.scene, seat->cursor.wlr_cursor->x, seat->cursor.wlr_cursor->y, &result)) {
		return;
	}
	if (result.surface != constraint->wlr_constraint->surface) {
		return;
	}

	const int32_t sx = (int32_t)result.sx;
	const int32_t sy = (int32_t)result.sy;
	if (!pixman_region32_contains_point(&constraint->wlr_constraint->region, sx, sy, NULL)) {
		return;
	}

	assert(constraint->state.tag == RV_POINTER_CONSTRAINT_STATE_INACTIVE);
	constraint->state = (struct rv_pointer_constraint_state){
		.tag = RV_POINTER_CONSTRAINT_STATE_ACTIVE,
		.active = {
			.node = result.node,
			.sx = result.sx,
			.sy = result.sy,
		},
	};
	wl_signal_add(&result.node->events.destroy, &constraint->node_destroy);

	rv_info(RV_SCOPE_INPUT, "activating pointer constraint");

	wlr_pointer_constraint_v1_send_activated(constraint->wlr_constraint);
}

// Called when the cursor position or content in the scene graph changes
void rv_pointer_constraint_update_state(struct rv_pointer_constraint *constraint) {
	struct rv_seat *seat = constraint->wlr_constraint->seat->data;

	rv_pointer_constraint_maybe_activate(constraint);

	if (constraint->state.tag != RV_POINTER_CONSTRAINT_STATE_ACTIVE) {
		return;
	}

	int lx;
	int ly;
	if (!wlr_scene_node_coords(constraint->state.active.node, &lx, &ly)) {
		rv_info(RV_SCOPE_INPUT, "deactivating pointer constraint, scene node disabled");
		rv_pointer_constraint_deactivate(constraint);
		return;
	}

	const double sx = constraint->state.active.sx;
	const double sy = constraint->state.active.sy;
	const double warp_lx = (double)lx + sx;
	const double warp_ly = (double)ly + sy;
	if (!wlr_cursor_warp(seat->cursor.wlr_cursor, NULL, warp_lx, warp_ly)) {
		rv_info(RV_SCOPE_INPUT, "deactivating pointer constraint, could not warp cursor");
		rv_pointer_constraint_deactivate(constraint);
		return;
	}

	// It is possible for the cursor to end up outside of the constraint region despite the warp
	// if, for example, the a keybinding is used to resize the window.
	if (!pixman_region32_contains_point(&constraint->wlr_constraint->region, (int)sx, (int)sy, NULL)) {
		rv_info(RV_SCOPE_INPUT, "deactivating pointer constraint, cursor outside region despite warp");
		rv_pointer_constraint_deactivate(constraint);
		return;
	}
}

void rv_pointer_constraint_confine(struct rv_pointer_constraint *constraint, double *dx, double *dy) {
	assert(constraint->state.tag == RV_POINTER_CONSTRAINT_STATE_ACTIVE);
	assert(constraint->wlr_constraint->type == WLR_POINTER_CONSTRAINT_V1_CONFINED);

	const pixman_region32_t *region = &constraint->wlr_constraint->region;
	const double sx = constraint->state.active.sx;
	const double sy = constraint->state.active.sy;
	double new_sx;
	double new_sy;
	// Zig evaluates the call inside assert(); keep the call outside of it.
	const bool confined = wlr_region_confine(region, sx, sy, sx + *dx, sy + *dy, &new_sx, &new_sy);
	assert(confined);
	(void)confined;

	*dx = new_sx - sx;
	*dy = new_sy - sy;

	constraint->state.active.sx = new_sx;
	constraint->state.active.sy = new_sy;
}

void rv_pointer_constraint_deactivate(struct rv_pointer_constraint *constraint) {
	struct rv_seat *seat = constraint->wlr_constraint->seat->data;

	assert(seat->cursor.constraint == constraint);
	assert(constraint->state.tag == RV_POINTER_CONSTRAINT_STATE_ACTIVE);

	warp_to_hint_if_set(constraint);

	constraint->state = (struct rv_pointer_constraint_state){ .tag = RV_POINTER_CONSTRAINT_STATE_INACTIVE };
	wl_list_remove(&constraint->node_destroy.link);
	wlr_pointer_constraint_v1_send_deactivated(constraint->wlr_constraint);
}

static void warp_to_hint_if_set(struct rv_pointer_constraint *constraint) {
	struct rv_seat *seat = constraint->wlr_constraint->seat->data;

	if (constraint->wlr_constraint->current.cursor_hint.enabled) {
		int lx;
		int ly;
		wlr_scene_node_coords(constraint->state.active.node, &lx, &ly);

		const double sx = constraint->wlr_constraint->current.cursor_hint.x;
		const double sy = constraint->wlr_constraint->current.cursor_hint.y;
		wlr_cursor_warp(seat->cursor.wlr_cursor, NULL, (double)lx + sx, (double)ly + sy);
		wlr_seat_pointer_warp(seat->wlr_seat, sx, sy);
	}
}

static void handle_node_destroy(struct wl_listener *listener, void *data) {
	struct rv_pointer_constraint *constraint = wl_container_of(listener, constraint, node_destroy);

	rv_info(RV_SCOPE_INPUT, "deactivating pointer constraint, scene node destroyed");
	rv_pointer_constraint_deactivate(constraint);
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_pointer_constraint *constraint = wl_container_of(listener, constraint, destroy);
	struct rv_seat *seat = constraint->wlr_constraint->seat->data;

	if (constraint->state.tag == RV_POINTER_CONSTRAINT_STATE_ACTIVE) {
		// We can't simply call deactivate() here as it calls sendDeactivated(),
		// which could in the case of a oneshot constraint lifetime recursively
		// destroy the constraint.
		warp_to_hint_if_set(constraint);
		wl_list_remove(&constraint->node_destroy.link);
	}

	wl_list_remove(&constraint->destroy.link);
	wl_list_remove(&constraint->commit.link);

	if (seat->cursor.constraint == constraint) {
		seat->cursor.constraint = NULL;
	}

	free(constraint);
}

// It is necessary to listen for the commit event rather than the set_region
// event as the latter is not triggered by wlroots when the input region of
// the surface changes.
static void handle_commit(struct wl_listener *listener, void *data) {
	struct rv_pointer_constraint *constraint = wl_container_of(listener, constraint, commit);
	struct rv_seat *seat = constraint->wlr_constraint->seat->data;

	switch (constraint->state.tag) {
	case RV_POINTER_CONSTRAINT_STATE_ACTIVE: {
		struct rv_pointer_constraint_state_active state = constraint->state.active;
		const int32_t sx = (int32_t)state.sx;
		const int32_t sy = (int32_t)state.sy;
		if (!pixman_region32_contains_point(&constraint->wlr_constraint->region, sx, sy, NULL)) {
			rv_info(RV_SCOPE_INPUT,
				"deactivating pointer constraint, (input) region change left pointer outside constraint");
			rv_pointer_constraint_deactivate(constraint);
		}
		break;
	}
	case RV_POINTER_CONSTRAINT_STATE_INACTIVE:
		if (seat->cursor.constraint == constraint) {
			rv_pointer_constraint_maybe_activate(constraint);
		}
		break;
	}
}
