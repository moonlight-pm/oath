/* Tiny X11 WM for rootful Xwayland (Steam nest).
 *
 * Host River has no XWayland. Steam runs on `Xwayland :2 -decorate`.
 * Without a WM, Steam's library window is created at INT_MIN (logged
 * as 805240832,805240832), stays unmapped / hidden, and the nest
 * looks black. This process:
 *   - SubstructureRedirect on the root
 *   - maps every MapRequest
 *   - clamps ConfigureRequest / MapRequest geometry onto the screen
 *   - keeps one InputOnly window mapped so Xwayland does not tear
 *     down its Wayland surface when Steam replaces the login popup
 */
#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

typedef struct xcb_connection_t xcb_connection_t;
typedef uint32_t xcb_window_t;
typedef uint32_t xcb_visualid_t;
typedef uint32_t xcb_colormap_t;

typedef struct {
	unsigned int sequence;
} xcb_void_cookie_t;

typedef struct {
	uint8_t response_type;
	uint8_t pad0;
	uint16_t sequence;
	uint32_t pad[7];
	uint32_t full_sequence;
} xcb_generic_event_t;

typedef struct {
	uint8_t response_type;
	uint8_t pad0;
	uint16_t sequence;
	xcb_window_t parent;
	xcb_window_t window;
} xcb_map_request_event_t;

typedef struct {
	uint8_t response_type;
	uint8_t stack_mode;
	uint16_t sequence;
	xcb_window_t parent;
	xcb_window_t window;
	xcb_window_t sibling;
	int16_t x;
	int16_t y;
	uint16_t width;
	uint16_t height;
	uint16_t border_width;
	uint16_t value_mask;
} xcb_configure_request_event_t;

typedef struct {
	uint8_t status;
	uint8_t pad0;
	uint16_t protocol_major_version;
	uint16_t protocol_minor_version;
	uint16_t length;
	uint32_t release_number;
	uint32_t resource_id_base;
	uint32_t resource_id_mask;
	uint32_t motion_buffer_size;
	uint16_t vendor_len;
	uint16_t maximum_request_length;
	uint8_t roots_len;
	uint8_t pixmap_formats_len;
	uint8_t image_byte_order;
	uint8_t bitmap_format_bit_order;
	uint8_t bitmap_format_scanline_unit;
	uint8_t bitmap_format_scanline_pad;
	uint8_t min_keycode;
	uint8_t max_keycode;
	uint8_t pad1[4];
} xcb_setup_t;

typedef struct {
	xcb_window_t root;
	xcb_colormap_t default_colormap;
	uint32_t white_pixel;
	uint32_t black_pixel;
	uint32_t current_input_masks;
	uint16_t width_in_pixels;
	uint16_t height_in_pixels;
	uint16_t width_in_millimeters;
	uint16_t height_in_millimeters;
	uint16_t min_installed_maps;
	uint16_t max_installed_maps;
	xcb_visualid_t root_visual;
	uint8_t backing_stores;
	uint8_t save_unders;
	uint8_t root_depth;
	uint8_t allowed_depths_len;
} xcb_screen_t;

typedef struct {
	xcb_screen_t *data;
	int rem;
	int index;
} xcb_screen_iterator_t;

enum {
	XCB_MAP_REQUEST = 20,
	XCB_CONFIGURE_REQUEST = 23,
	XCB_CW_EVENT_MASK = 1 << 11,
	XCB_EVENT_MASK_SUBSTRUCTURE_REDIRECT = 1 << 8,
	XCB_EVENT_MASK_SUBSTRUCTURE_NOTIFY = 1 << 19,
	XCB_CONFIG_WINDOW_X = 1,
	XCB_CONFIG_WINDOW_Y = 2,
	XCB_CONFIG_WINDOW_WIDTH = 4,
	XCB_CONFIG_WINDOW_HEIGHT = 8,
	XCB_CONFIG_WINDOW_BORDER_WIDTH = 16,
	XCB_WINDOW_CLASS_INPUT_ONLY = 2,
	XCB_COPY_FROM_PARENT = 0,
};

xcb_connection_t *xcb_connect(const char *displayname, int *screenp);
int xcb_connection_has_error(xcb_connection_t *c);
void xcb_disconnect(xcb_connection_t *c);
const xcb_setup_t *xcb_get_setup(xcb_connection_t *c);
xcb_screen_iterator_t xcb_setup_roots_iterator(const xcb_setup_t *r);
int xcb_flush(xcb_connection_t *c);
xcb_generic_event_t *xcb_wait_for_event(xcb_connection_t *c);
uint32_t xcb_generate_id(xcb_connection_t *c);
xcb_void_cookie_t xcb_change_window_attributes(xcb_connection_t *c, xcb_window_t window,
    uint32_t value_mask, const void *value_list);
