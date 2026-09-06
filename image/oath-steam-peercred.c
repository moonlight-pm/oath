/* 32-bit helper for ubuntu12_32/steam / steamui.so (dlmopen namespace:
 * process LD_PRELOAD does not apply; DT_NEEDED on steamui.so does).
 *
 * If steamui calls getsockopt(SO_PEERCRED) on the accepted TCP websocket,
 * Linux returns pid 0 for AF_INET. Fill pid from /proc/net/tcp +
 * /proc/<pid>/fd, then walk up to the steamwebhelper browser process
 * (no --type=). The current client’s “Checked: 0/<webhelper>” path does
 * not call SO_PEERCRED (it fopen()s /proc/<pid>/stat); this hook is
 * best-effort for the creds case.
 */
#define _GNU_SOURCE
#include <dirent.h>
#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>

#ifndef SO_PEERCRED
#define SO_PEERCRED 17
#endif

struct ucred_t {
	unsigned int pid;
	unsigned int uid;
	unsigned int gid;
};

static int (*real_getsockopt)(int, int, int, void *, socklen_t *);

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
	if (x && (*x < -64 || *x > 7680))
		*x = 0;
	if (y && (*y < -64 || *y > 4320))
		*y = 0;
	if (w && (*w < 64 || *w > 7680))
		*w = 1920;
	if (h && (*h < 64 || *h > 4320))
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

/* steamui.so is SDL3. Zero-size CreateWindow is the library chrome. */
void *SDL_CreateWindow(const char *title, int w, int h, unsigned long flags)
{
	static void *(*real)(const char *, int, int, unsigned long);
	if (!real)
		real = (void *(*)(const char *, int, int, unsigned long))
			dlsym(RTLD_NEXT, "SDL_CreateWindow");
	if (w < 64)
		w = 1920;
	if (h < 64)
		h = 1080;
	return real(title, w, h, flags);
}

int SDL_SetWindowSize(void *window, int w, int h)
{
	static int (*real)(void *, int, int);
	if (!real)
		real = (int (*)(void *, int, int))dlsym(RTLD_NEXT, "SDL_SetWindowSize");
	if (w < 64)
		w = 1920;
	if (h < 64)
		h = 1080;
	return real(window, w, h);
}

int SDL_SetWindowPosition(void *window, int x, int y)
{
	static int (*real)(void *, int, int);
	if (!real)
		real = (int (*)(void *, int, int))dlsym(RTLD_NEXT, "SDL_SetWindowPosition");
	if (x < -64 || x > 7680)
		x = 0;
	if (y < -64 || y > 4320)
		y = 0;
	return real(window, x, y);
}

static void __attribute__((constructor)) oath_peercred_init(void)
{
	fprintf(stderr, "oath-steam-peercred: loaded (X11/SDL clamp)\n");
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
