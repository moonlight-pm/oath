/* 32-bit helper for ubuntu12_32/steam / steamui.so (dlmopen namespace:
 * process LD_PRELOAD does not apply; DT_NEEDED on steamui.so does, and
 * it must be *first* so SDL_CreateWindow* binds here instead of libSDL3).
 *
 * If steamui calls getsockopt(SO_PEERCRED) on the accepted TCP websocket,
 * Linux returns pid 0 for AF_INET. Fill pid from /proc/net/tcp +
 * /proc/<pid>/fd, then walk up to the steamwebhelper browser process
 * (no --type=). The current client’s “Checked: 0/<webhelper>” path does
 * not call SO_PEERCRED (it fopen()s /proc/<pid>/stat); this hook is
 * best-effort for the creds case.
 *
 * Steam's library / CEF chrome is SDL3. A modal/popup with no parent
 * returns NULL ("Modal windows must specify a parent window") and
 * steamui segfaults after WaitingForLibraryReady.
 */
#define _GNU_SOURCE
#include <dirent.h>
#include <dlfcn.h>
#include <errno.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <netinet/in.h>

#ifndef SO_PEERCRED
#define SO_PEERCRED 17
#endif

#ifndef LM_ID_NEWLM
#define LM_ID_NEWLM ((long)-1)
#endif

struct ucred_t {
	unsigned int pid;
	unsigned int uid;
	unsigned int gid;
};

static int (*real_getsockopt)(int, int, int, void *, socklen_t *);

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

static unsigned int ppid_of(unsigned int pid)
{
	char path[64], line[128];
	FILE *f;
	unsigned int ppid = 0;

	snprintf(path, sizeof path, "/proc/%u/status", pid);
	f = fopen(path, "r");
	if (!f)
		return 0;
	while (fgets(line, sizeof line, f)) {
		if (sscanf(line, "PPid:\t%u", &ppid) == 1)
			break;
	}
	fclose(f);
	return ppid;
}

static int cmdline_has(unsigned int pid, const char *needle)
{
	char path[64], buf[256];
	FILE *f;
	size_t n, i;

	snprintf(path, sizeof path, "/proc/%u/cmdline", pid);
	f = fopen(path, "r");
	if (!f)
		return 0;
	n = fread(buf, 1, sizeof buf - 1, f);
	fclose(f);
	if (n == 0)
		return 0;
	for (i = 0; i < n; i++) {
		if (buf[i] == 0)
			buf[i] = ' ';
	}
	buf[n] = 0;
	return strstr(buf, needle) != NULL;
}

static unsigned int webhelper_root(unsigned int pid)
{
	unsigned int cur = pid, i;

	for (i = 0; i < 16 && cur; i++) {
		if (cmdline_has(cur, "steamwebhelper") && !cmdline_has(cur, "--type="))
			return cur;
		if (!cmdline_has(cur, "steamwebhelper"))
			break;
		cur = ppid_of(cur);
	}
	return pid;
}

static unsigned int pid_for_inode(unsigned int inode)
{
	DIR *proc;
	struct dirent *de;
	char path[80], target[96];
	unsigned int found = 0;

	proc = opendir("/proc");
	if (!proc)
		return 0;
	while ((de = readdir(proc))) {
		char *end = 0;
		long pid;
		DIR *fd;
		struct dirent *fe;

		if (de->d_name[0] < '1' || de->d_name[0] > '9')
			continue;
		pid = strtol(de->d_name, &end, 10);
		if (!end || *end || pid <= 0)
			continue;
		snprintf(path, sizeof path, "/proc/%ld/fd", pid);
		fd = opendir(path);
		if (!fd)
			continue;
		while ((fe = readdir(fd))) {
			ssize_t n;
			unsigned int ino2;

			if (fe->d_name[0] == '.')
				continue;
			snprintf(path, sizeof path, "/proc/%ld/fd/%s", pid, fe->d_name);
			n = readlink(path, target, sizeof target - 1);
			if (n < 0)
				continue;
			target[n] = 0;
			if (sscanf(target, "socket:[%u]", &ino2) == 1 && ino2 == inode) {
				found = (unsigned int)pid;
				break;
			}
		}
		closedir(fd);
		if (found)
			break;
	}
	closedir(proc);
	return found;
}

