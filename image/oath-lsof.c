/* Minimal lsof for Steam WebUITransport.
 *
 * steamui runs:  <lsof> -P -F upnR -i TCP@<ip>:<port>
 * and parses field output (p/u/n/R). Without this, it logs
 * "Checked: 0/<webhelper>" and the 32-bit client segfaults.
 *
 * Match ESTABLISHED TCP sockets via /proc/net/tcp + /proc/<pid>/fd.
 */
#define _GNU_SOURCE
#include <ctype.h>
#include <dirent.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <arpa/inet.h>
#include <netinet/in.h>
#include <sys/stat.h>
#include <sys/types.h>

static unsigned want_ip;
static unsigned want_port; /* host order */
static int have_filter;

static int parse_tcp_at(const char *s)
{
	char ip[64];
	unsigned p = 0;
	const char *at, *col;

	if (strncmp(s, "TCP@", 4) == 0)
		s += 4;
	else if (strncmp(s, "tcp@", 4) == 0)
		s += 4;
	at = s;
	col = strrchr(at, ':');
	if (!col || col == at)
		return -1;
	if ((size_t)(col - at) >= sizeof ip)
		return -1;
	memcpy(ip, at, (size_t)(col - at));
	ip[col - at] = 0;
	p = (unsigned)atoi(col + 1);
	if (!p)
		return -1;
	if (inet_pton(AF_INET, ip, &want_ip) != 1)
		return -1;
	want_port = p;
	have_filter = 1;
	return 0;
}

static int parse_args(int argc, char **argv)
{
	int i;
	for (i = 1; i < argc; i++) {
		if (strcmp(argv[i], "-P") == 0 || strcmp(argv[i], "-n") == 0)
			continue;
		if (strncmp(argv[i], "-F", 2) == 0)
			continue;
		if (strcmp(argv[i], "-i") == 0 && i + 1 < argc) {
			if (parse_tcp_at(argv[++i]) != 0)
				return -1;
			continue;
		}
		if (strncmp(argv[i], "-i", 2) == 0) {
			if (parse_tcp_at(argv[i] + 2) != 0)
				return -1;
			continue;
		}
	}
	return 0;
}

static int addr_match(unsigned ip, unsigned port)
{
	return have_filter && ip == want_ip && port == want_port;
}

/* /proc/net/tcp: ip is native endian hex of s_addr; port is host-order hex. */
static unsigned inode_for_peer(unsigned *out_lip, unsigned *out_lport,
                               unsigned *out_rip, unsigned *out_rport)
{
	FILE *f;
	char line[512];
	unsigned lip, lport, rip, rport, st, ino;

	f = fopen("/proc/net/tcp", "r");
	if (!f)
		return 0;
	if (!fgets(line, sizeof line, f)) {
		fclose(f);
		return 0;
	}
	while (fgets(line, sizeof line, f)) {
		if (sscanf(line, "%*d: %x:%x %x:%x %x %*s %*s %*s %*d %*d %u",
		           &lip, &lport, &rip, &rport, &st, &ino) < 6)
			continue;
		if (st != 0x01) /* ESTABLISHED */
			continue;
		if (addr_match(lip, lport) || addr_match(rip, rport)) {
			*out_lip = lip;
			*out_lport = lport;
			*out_rip = rip;
			*out_rport = rport;
			fclose(f);
			return ino;
		}
	}
	fclose(f);
	return 0;
}

static unsigned ppid_of(unsigned pid)
{
	char path[64], line[128];
	FILE *f;
	unsigned ppid = 0;

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

static unsigned uid_of(unsigned pid)
{
	char path[64], line[128];
	FILE *f;
	unsigned uid = 0;

	snprintf(path, sizeof path, "/proc/%u/status", pid);
	f = fopen(path, "r");
	if (!f)
		return 0;
	while (fgets(line, sizeof line, f)) {
		if (sscanf(line, "Uid:\t%u", &uid) == 1)
			break;
	}
	fclose(f);
	return uid;
}

static int pid_has_inode(unsigned pid, unsigned inode)
{
	char path[80], target[96];
	DIR *fd;
	struct dirent *fe;

	snprintf(path, sizeof path, "/proc/%u/fd", pid);
	fd = opendir(path);
	if (!fd)
		return 0;
	while ((fe = readdir(fd))) {
		ssize_t n;
		unsigned ino2;

		if (fe->d_name[0] == '.')
			continue;
		snprintf(path, sizeof path, "/proc/%u/fd/%s", pid, fe->d_name);
		n = readlink(path, target, sizeof target - 1);
		if (n < 0)
			continue;
		target[n] = 0;
		if (sscanf(target, "socket:[%u]", &ino2) == 1 && ino2 == inode) {
			closedir(fd);
			return 1;
		}
	}
	closedir(fd);
	return 0;
}

static unsigned find_pid(unsigned inode)
{
	DIR *proc;
	struct dirent *de;
	unsigned found = 0;

	proc = opendir("/proc");
	if (!proc)
		return 0;
	while ((de = readdir(proc))) {
		char *end = 0;
		long pid;

		if (de->d_name[0] < '1' || de->d_name[0] > '9')
			continue;
		pid = strtol(de->d_name, &end, 10);
		if (!end || *end || pid <= 0)
			continue;
		if (pid_has_inode((unsigned)pid, inode)) {
			found = (unsigned)pid;
			break;
		}
	}
	closedir(proc);
	return found;
}

static void ip_str(unsigned saddr, char *buf, size_t n)
{
	struct in_addr a;
	a.s_addr = saddr;
	if (!inet_ntop(AF_INET, &a, buf, n))
		snprintf(buf, n, "0.0.0.0");
}

int main(int argc, char **argv)
{
	unsigned ino, pid, lip, lport, rip, rport;
	char lbuf[32], rbuf[32];

	if (parse_args(argc, argv) != 0 || !have_filter)
		return 1;
	ino = inode_for_peer(&lip, &lport, &rip, &rport);
	if (!ino)
		return 0;
	pid = find_pid(ino);
	if (!pid)
		return 0;
	ip_str(lip, lbuf, sizeof lbuf);
	ip_str(rip, rbuf, sizeof rbuf);
	printf("p%u\n", pid);
	printf("u%u\n", uid_of(pid));
	printf("R%u\n", ppid_of(pid));
	printf("n%s:%u->%s:%u\n", lbuf, lport, rbuf, rport);
	return 0;
}