xcb_void_cookie_t xcb_map_window(xcb_connection_t *c, xcb_window_t window);
xcb_void_cookie_t xcb_configure_window(xcb_connection_t *c, xcb_window_t window,
    uint16_t value_mask, const void *value_list);
xcb_void_cookie_t xcb_create_window(xcb_connection_t *c, uint8_t depth, xcb_window_t wid,
    xcb_window_t parent, int16_t x, int16_t y, uint16_t width, uint16_t height,
    uint16_t border_width, uint16_t class_, xcb_visualid_t visual, uint32_t value_mask,
    const void *value_list);

static uint16_t screen_w, screen_h;
static xcb_window_t root;

static void place(xcb_connection_t *c, xcb_window_t w, int x, int y, int width, int height)
{
	uint32_t v[4];
	uint16_t mask;

	if (width < 64)
		width = screen_w;
	if (height < 64)
		height = screen_h;
	if (width > screen_w)
		width = screen_w;
	if (height > screen_h)
		height = screen_h;
	if (x < 0 || x + width > screen_w)
		x = 0;
	if (y < 0 || y + height > screen_h)
		y = 0;
	v[0] = (uint32_t)x;
	v[1] = (uint32_t)y;
	v[2] = (uint32_t)width;
	v[3] = (uint32_t)height;
	mask = XCB_CONFIG_WINDOW_X | XCB_CONFIG_WINDOW_Y | XCB_CONFIG_WINDOW_WIDTH |
	       XCB_CONFIG_WINDOW_HEIGHT;
	xcb_configure_window(c, w, mask, v);
}

int main(void)
{
	xcb_connection_t *c;
	xcb_screen_iterator_t si;
	xcb_window_t hold;
	uint32_t mask;
	int scr = 0;

	c = xcb_connect(NULL, &scr);
	if (!c || xcb_connection_has_error(c)) {
		fprintf(stderr, "oath-xwm: cannot open display %s\n",
		    getenv("DISPLAY") ? getenv("DISPLAY") : "(null)");
		return 1;
	}
	si = xcb_setup_roots_iterator(xcb_get_setup(c));
	if (!si.data) {
		fprintf(stderr, "oath-xwm: no screen\n");
		return 1;
	}
	root = si.data->root;
	screen_w = si.data->width_in_pixels;
	screen_h = si.data->height_in_pixels;
	if (screen_w < 64)
		screen_w = 1920;
	if (screen_h < 64)
		screen_h = 1080;

	mask = XCB_EVENT_MASK_SUBSTRUCTURE_REDIRECT | XCB_EVENT_MASK_SUBSTRUCTURE_NOTIFY;
	xcb_change_window_attributes(c, root, XCB_CW_EVENT_MASK, &mask);
	if (xcb_flush(c) <= 0 || xcb_connection_has_error(c)) {
		fprintf(stderr, "oath-xwm: SubstructureRedirect denied (another WM?)\n");
		return 1;
	}

	/* Keep Xwayland's Wayland surface alive across Steam login → library. */
	hold = xcb_generate_id(c);
	xcb_create_window(c, 0, hold, root, 0, 0, 1, 1, 0, XCB_WINDOW_CLASS_INPUT_ONLY,
	    XCB_COPY_FROM_PARENT, 0, NULL);
	xcb_map_window(c, hold);
	xcb_flush(c);
	fprintf(stderr, "oath-xwm: root %ux%u hold 0x%x\n", screen_w, screen_h, hold);

	for (;;) {
		xcb_generic_event_t *e;
		uint8_t t;

		e = xcb_wait_for_event(c);
		if (!e) {
			if (xcb_connection_has_error(c))
				break;
			continue;
		}
		t = e->response_type & 0x7f;
		if (t == XCB_MAP_REQUEST) {
			xcb_map_request_event_t *m = (xcb_map_request_event_t *)e;
			if (m->window != hold) {
				place(c, m->window, 0, 0, screen_w, screen_h);
				xcb_map_window(c, m->window);
				xcb_flush(c);
			}
		} else if (t == XCB_CONFIGURE_REQUEST) {
			xcb_configure_request_event_t *r = (xcb_configure_request_event_t *)e;
			int x = r->x, y = r->y, w = r->width, h = r->height;
			if (!(r->value_mask & XCB_CONFIG_WINDOW_X))
				x = 0;
			if (!(r->value_mask & XCB_CONFIG_WINDOW_Y))
				y = 0;
			if (!(r->value_mask & XCB_CONFIG_WINDOW_WIDTH))
				w = screen_w;
			if (!(r->value_mask & XCB_CONFIG_WINDOW_HEIGHT))
				h = screen_h;
			if (r->window != hold)
				place(c, r->window, x, y, w, h);
			xcb_flush(c);
		}
		free(e);
	}
	return 0;
}