static unsigned int inode_from_proc_net(const char *file, unsigned int want_lip,
                                        unsigned int want_lport, unsigned int want_rip,
                                        unsigned int want_rport)
{
	FILE *f;
	char line[512];
	unsigned int lip, lport, rip, rport, ino = 0;

	f = fopen(file, "r");
	if (!f)
		return 0;
	if (!fgets(line, sizeof line, f)) {
		fclose(f);
		return 0;
	}
	while (fgets(line, sizeof line, f)) {
		if (sscanf(line, "%*d: %x:%x %x:%x %*x %*s %*s %*s %*d %*d %u",
		           &lip, &lport, &rip, &rport, &ino) < 5)
			continue;
		if (lip == want_lip && lport == want_lport && rip == want_rip &&
		    rport == want_rport) {
			fclose(f);
			return ino;
		}
	}
	fclose(f);
	return 0;
}

static unsigned int pid_for_tcp_fd(int s)
{
	struct sockaddr_storage loc, rem;
	socklen_t ln = sizeof loc, rn = sizeof rem;
	unsigned int ino, pid;

	memset(&loc, 0, sizeof loc);
	memset(&rem, 0, sizeof rem);
	if (getsockname(s, (struct sockaddr *)&loc, &ln) < 0)
		return 0;
	if (getpeername(s, (struct sockaddr *)&rem, &rn) < 0)
		return 0;
	if (loc.ss_family == AF_INET && rem.ss_family == AF_INET) {
		struct sockaddr_in *l = (struct sockaddr_in *)&loc;
		struct sockaddr_in *r = (struct sockaddr_in *)&rem;
		ino = inode_from_proc_net("/proc/net/tcp", l->sin_addr.s_addr,
		                          (unsigned int)ntohs(l->sin_port),
		                          r->sin_addr.s_addr,
		                          (unsigned int)ntohs(r->sin_port));
	} else {
		return 0;
	}
	if (!ino)
		return 0;
	pid = pid_for_inode(ino);
	if (!pid)
		return 0;
	return webhelper_root(pid);
}

/* Steam's library / CEF windows are created at INT_MIN with size 0x0.
 * X11 then either BadValue's or gamescope composites an empty buffer
 * (static on RADV SI). Clamp before the request hits the wire.
 */
typedef unsigned long XWindow;

static void clamp_geom(int *x, int *y, unsigned int *w, unsigned int *h)
{
	/* INT_MIN / 0x0 is the library CEF chrome. Tiny 1x1 helper
	 * windows are real — do not blow them up to 1920x1080. */
	if (x && (*x < -4096 || *x > 7680))
		*x = 0;
	if (y && (*y < -4096 || *y > 4320))
		*y = 0;
	if (w && (*w == 0 || *w > 7680))
		*w = 1920;
	if (h && (*h == 0 || *h > 4320))
		*h = 1080;
}

XWindow XCreateWindow(void *dpy, XWindow parent, int x, int y, unsigned int width,
    unsigned int height, unsigned int border_width, int depth, unsigned int class_,
    void *visual, unsigned long valuemask, void *attributes)
{
	static XWindow (*real)(void *, XWindow, int, int, unsigned int, unsigned int,
	    unsigned int, int, unsigned int, void *, unsigned long, void *);
	if (!real)
		real = (XWindow (*)(void *, XWindow, int, int, unsigned int, unsigned int,
		           unsigned int, int, unsigned int, void *, unsigned long, void *))
			dlsym(RTLD_NEXT, "XCreateWindow");
	if (x < -4096 || x > 7680 || y < -4096 || y > 4320 || width == 0 || height == 0)
		slog("oath-steam: XCreateWindow clamp %d,%d %ux%u\n", x, y, width, height);
	clamp_geom(&x, &y, &width, &height);
	return real(dpy, parent, x, y, width, height, border_width, depth, class_, visual,
	    valuemask, attributes);
}

