/* 64-bit LD_PRELOAD for steamwebhelper.
 *
 * CEF/Ozone creates X11/xcb windows at 0x0 (Shared JS Context and
 * gamepadui views). A 0x0 backing store never paints; gamescope then
 * composites an empty nest. Clamp 0 size to the nest (1920x1080).
 *
 * INT_MIN position is the offscreen helper — move it to 0,0 but still
 * give it a real buffer so CEF does not keep "Invalid browser dimensions".
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static void slog(const char *fmt, ...)
{
	va_list ap;
	FILE *f;
	va_start(ap, fmt);
	vfprintf(stderr, fmt, ap);
	va_end(ap);
	f = fopen("/tmp/oath-steam-shim.log", "a");
	if (!f)
		return;
	va_start(ap, fmt);
	vfprintf(f, fmt, ap);
	va_end(ap);
	fclose(f);
}

static void clamp_wh(uint16_t *w, uint16_t *h)
{
	if (w && *w == 0)
		*w = 1920;
	if (h && *h == 0)
		*h = 1080;
}

static void clamp_xywh_int(int *x, int *y, unsigned int *w, unsigned int *h)
{
	if (x && (*x < -4096 || *x > 7680))
		*x = 0;
	if (y && (*y < -4096 || *y > 4320))
		*y = 0;
	if (w && (*w == 0 || *w > 7680))
		*w = 1920;
	if (h && (*h == 0 || *h > 4320))
		*h = 1080;
}

typedef unsigned long XWindow;

XWindow XCreateWindow(void *dpy, XWindow parent, int x, int y, unsigned int width,
    unsigned int height, unsigned int border_width, int depth, unsigned int class_,
    void *visual, unsigned long valuemask, void *attributes)
{
	static XWindow (*real)(void *, XWindow, int, int, unsigned int, unsigned int,
	    unsigned int, int, unsigned int, void *, unsigned long, void *);
	if (!real)
		real = dlsym(RTLD_NEXT, "XCreateWindow");
	if (!real)
		return 0;
	if (width == 0 || height == 0 || x < -4096 || y < -4096)
		slog("oath-cefgeom: XCreateWindow %d,%d %ux%u -> 1920x1080\n", x, y, width, height);
	clamp_xywh_int(&x, &y, &width, &height);
	return real(dpy, parent, x, y, width, height, border_width, depth, class_, visual,
	    valuemask, attributes);
}

int XResizeWindow(void *dpy, XWindow w, unsigned int width, unsigned int height)
{
	static int (*real)(void *, XWindow, unsigned int, unsigned int);
	if (!real)
		real = dlsym(RTLD_NEXT, "XResizeWindow");
	if (!real)
		return 0;
	clamp_xywh_int(NULL, NULL, &width, &height);
	return real(dpy, w, width, height);
}

int XMoveResizeWindow(void *dpy, XWindow w, int x, int y, unsigned int width, unsigned int height)
{
	static int (*real)(void *, XWindow, int, int, unsigned int, unsigned int);
	if (!real)
		real = dlsym(RTLD_NEXT, "XMoveResizeWindow");
	if (!real)
		return 0;
	clamp_xywh_int(&x, &y, &width, &height);
	return real(dpy, w, x, y, width, height);
}

#define CWX (1u << 0)
#define CWY (1u << 1)
#define CWWidth (1u << 2)
#define CWHeight (1u << 3)

int XConfigureWindow(void *dpy, XWindow w, unsigned int mask, void *changes)
{
	static int (*real)(void *, XWindow, unsigned int, void *);
	struct {
		int x, y, width, height, border_width;
		XWindow sibling;
		int stack_mode;
	} copy;
	unsigned int width = 1920, height = 1080;
	int x = 0, y = 0;

	if (!real)
		real = dlsym(RTLD_NEXT, "XConfigureWindow");
	if (!real)
		return 0;
	if (!changes)
		return real(dpy, w, mask, changes);
	memcpy(&copy, changes, sizeof copy);
	if (mask & CWX)
		x = copy.x;
	if (mask & CWY)
		y = copy.y;
	if (mask & CWWidth)
		width = (unsigned int)copy.width;
	if (mask & CWHeight)
		height = (unsigned int)copy.height;
	clamp_xywh_int(&x, &y, &width, &height);
	if (mask & CWX)
		copy.x = x;
	if (mask & CWY)
		copy.y = y;
	if (mask & CWWidth)
		copy.width = (int)width;
	if (mask & CWHeight)
		copy.height = (int)height;
	return real(dpy, w, mask, &copy);
}

typedef struct xcb_connection_t xcb_connection_t;
typedef uint32_t xcb_window_t;
typedef uint32_t xcb_visualid_t;
typedef struct {
	unsigned int sequence;
} xcb_void_cookie_t;

#define XCB_CONFIG_WINDOW_WIDTH 4
#define XCB_CONFIG_WINDOW_HEIGHT 8

xcb_void_cookie_t xcb_create_window(xcb_connection_t *c, uint8_t depth, xcb_window_t wid,
    xcb_window_t parent, int16_t x, int16_t y, uint16_t width, uint16_t height,
    uint16_t border_width, uint16_t class_, xcb_visualid_t visual, uint32_t value_mask,
    const void *value_list)
{
	static xcb_void_cookie_t (*real)(xcb_connection_t *, uint8_t, xcb_window_t, xcb_window_t,
	    int16_t, int16_t, uint16_t, uint16_t, uint16_t, uint16_t, xcb_visualid_t, uint32_t,
	    const void *);
	if (!real)
		real = dlsym(RTLD_NEXT, "xcb_create_window");
	if (width == 0 || height == 0)
		slog("oath-cefgeom: xcb_create_window %dx%d -> 1920x1080\n", width, height);
	clamp_wh(&width, &height);
	return real(c, depth, wid, parent, x, y, width, height, border_width, class_, visual,
	    value_mask, value_list);
}

xcb_void_cookie_t xcb_create_window_aux(xcb_connection_t *c, uint8_t depth, xcb_window_t wid,
    xcb_window_t parent, int16_t x, int16_t y, uint16_t width, uint16_t height,
    uint16_t border_width, uint16_t class_, xcb_visualid_t visual, uint32_t value_mask,
    const void *value_list)
{
	static xcb_void_cookie_t (*real)(xcb_connection_t *, uint8_t, xcb_window_t, xcb_window_t,
	    int16_t, int16_t, uint16_t, uint16_t, uint16_t, uint16_t, xcb_visualid_t, uint32_t,
	    const void *);
	if (!real)
		real = dlsym(RTLD_NEXT, "xcb_create_window_aux");
	clamp_wh(&width, &height);
	if (!real)
		return xcb_create_window(c, depth, wid, parent, x, y, width, height, border_width,
		    class_, visual, value_mask, value_list);
	return real(c, depth, wid, parent, x, y, width, height, border_width, class_, visual,
	    value_mask, value_list);
}

xcb_void_cookie_t xcb_configure_window(xcb_connection_t *c, xcb_window_t window, uint16_t value_mask,
    const void *value_list)
{
	static xcb_void_cookie_t (*real)(xcb_connection_t *, xcb_window_t, uint16_t, const void *);
	uint32_t vals[16];
	const uint32_t *in = value_list;
	unsigned n = 0;
	uint16_t bit;

	if (!real)
		real = dlsym(RTLD_NEXT, "xcb_configure_window");
	if (!real || !value_list)
		return real ? real(c, window, value_mask, value_list) : (xcb_void_cookie_t){ 0 };
	for (bit = 1; bit; bit = (uint16_t)(bit << 1)) {
		if (!(value_mask & bit))
			continue;
		vals[n] = in[n];
		if (bit == XCB_CONFIG_WINDOW_WIDTH && vals[n] == 0)
			vals[n] = 1920;
		if (bit == XCB_CONFIG_WINDOW_HEIGHT && vals[n] == 0)
			vals[n] = 1080;
		n++;
		if (n >= 16)
			break;
	}
	return real(c, window, value_mask, vals);
}
