/* 64-bit LD_PRELOAD for steamwebhelper.
 *
 * Chromium sets PR_SET_DUMPABLE 0. Then steamui cannot scan
 * /proc/<pid>/fd for the websocket inode and logs
 * "Checked: 0/<webhelper>" / Connection rejected.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdarg.h>
#include <sys/prctl.h>
#include <unistd.h>

static int (*real_prctl)(int, unsigned long, unsigned long, unsigned long, unsigned long);

static void init(void) __attribute__((constructor));
static void init(void)
{
	real_prctl = (int (*)(int, unsigned long, unsigned long, unsigned long, unsigned long))dlsym(
		RTLD_NEXT, "prctl");
	if (real_prctl)
		real_prctl(PR_SET_DUMPABLE, 1, 0, 0, 0);
}

int prctl(int option, ...)
{
	unsigned long a2 = 0, a3 = 0, a4 = 0, a5 = 0;
	va_list ap;

	va_start(ap, option);
	a2 = va_arg(ap, unsigned long);
	a3 = va_arg(ap, unsigned long);
	a4 = va_arg(ap, unsigned long);
	a5 = va_arg(ap, unsigned long);
	va_end(ap);
	if (!real_prctl)
		real_prctl = (int (*)(int, unsigned long, unsigned long, unsigned long, unsigned long))dlsym(
			RTLD_NEXT, "prctl");
	if (option == PR_SET_DUMPABLE)
		a2 = 1;
	return real_prctl(option, a2, a3, a4, a5);
}