XWindow XCreateSimpleWindow(void *dpy, XWindow parent, int x, int y, unsigned int width,
    unsigned int height, unsigned int border_width, unsigned long border,
    unsigned long background)
{
	static XWindow (*real)(void *, XWindow, int, int, unsigned int, unsigned int,
	    unsigned int, unsigned long, unsigned long);
	if (!real)
		real = (XWindow (*)(void *, XWindow, int, int, unsigned int, unsigned int,
		           unsigned int, unsigned long, unsigned long))
			dlsym(RTLD_NEXT, "XCreateSimpleWindow");
	clamp_geom(&x, &y, &width, &height);
	return real(dpy, parent, x, y, width, height, border_width, border, background);
}

int XMoveResizeWindow(void *dpy, XWindow w, int x, int y, unsigned int width,
    unsigned int height)
{
	static int (*real)(void *, XWindow, int, int, unsigned int, unsigned int);
	if (!real)
		real = (int (*)(void *, XWindow, int, int, unsigned int, unsigned int))
			dlsym(RTLD_NEXT, "XMoveResizeWindow");
	clamp_geom(&x, &y, &width, &height);
	return real(dpy, w, x, y, width, height);
}

int XResizeWindow(void *dpy, XWindow w, unsigned int width, unsigned int height)
{
	static int (*real)(void *, XWindow, unsigned int, unsigned int);
	if (!real)
		real = (int (*)(void *, XWindow, unsigned int, unsigned int))
			dlsym(RTLD_NEXT, "XResizeWindow");
	clamp_geom(NULL, NULL, &width, &height);
	return real(dpy, w, width, height);
}

int XMoveWindow(void *dpy, XWindow w, int x, int y)
{
	static int (*real)(void *, XWindow, int, int);
	if (!real)
		real = (int (*)(void *, XWindow, int, int))dlsym(RTLD_NEXT, "XMoveWindow");
	clamp_geom(&x, &y, NULL, NULL);
	return real(dpy, w, x, y);
}

#define CWX (1 << 0)
#define CWY (1 << 1)
#define CWWidth (1 << 2)
#define CWHeight (1 << 3)

