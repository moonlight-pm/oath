/* 32-bit helper for ubuntu12_32/steam / steamui.so (dlmopen namespace:
 * process LD_PRELOAD does not apply; DT_NEEDED on steamui.so does).
 *
 * If steamui calls getsockopt(SO_PEERCRED) on the accepted TCP websocket,
 * Linux returns pid 0 for AF_INET. Fill pid from /proc/net/tcp +
 * /proc/<pid>/fd, then walk up to the steamwebhelper browser process
 * (no --type=). The current client’s “Checked: 0/<webhelper>” path does
 * not call SO_PEERCRED (it fopen()s /proc/*/stat); this hook is
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
