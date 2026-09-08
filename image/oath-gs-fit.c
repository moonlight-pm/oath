/* sola-arcade Fit poke for nested gamescope X (not the gamescope CLI).
 *
 * Stock gamescope has no --nested-auto-resize. Arcade's proven path:
 *   GAMESCOPE_FORCE_WINDOWS_FULLSCREEN=1 on the nested root
 *   GAMESCOPE_XWAYLAND_MODE_CONTROL = [server_idx, w, h, allowSuperRes]
 *   ConfigureWindow focused client to 0,0,w,h
 * DISPLAY must be the nested X. On canto that is :0 (gamescope's
 * Xwayland). Never poke the host. Never pass --force-windows-fullscreen
 * as a gamescope argv (that aborted the Wayland backend).
 */
#include <stdio.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <xcb/xcb.h>

static xcb_atom_t intern(xcb_connection_t *c, const char *name) {
	xcb_intern_atom_cookie_t ck = xcb_intern_atom(c, 1, (uint16_t)strlen(name), name);
	xcb_intern_atom_reply_t *r = xcb_intern_atom_reply(c, ck, NULL);
	xcb_atom_t a = r ? r->atom : 0;
	free(r);
	return a;
}

static int get_card32(xcb_connection_t *c, xcb_window_t win, xcb_atom_t atom, uint32_t *out) {
	xcb_get_property_cookie_t ck =
		xcb_get_property(c, 0, win, atom, XCB_ATOM_CARDINAL, 0, 1);
	xcb_get_property_reply_t *r = xcb_get_property_reply(c, ck, NULL);
	int ok = 0;
	if (r && r->value_len >= 1 && r->format == 32) {
		uint32_t *v = xcb_get_property_value(r);
		if (v) {
			*out = v[0];
			ok = 1;
		}
	}
	free(r);
	return ok;
}

int main(int argc, char **argv) {
	uint32_t w = 1920, h = 1080, server = 0, focused = 0;
	xcb_connection_t *c;
	xcb_window_t root;
	xcb_atom_t mode_a, force_a, focus_a, server_a;
	uint32_t mode[4], cfg[4];
	const xcb_setup_t *setup;
	xcb_screen_t *screen;

	if (argc >= 3) {
		w = (uint32_t)atoi(argv[1]);
		h = (uint32_t)atoi(argv[2]);
	}
	if (w < 64 || h < 64) {
		fprintf(stderr, "oath-gs-fit: size %ux%u too small\n", w, h);
		return 1;
	}
	c = xcb_connect(NULL, NULL);
	if (!c || xcb_connection_has_error(c)) {
		fprintf(stderr, "oath-gs-fit: no DISPLAY\n");
		return 1;
	}
	setup = xcb_get_setup(c);
	screen = xcb_setup_roots_iterator(setup).data;
	root = screen->root;
	server_a = intern(c, "GAMESCOPE_XWAYLAND_SERVER_ID");
	mode_a = intern(c, "GAMESCOPE_XWAYLAND_MODE_CONTROL");
	force_a = intern(c, "GAMESCOPE_FORCE_WINDOWS_FULLSCREEN");
	focus_a = intern(c, "GAMESCOPE_FOCUSED_WINDOW");
	if (!server_a || !mode_a || !force_a) {
		fprintf(stderr, "oath-gs-fit: not a gamescope nested X (missing atoms)\n");
		xcb_disconnect(c);
		return 1;
	}
	if (!get_card32(c, root, server_a, &server)) {
		fprintf(stderr, "oath-gs-fit: GAMESCOPE_XWAYLAND_SERVER_ID unset — refusing poke\n");
		xcb_disconnect(c);
		return 1;
	}
	xcb_change_property(c, XCB_PROP_MODE_REPLACE, root, force_a, XCB_ATOM_CARDINAL, 32, 1,
			    (uint32_t[]){1});
	mode[0] = server;
	mode[1] = w;
	mode[2] = h;
	mode[3] = 1;
	xcb_change_property(c, XCB_PROP_MODE_REPLACE, root, mode_a, XCB_ATOM_CARDINAL, 32, 4, mode);
	xcb_flush(c);
	if (focus_a && get_card32(c, root, focus_a, &focused) && focused && focused != root) {
		cfg[0] = 0;
		cfg[1] = 0;
		cfg[2] = w;
		cfg[3] = h;
		xcb_configure_window(c, focused,
				     XCB_CONFIG_WINDOW_X | XCB_CONFIG_WINDOW_Y | XCB_CONFIG_WINDOW_WIDTH |
					     XCB_CONFIG_WINDOW_HEIGHT,
				     cfg);
		xcb_flush(c);
	}
	fprintf(stderr, "oath-gs-fit: nested %ux%u server=%u focused=0x%x\n", w, h, server, focused);
	xcb_disconnect(c);
	return 0;
}
