// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/DragIcon.zig for Oath, 2026-10-18.

#include "river.h"

static void handle_destroy(struct wl_listener *listener, void *data);

bool rv_drag_icon_create(struct wlr_drag_icon *wlr_drag_icon, struct rv_cursor *cursor) {
	struct wlr_scene_tree *scene_drag_icon = wlr_scene_drag_icon_create(server.scene.drag_icons, wlr_drag_icon);
	if (scene_drag_icon == NULL) {
		return false;
	}

	struct rv_drag_icon *drag_icon = calloc(1, sizeof(*drag_icon));
	if (drag_icon == NULL) {
		wlr_scene_node_destroy(&scene_drag_icon->node);
		return false;
	}

	*drag_icon = (struct rv_drag_icon){
		.wlr_drag_icon = wlr_drag_icon,
		.scene_drag_icon = scene_drag_icon,
	};
	drag_icon->destroy.notify = handle_destroy;
	scene_drag_icon->node.data = drag_icon;

	rv_drag_icon_update_position(drag_icon, cursor);

	wl_signal_add(&wlr_drag_icon->events.destroy, &drag_icon->destroy);

	return true;
}

void rv_drag_icon_update_position(struct rv_drag_icon *drag_icon, struct rv_cursor *cursor) {
	switch (drag_icon->wlr_drag_icon->drag->grab_type) {
	case WLR_DRAG_GRAB_KEYBOARD:
		abort(); // unreachable
	case WLR_DRAG_GRAB_KEYBOARD_POINTER:
		wlr_scene_node_set_position(
			&drag_icon->scene_drag_icon->node,
			(int)cursor->wlr_cursor->x,
			(int)cursor->wlr_cursor->y
		);
		break;
	case WLR_DRAG_GRAB_KEYBOARD_TOUCH: {
		const int32_t touch_id = drag_icon->wlr_drag_icon->drag->touch_id;
		// cursor.touch_points.get(touch_id)
		for (size_t i = 0; i < cursor->touch_points.len; i++) {
			if (cursor->touch_points.items[i].touch_id == touch_id) {
				const struct rv_cursor_layout_point point = cursor->touch_points.items[i].point;
				wlr_scene_node_set_position(
					&drag_icon->scene_drag_icon->node,
					(int)point.lx,
					(int)point.ly
				);
				break;
			}
		}
		break;
	}
	}
}

static void handle_destroy(struct wl_listener *listener, void *data) {
	struct rv_drag_icon *drag_icon = wl_container_of(listener, drag_icon, destroy);

	wl_list_remove(&drag_icon->destroy.link);

	free(drag_icon);
}