int XConfigureWindow(void *dpy, XWindow w, unsigned int mask, void *changes)
{
	static int (*real)(void *, XWindow, unsigned int, void *);
	struct {
		int x, y;
		int width, height;
		int border_width;
		XWindow sibling;
		int stack_mode;
	} copy;
	int x = 0, y = 0;
	unsigned int width = 1920, height = 1080;

	if (!real)
		real = (int (*)(void *, XWindow, unsigned int, void *))
			dlsym(RTLD_NEXT, "XConfigureWindow");
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
	clamp_geom(&x, &y, &width, &height);
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

/* gamescope --steam publishes these on the nested root. Rootless Xwayland
 * plus a late set means steamui often sees None and then enables HDR on SI.
 */
static unsigned long atom_viewport;
static unsigned long atom_hdr;
static unsigned long atom_vroverlay;

unsigned long XInternAtom(void *dpy, const char *name, int only_if_exists)
{
	static unsigned long (*real)(void *, const char *, int);
	unsigned long a;

	if (!real)
		real = (unsigned long (*)(void *, const char *, int))
			dlsym(RTLD_NEXT, "XInternAtom");
	a = real(dpy, name, only_if_exists);
	if (name && a) {
		if (!atom_viewport && strcmp(name, "GAMESCOPE_VIEWPORT_SUPPORTED") == 0)
			atom_viewport = a;
		else if (!atom_hdr && strcmp(name, "GAMESCOPE_HDR_ENABLED") == 0)
			atom_hdr = a;
		else if (!atom_vroverlay && strcmp(name, "GAMESCOPE_VROVERLAY_FORWARDING") == 0)
			atom_vroverlay = a;
	}
	return a;
}

static int fake_cardinal(unsigned int val, unsigned long *actual_type, int *actual_format,
    unsigned long *nitems, unsigned long *bytes_after, unsigned char **prop)
{
	unsigned int *p = malloc(sizeof *p);

	if (!p)
		return 1;
	*p = val;
	if (actual_type)
		*actual_type = 6; /* XA_CARDINAL */
	if (actual_format)
		*actual_format = 32;
	if (nitems)
		*nitems = 1;
	if (bytes_after)
		*bytes_after = 0;
	if (prop)
		*prop = (unsigned char *)p;
	return 0;
}

int XGetWindowProperty(void *dpy, unsigned long w, unsigned long property, long offset,
    long length, int del, unsigned long req_type, unsigned long *actual_type,
    int *actual_format, unsigned long *nitems, unsigned long *bytes_after,
    unsigned char **prop)
{
	static int (*real)(void *, unsigned long, unsigned long, long, long, int,
	    unsigned long, unsigned long *, int *, unsigned long *, unsigned long *,
	    unsigned char **);

	if (!real)
		real = (int (*)(void *, unsigned long, unsigned long, long, long, int,
		           unsigned long, unsigned long *, int *, unsigned long *,
		           unsigned long *, unsigned char **))
			dlsym(RTLD_NEXT, "XGetWindowProperty");
	if (property && property == atom_viewport)
		return fake_cardinal(1, actual_type, actual_format, nitems, bytes_after, prop);
	if (property && property == atom_hdr)
		return fake_cardinal(0, actual_type, actual_format, nitems, bytes_after, prop);
	if (property && property == atom_vroverlay)
		return fake_cardinal(0, actual_type, actual_format, nitems, bytes_after, prop);
	return real(dpy, w, property, offset, length, del, req_type, actual_type,
	    actual_format, nitems, bytes_after, prop);
}

/* SDL3. Flags are Uint64 — unsigned long is 32-bit here and would truncate. */
#define SDL_WINDOW_MODAL 0x0000000000001000ull
#define SDL_WINDOW_UTILITY 0x0000000000020000ull
#define SDL_WINDOW_TOOLTIP 0x0000000000040000ull
#define SDL_WINDOW_POPUP_MENU 0x0000000000080000ull
#define SDL_WINDOW_HIDDEN 0x0000000000000008ull

#define PROP_W "SDL.window.create.width"
#define PROP_H "SDL.window.create.height"
#define PROP_X "SDL.window.create.x"
#define PROP_Y "SDL.window.create.y"
#define PROP_MODAL "SDL.window.create.modal"
#define PROP_PARENT "SDL.window.create.parent"
#define PROP_MENU "SDL.window.create.menu"
#define PROP_TOOLTIP "SDL.window.create.tooltip"
#define PROP_FLAGS "SDL.window.create.flags"

static void *last_window;

static int sdl_pos_magic(long long v)
{
	unsigned u = (unsigned)v;

	return (u & 0xFFFF0000u) == 0x1FFF0000u || (u & 0xFFFF0000u) == 0x2FFF0000u;
}

/*
 * i386 SysV puts Uint64 on the stack as two uint32 words. zig cc's
 * uint64_t ABI does not match libSDL3 (flags became 0x2000000020 and
 * dropped SDL_WINDOW_OPENGL → "window isn't an OpenGL window"). Split
 * the words so we both receive and forward the gcc layout.
 */
void *SDL_CreateWindow(const char *title, int w, int h, uint32_t flags_lo,
    uint32_t flags_hi)
{
	static void *(*real)(const char *, int, int, uint32_t, uint32_t);
	static const char *(*geterr)(void);
	void *win;
	uint32_t lo = flags_lo, hi = flags_hi;

	if (!real)
		real = (void *(*)(const char *, int, int, uint32_t, uint32_t))
			dlsym(RTLD_NEXT, "SDL_CreateWindow");
	if (!geterr)
		geterr = (const char *(*)(void))dlsym(RTLD_NEXT, "SDL_GetError");
	if (w == 0)
		w = 1920;
	if (h == 0)
		h = 1080;
	lo &= ~(uint32_t)(SDL_WINDOW_MODAL | SDL_WINDOW_TOOLTIP | SDL_WINDOW_POPUP_MENU);
	if (title && strstr(title, "OpenGL"))
		lo |= 2u; /* SDL_WINDOW_OPENGL */
	slog("oath-steam: SDL_CreateWindow title=%s w=%d h=%d flags=%08x:%08x\n",
	    title ? title : "", w, h, hi, lo);
	win = real(title, w, h, lo, hi);
	if (win)
		last_window = win;
	else
		slog("oath-steam: SDL_CreateWindow failed: %s\n", geterr ? geterr() : "?");
	return win;
}

int SDL_SetWindowSize(void *window, int w, int h)
{
	static int (*real)(void *, int, int);
	if (!real)
		real = (int (*)(void *, int, int))dlsym(RTLD_NEXT, "SDL_SetWindowSize");
	if (w == 0)
		w = 1920;
	if (h == 0)
		h = 1080;
	return real(window, w, h);
}

int SDL_SetWindowPosition(void *window, int x, int y)
{
	static int (*real)(void *, int, int);
	if (!real)
		real = (int (*)(void *, int, int))dlsym(RTLD_NEXT, "SDL_SetWindowPosition");
	if (!sdl_pos_magic(x) && (x < -4096 || x > 7680))
		x = 0;
	if (!sdl_pos_magic(y) && (y < -4096 || y > 4320))
		y = 0;
	return real(window, x, y);
}

int SDL_SetWindowModal(void *window, int modal)
{
	static int (*real)(void *, int);
	if (!real)
		real = (int (*)(void *, int))dlsym(RTLD_NEXT, "SDL_SetWindowModal");
	if (modal && window && !last_window)
		last_window = window;
	slog("oath-steam: SDL_SetWindowModal %p modal=%d\n", window, modal);
	if (real)
		return real(window, 0);
	return 1;
}

int SDL_SetWindowParent(void *window, void *parent)
{
	static int (*real)(void *, void *);
	if (!real)
		real = (int (*)(void *, void *))dlsym(RTLD_NEXT, "SDL_SetWindowParent");
	if (!parent)
		parent = last_window;
	slog("oath-steam: SDL_SetWindowParent %p parent=%p\n", window, parent);
	if (real)
		return real(window, parent);
	return 1;
}

void *SDL_CreatePopupWindow(void *parent, int x, int y, int w, int h,
    uint32_t flags_lo, uint32_t flags_hi)
{
	static void *(*real)(void *, int, int, int, int, uint32_t, uint32_t);
	void *win;

	if (!real)
		real = (void *(*)(void *, int, int, int, int, uint32_t, uint32_t))
			dlsym(RTLD_NEXT, "SDL_CreatePopupWindow");
	if (!parent)
		parent = last_window;
	if (w == 0)
		w = 1920;
	if (h == 0)
		h = 1080;
	if (!parent) {
		slog("oath-steam: SDL_CreatePopupWindow no parent; demote to window\n");
		flags_lo &= ~(uint32_t)(SDL_WINDOW_MODAL | SDL_WINDOW_TOOLTIP |
		    SDL_WINDOW_POPUP_MENU);
		return SDL_CreateWindow("oath-popup", w, h, flags_lo, flags_hi);
	}
	win = real(parent, x, y, w, h, flags_lo, flags_hi);
	if (win)
		last_window = win;
	return win;
}

void *SDL_CreateWindowWithProperties(unsigned int props)
{
	static void *(*real)(unsigned int);
	static long long (*getnum)(unsigned int, const char *, long long);
	static int (*getbool)(unsigned int, const char *, int);
	static void *(*getptr)(unsigned int, const char *, void *);
	static int (*setnum)(unsigned int, const char *, long long);
	static int (*setbool)(unsigned int, const char *, int);
	static int (*setptr)(unsigned int, const char *, void *);
	static const char *(*geterr)(void);
	void *w;
	void *parent;
	long long x = 0, y = 0, ww = 0, hh = 0, flags = 0;
	int modal = 0, menu = 0, tooltip = 0;

	if (!real)
		real = (void *(*)(unsigned int))dlsym(RTLD_NEXT, "SDL_CreateWindowWithProperties");
	if (!getnum)
		getnum = (long long (*)(unsigned int, const char *, long long))
			dlsym(RTLD_NEXT, "SDL_GetNumberProperty");
	if (!getbool)
		getbool = (int (*)(unsigned int, const char *, int))
			dlsym(RTLD_NEXT, "SDL_GetBooleanProperty");
	if (!getptr)
		getptr = (void *(*)(unsigned int, const char *, void *))
			dlsym(RTLD_NEXT, "SDL_GetPointerProperty");
	if (!setnum)
		setnum = (int (*)(unsigned int, const char *, long long))
			dlsym(RTLD_NEXT, "SDL_SetNumberProperty");
	if (!setbool)
		setbool = (int (*)(unsigned int, const char *, int))
			dlsym(RTLD_NEXT, "SDL_SetBooleanProperty");
	if (!setptr)
		setptr = (int (*)(unsigned int, const char *, void *))
			dlsym(RTLD_NEXT, "SDL_SetPointerProperty");
	if (!geterr)
		geterr = (const char *(*)(void))dlsym(RTLD_NEXT, "SDL_GetError");

	if (getnum) {
		ww = getnum(props, PROP_W, 0);
		hh = getnum(props, PROP_H, 0);
		x = getnum(props, PROP_X, 0);
		y = getnum(props, PROP_Y, 0);
		flags = getnum(props, PROP_FLAGS, 0);
	}
	if (getbool) {
		modal = getbool(props, PROP_MODAL, 0);
		menu = getbool(props, PROP_MENU, 0);
		tooltip = getbool(props, PROP_TOOLTIP, 0);
	}
	parent = getptr ? getptr(props, PROP_PARENT, NULL) : NULL;
	if (flags & (long long)SDL_WINDOW_MODAL)
		modal = 1;
	if (flags & (long long)SDL_WINDOW_POPUP_MENU)
		menu = 1;
	if (flags & (long long)SDL_WINDOW_TOOLTIP)
		tooltip = 1;

	slog("oath-steam: CreateWindowWithProperties x=%lld y=%lld w=%lld h=%lld modal=%d menu=%d tooltip=%d parent=%p flags=0x%llx\n",
	    x, y, ww, hh, modal, menu, tooltip, parent, (unsigned long long)flags);

	if (setnum) {
		if (ww == 0 || ww > 7680)
			setnum(props, PROP_W, 1920);
		if (hh == 0 || hh > 4320)
			setnum(props, PROP_H, 1080);
		if (!sdl_pos_magic(x) && (x < -64 || x > 7680))
			setnum(props, PROP_X, 0);
		if (!sdl_pos_magic(y) && (y < -64 || y > 4320))
			setnum(props, PROP_Y, 0);
		if (flags & (long long)(SDL_WINDOW_MODAL | SDL_WINDOW_TOOLTIP | SDL_WINDOW_POPUP_MENU)) {
			flags &= ~(long long)(SDL_WINDOW_MODAL | SDL_WINDOW_TOOLTIP | SDL_WINDOW_POPUP_MENU);
			setnum(props, PROP_FLAGS, flags);
		}
	}

	if ((modal || menu || tooltip) && !parent) {
		if (last_window && setptr) {
			slog("oath-steam: using last_window %p as parent\n", last_window);
			setptr(props, PROP_PARENT, last_window);
			parent = last_window;
		} else if (setbool) {
			slog("oath-steam: clearing modal/menu/tooltip (no parent yet)\n");
			setbool(props, PROP_MODAL, 0);
			setbool(props, PROP_MENU, 0);
			setbool(props, PROP_TOOLTIP, 0);
		}
	}

	w = real(props);
	if (!w)
		slog("oath-steam: CreateWindowWithProperties failed: %s\n",
		    geterr ? geterr() : "?");
	else
		last_window = w;
	return w;
}

/* Do not patchelf the live steamui.so: Steam verifies size/checksum and
 * re-extracts forever. Load a patched copy from /tmp instead.
 */
static const char *prepare_steamui(const char *filename)
{
	static char dst[] = "/home/.local/share/Steam/ubuntu12_32/steamui.oath.so";
	static time_t last_mtime;
	struct stat st;
	char src[320], cmd[768];
	const char *so = "/oath/store/pkg/steam/lib32/liboath-peercred.so";
	const char *link = "/home/.local/share/Steam/ubuntu12_32/liboath-peercred.so";

	if (!filename || !strstr(filename, "steamui"))
		return filename;
	if (filename[0] == '/')
		snprintf(src, sizeof src, "%s", filename);
	else
		snprintf(src, sizeof src,
		    "/home/.local/share/Steam/ubuntu12_32/%s", filename);
	if (stat(src, &st) != 0)
		return filename;
	if (access(so, R_OK) == 0)
		symlink(so, link);
	if (last_mtime == st.st_mtime && access(dst, R_OK) == 0)
		return dst;
	if (access("/bin/patchelf", X_OK) != 0 || access("/bin/cp", X_OK) != 0)
		return filename;
	snprintf(cmd, sizeof cmd,
	    "LD_LIBRARY_PATH= LD_PRELOAD= /bin/cp -f '%s' '%s' && "
	    "LD_LIBRARY_PATH= LD_PRELOAD= /bin/patchelf --add-needed liboath-peercred.so '%s'",
	    src, dst, dst);
	if (system(cmd) != 0) {
		slog("oath-steam: failed to build patched copy of %s\n", src);
		return filename;
	}
	last_mtime = st.st_mtime;
	slog("oath-steam: dlmopen copy %s -> %s (size=%ld)\n", src, dst, (long)st.st_size);
	return dst;
}

void *dlmopen(long lmid, const char *filename, int flags)
{
	static void *(*real)(long, const char *, int);

	if (!real)
		real = (void *(*)(long, const char *, int))dlsym(RTLD_NEXT, "dlmopen");
	if (filename)
		filename = prepare_steamui(filename);
	return real(lmid, filename, flags);
}

void *dlopen(const char *filename, int flags)
{
	static void *(*real)(const char *, int);

	if (!real)
		real = (void *(*)(const char *, int))dlsym(RTLD_NEXT, "dlopen");
	if (filename)
		filename = prepare_steamui(filename);
	return real(filename, flags);
}

static void __attribute__((constructor)) oath_peercred_init(void)
{
	const char *so = "/oath/store/pkg/steam/lib32/liboath-peercred.so";
	const char *link = "/home/.local/share/Steam/ubuntu12_32/liboath-peercred.so";

	slog("oath-steam-peercred: loaded (X11/SDL clamp) pid=%d\n", (int)getpid());
	if (access(so, R_OK) == 0)
		symlink(so, link);
}

int getsockopt(int fd, int level, int optname, void *optval, socklen_t *optlen)
{
	int r;
	struct ucred_t *u;
	unsigned int p;

	if (!real_getsockopt)
		real_getsockopt = (int (*)(int, int, int, void *, socklen_t *))dlsym(
			RTLD_NEXT, "getsockopt");
	r = real_getsockopt(fd, level, optname, optval, optlen);
	if (r != 0 || level != SOL_SOCKET || optname != SO_PEERCRED || !optval ||
	    !optlen)
		return r;
	if (*optlen < sizeof(struct ucred_t))
		return r;
	u = (struct ucred_t *)optval;
	if (u->pid != 0)
		return r;
	p = pid_for_tcp_fd(fd);
	if (p)
		u->pid = p;
	return r;
}
