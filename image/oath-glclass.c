/*
 * 64-bit LD_PRELOAD. This glibc errors on wrong ELF class instead of
 * skipping, so a mixed LD_LIBRARY_PATH (ubuntu12_32 32-bit libGL first)
 * kills 64-bit Steam helpers such as ubuntu12_64/gldriverquery.
 * Redirect GL/EGL SONAMEs to pkg:mesa. 32-bit processes ignore this
 * DSO (ELFCLASS64).
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <string.h>

static void *(*real_dlopen)(const char *, int);

static const char *mesa_gl(const char *file)
{
	const char *base;

	if (!file || !file[0])
		return NULL;
	base = strrchr(file, '/');
	base = base ? base + 1 : file;
	if (strcmp(base, "libGL.so.1") == 0 || strncmp(base, "libGL.so.1.", 11) == 0)
		return "/oath/store/pkg/mesa/lib/libGL.so.1";
	if (strcmp(base, "libEGL.so.1") == 0 || strncmp(base, "libEGL.so.1.", 12) == 0)
		return "/oath/store/pkg/mesa/lib/libEGL.so.1";
	if (strcmp(base, "libGLESv2.so.2") == 0 || strncmp(base, "libGLESv2.so.2.", 15) == 0)
		return "/oath/store/pkg/mesa/lib/libGLESv2.so.2";
	if (strcmp(base, "libGLX.so.0") == 0 || strncmp(base, "libGLX.so.0.", 12) == 0)
		return "/oath/store/pkg/mesa/lib/libGLX.so.0";
	if (strcmp(base, "libGLX_mesa.so.0") == 0 || strncmp(base, "libGLX_mesa.so.0.", 17) == 0)
		return "/oath/store/pkg/mesa/lib/libGLX_mesa.so.0";
	if (strcmp(base, "libGLdispatch.so.0") == 0 || strncmp(base, "libGLdispatch.so.0.", 19) == 0)
		return "/oath/store/pkg/mesa/lib/libGLdispatch.so.0";
	if (strcmp(base, "libgbm.so.1") == 0 || strncmp(base, "libgbm.so.1.", 12) == 0)
		return "/oath/store/pkg/mesa/lib/libgbm.so.1";
	return NULL;
}

static void init(void) __attribute__((constructor));
static void init(void)
{
	real_dlopen = (void *(*)(const char *, int))dlsym(RTLD_NEXT, "dlopen");
}

void *dlopen(const char *file, int flags)
{
	const char *mesa;
	void *h;

	if (!real_dlopen)
		real_dlopen = (void *(*)(const char *, int))dlsym(RTLD_NEXT, "dlopen");
	if (!real_dlopen)
		return NULL;
	mesa = mesa_gl(file);
	if (mesa) {
		h = real_dlopen(mesa, flags);
		if (h)
			return h;
	}
	return real_dlopen(file, flags);
}
