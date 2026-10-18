// SPDX-FileCopyrightText: © 2020 The River Developers
// SPDX-License-Identifier: GPL-3.0-only
// Ported to C from river/main.zig for Oath, 2026-10-18.
//
// common/flags.zig is not ported: main() parses the six options by hand with
// the same single-dash spellings, the same matching rules and the same usage text.

#include <fcntl.h>
#include <limits.h>
#include <signal.h>
#include <stdarg.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

#include "rivulet.h"

extern char **environ;

static const char usage[] =
	"usage: rivulet [options]\n"
	"\n"
	"  -h                 Print this help message and exit.\n"
	"  -version           Print the version number and exit.\n"
	"  -c <command>       Run `sh -c <command>` on startup instead of the default init executable.\n"
	"  -log-level <level> Set the log level to error, warning, info, or debug.\n"
	"  -no-xwayland       Disable xwayland even if built with support.\n";

#if WLR_HAS_XWAYLAND
#define FULL_VERSION RIVULET_VERSION " +xwayland"
#else
#define FULL_VERSION RIVULET_VERSION " -xwayland"
#endif

struct rv_server server;

// std.process.Init.Minimal environ: the environment block rivulet was started with.
static char **initial_environ;

// ---------------------------------------------------------------------------
// Logging (logFn, LogScope, log_scopes, runtime_log_level)
// ---------------------------------------------------------------------------

// LogScope tag names, in enum rv_log_scope order.
static const char *const scope_names[RV_SCOPE_COUNT] = {
	"default",
	"wlroots",
	"output",
	"input",
	"lock",
	"wm",
	"xdg",
	"xwayland",
};

// std.log.Level.asText(), in enum rv_log_level order.
static const char *const level_names[] = {
	"error",
	"warning",
	"info",
	"debug",
};

static bool log_scopes[RV_SCOPE_COUNT] = { true, true, true, true, true, true, true, true };

// Set the default log level based on the build mode (ReleaseSafe: info).
static enum rv_log_level runtime_log_level = RV_LOG_INFO;

void rv_log(enum rv_log_level level, enum rv_log_scope scope, const char *fmt, ...) {
	if (level > runtime_log_level) {
		return;
	}

	if (scope != RV_SCOPE_DEFAULT && !log_scopes[scope]) {
		return;
	}

	// level.asText() ++ scope_prefix ++ format ++ "\n", written in one go.
	char prefix[32];
	int prefix_len;
	if (scope == RV_SCOPE_DEFAULT) {
		prefix_len = snprintf(prefix, sizeof(prefix), "%s: ", level_names[level]);
	} else {
		prefix_len = snprintf(prefix, sizeof(prefix), "%s(%s): ", level_names[level], scope_names[scope]);
	}
	if (prefix_len < 0) {
		return;
	}

	va_list args;
	va_start(args, fmt);
	va_list args_copy;
	va_copy(args_copy, args);

	char buffer[1024];
	char *line = buffer;
	char *allocated = NULL;
	const int msg_len = vsnprintf(buffer + prefix_len, sizeof(buffer) - (size_t)prefix_len, fmt, args);
	va_end(args);
	if (msg_len < 0) {
		va_end(args_copy);
		return;
	}
	const size_t line_len = (size_t)prefix_len + (size_t)msg_len + 1;
	if (line_len + 1 > sizeof(buffer)) {
		allocated = malloc(line_len + 1);
		if (allocated == NULL) {
			va_end(args_copy);
			return;
		}
		vsnprintf(allocated + prefix_len, (size_t)msg_len + 1, fmt, args_copy);
		line = allocated;
	}
	va_end(args_copy);
	memcpy(line, prefix, (size_t)prefix_len);
	line[line_len - 1] = '\n';

	fwrite(line, 1, line_len, stderr);
	fflush(stderr);
	free(allocated);
}

// See wlroots_log_wrapper.c
void rv_wlroots_log_callback(enum wlr_log_importance importance, const char *ptr, size_t len) {
	switch (importance) {
	case WLR_ERROR:
		rv_err(RV_SCOPE_WLROOTS, "%.*s", (int)len, ptr);
		break;
	case WLR_INFO:
		rv_info(RV_SCOPE_WLROOTS, "%.*s", (int)len, ptr);
		break;
	case WLR_DEBUG:
		rv_debug(RV_SCOPE_WLROOTS, "%.*s", (int)len, ptr);
		break;
	case WLR_SILENT:
	case WLR_LOG_IMPORTANCE_LAST:
		abort(); // unreachable
	}
}

// ---------------------------------------------------------------------------
// Helpers for the std library behaviour main.zig relies on
// ---------------------------------------------------------------------------

// std.process.Environ.getPosix(): the first entry of the initial environment
// block whose key matches.
static const char *env_get(const char *key) {
	const size_t key_len = strlen(key);
	for (char **entry = initial_environ; entry != NULL && *entry != NULL; entry++) {
		if (strncmp(*entry, key, key_len) == 0 && (*entry)[key_len] == '=') {
			return *entry + key_len + 1;
		}
	}
	return NULL;
}

// std.fs.path.joinZ() of two paths where `b` is non-empty and does not start
// with a separator: an empty `a` is skipped and no separator is doubled.
static char *path_join(const char *a, const char *b) {
	const size_t a_len = strlen(a);
	const size_t b_len = strlen(b);
	const bool add_sep = a_len > 0 && a[a_len - 1] != '/';
	char *path = malloc(a_len + add_sep + b_len + 1);
	if (path == NULL) {
		return NULL;
	}
	memcpy(path, a, a_len);
	if (add_sep) {
		path[a_len] = '/';
	}
	memcpy(path + a_len + add_sep, b, b_len + 1);
	return path;
}

// Io.Dir.access() error names (Zig 0.16 maps EACCES to AccessDenied and EPERM
// to PermissionDenied).
static const char *access_error_name(int err) {
	switch (err) {
	case EACCES:
		return "AccessDenied";
	case EPERM:
		return "PermissionDenied";
	case EROFS:
		return "ReadOnlyFileSystem";
	case ELOOP:
		return "SymLinkLoop";
	case ETXTBSY:
		return "FileBusy";
	case ENOTDIR:
	case ENOENT:
		return "FileNotFound";
	case ENAMETOOLONG:
		return "NameTooLong";
	case EIO:
		return "InputOutput";
	case ENOMEM:
		return "SystemResources";
	case EILSEQ:
		return "BadPathName";
	default:
		return "Unexpected";
	}
}

// Io.Dir.access(): returns 0 or the errno.
static int dir_access(const char *path, int mode) {
	// pathToPosix() needs room for the terminating NUL in a PATH_MAX buffer.
	if (strlen(path) >= PATH_MAX) {
		return ENAMETOOLONG;
	}
	while (true) {
		if (faccessat(AT_FDCWD, path, mode, 0) == 0) {
			return 0;
		}
		if (errno != EINTR) {
			return errno;
		}
	}
}

// Io.Dir.openFile() error names.
static const char *open_error_name(int err) {
	switch (err) {
	case EINVAL:
	case EILSEQ:
		return "BadPathName";
	case EACCES:
		return "AccessDenied";
	case EFBIG:
	case EOVERFLOW:
		return "FileTooBig";
	case EISDIR:
		return "IsDir";
	case ELOOP:
		return "SymLinkLoop";
	case EMFILE:
		return "ProcessFdQuotaExceeded";
	case ENAMETOOLONG:
		return "NameTooLong";
	case ENFILE:
		return "SystemFdQuotaExceeded";
	case ENODEV:
	case ENXIO:
		return "NoDevice";
	case ENOENT:
	case ESRCH:
		return "FileNotFound";
	case ENOMEM:
		return "SystemResources";
	case ENOSPC:
		return "NoSpaceLeft";
	case ENOTDIR:
		return "NotDir";
	case EPERM:
		return "PermissionDenied";
	case EEXIST:
		return "PathAlreadyExists";
	case EBUSY:
		return "DeviceBusy";
	case EAGAIN:
		return "WouldBlock";
	case ETXTBSY:
		return "FileBusy";
	case EROFS:
		return "ReadOnlyFileSystem";
	default:
		return "Unexpected";
	}
}

// ---------------------------------------------------------------------------
// main.zig
// ---------------------------------------------------------------------------

// Zig returns !?[:0]const u8: returns the path, NULL for null (with *oom
// false) or NULL with *oom true for error.OutOfMemory.
static char *default_init_path(bool *oom) {
	*oom = false;
	char *path;
	const char *xdg_config_home = env_get("XDG_CONFIG_HOME");
	const char *home = env_get("HOME");
	if (xdg_config_home != NULL) {
		path = path_join(xdg_config_home, "rivulet/init");
	} else if (home != NULL) {
		path = path_join(home, ".config/rivulet/init");
	} else {
		return NULL;
	}
	if (path == NULL) {
		*oom = true;
		return NULL;
	}

	const int err = dir_access(path, X_OK);
	if (err != 0) {
		if (err == EPERM) {
			if (dir_access(path, F_OK) == 0) {
				rv_err(RV_SCOPE_DEFAULT, "failed to run init executable %s: the file is not executable", path);
				exit(1);
			}
		}
		rv_err(RV_SCOPE_DEFAULT, "failed to run init executable %s: %s", path, access_error_name(err));
		free(path);
		return NULL;
	}

	return path;
}

// Returns 1 if the file contains "riverctl", 0 if not, and -1 with *error_name
// set if the file could not be opened or read.
static int grep_riverctl(const char *path, const char **error_name) {
	if (strlen(path) >= PATH_MAX) {
		*error_name = "NameTooLong";
		return -1;
	}
	int fd;
	while (true) {
		fd = open(path, O_RDONLY | O_CLOEXEC | O_NOCTTY);
		if (fd >= 0) {
			break;
		}
		if (errno != EINTR) {
			*error_name = open_error_name(errno);
			return -1;
		}
	}
	struct stat st;
	if (fstat(fd, &st) == 0 && S_ISDIR(st.st_mode)) {
		close(fd);
		*error_name = "IsDir";
		return -1;
	}

	static const char needle[] = "riverctl";
	const size_t needle_len = sizeof(needle) - 1;
	// The last needle_len - 1 bytes of the previous read are kept in front of
	// the next one so that a match across two reads is found.
	char buffer[1024 + sizeof(needle) - 1];
	size_t kept = 0;
	int result = 0;
	while (true) {
		const ssize_t n = read(fd, buffer + kept, sizeof(buffer) - kept);
		if (n < 0) {
			if (errno == EINTR) {
				continue;
			}
			*error_name = "ReadFailed";
			result = -1;
			break;
		}
		if (n == 0) {
			break;
		}
		const size_t len = kept + (size_t)n;
		for (size_t i = 0; i + needle_len <= len; i++) {
			if (memcmp(buffer + i, needle, needle_len) == 0) {
				result = 1;
				break;
			}
		}
		if (result == 1) {
			break;
		}
		kept = len < needle_len - 1 ? len : needle_len - 1;
		memmove(buffer, buffer + len - kept, kept);
	}
	close(fd);
	return result;
}

static void detect_classic(const char *startup_command) {
	const char *path = startup_command;
	if (path == NULL) {
		return;
	}
	if (strchr(path, '/') == NULL) {
		return;
	}

	const char *error_name = NULL;
	const int classic = grep_riverctl(path, &error_name);
	if (classic < 0) {
		rv_debug(RV_SCOPE_DEFAULT, "failed to detect riverctl usage in init file: %s", error_name);
		return;
	}
	if (classic) {
		fprintf(stderr,
			"The init file %s contains the string \"riverctl\".\n"
			"rivulet %s does not support riverctl, you may wish to\n"
			"install river-classic instead.\n"
			"\n"
			"rivulet is River 0.4.5 ported to C: a non-monolithic Wayland\n"
			"compositor that requires a compatible window manager to be useful.\n"
			"See https://isaacfreund.com/software/river for more information.\n",
			path, RIVULET_VERSION);
		fflush(stderr);
		exit(1);
	}
}

// zig-wayland's addSocketAuto(): wayland-client will connect to wayland-0 even
// if WAYLAND_DISPLAY is unset, so never call the socket wayland-0 and try
// wayland-1 through wayland-32 instead of wl_display_add_socket_auto().
static const char *add_socket_auto(struct wl_display *wl_server, char buf[static 11]) {
	for (uint32_t i = 1; i <= 32; i++) {
		snprintf(buf, 11, "wayland-%u", i);
		if (wl_display_add_socket(wl_server, buf) == -1) {
			continue;
		}
		return buf;
	}
	return NULL;
}

// One entry of std.process.Environ.Map (an array hash map: insertion order,
// put() on an existing key replaces the value in place).
struct env_entry {
	char *key;
	char *value;
};

struct env_map {
	RV_ARRAY(struct env_entry) entries;
};

static void env_map_deinit(struct env_map *map) {
	for (size_t i = 0; i < map->entries.len; i++) {
		free(map->entries.items[i].key);
		free(map->entries.items[i].value);
	}
	rv_array_finish(&map->entries);
}

static bool env_map_put(struct env_map *map, const char *key, size_t key_len, const char *value) {
	char *value_copy = strdup(value);
	if (value_copy == NULL) {
		return false;
	}
	for (size_t i = 0; i < map->entries.len; i++) {
		struct env_entry *entry = &map->entries.items[i];
		if (strlen(entry->key) == key_len && strncmp(entry->key, key, key_len) == 0) {
			free(entry->value);
			entry->value = value_copy;
			return true;
		}
	}
	char *key_copy = strndup(key, key_len);
	if (key_copy == NULL) {
		free(value_copy);
		return false;
	}
	struct env_entry entry = { .key = key_copy, .value = value_copy };
	if (!rv_array_append(&map->entries, entry)) {
		free(key_copy);
		free(value_copy);
		return false;
	}
	return true;
}

// Environ.createMap(): every KEY=VALUE entry of the initial environment block.
static bool env_map_create(struct env_map *map) {
	*map = (struct env_map){ 0 };
	for (char **entry = initial_environ; entry != NULL && *entry != NULL; entry++) {
		const char *equals = strchr(*entry, '=');
		const size_t key_len = equals != NULL ? (size_t)(equals - *entry) : strlen(*entry);
		const char *value = equals != NULL ? equals + 1 : "";
		if (!env_map_put(map, *entry, key_len, value)) {
			env_map_deinit(map);
			return false;
		}
	}
	return true;
}

static void env_block_free(char **env_block) {
	if (env_block == NULL) {
		return;
	}
	for (char **entry = env_block; *entry != NULL; entry++) {
		free(*entry);
	}
	free(env_block);
}

// Map.createPosixBlock(): NULL-terminated array of "KEY=VALUE" strings in map order.
static char **env_map_create_posix_block(const struct env_map *map) {
	char **env_block = calloc(map->entries.len + 1, sizeof(char *));
	if (env_block == NULL) {
		return NULL;
	}
	for (size_t i = 0; i < map->entries.len; i++) {
		const struct env_entry *entry = &map->entries.items[i];
		const size_t key_len = strlen(entry->key);
		const size_t value_len = strlen(entry->value);
		char *str = malloc(key_len + 1 + value_len + 1);
		if (str == NULL) {
			env_block_free(env_block);
			return NULL;
		}
		memcpy(str, entry->key, key_len);
		str[key_len] = '=';
		memcpy(str + key_len + 1, entry->value, value_len + 1);
		env_block[i] = str;
	}
	return env_block;
}

// The environment for the init command: river's own plus WAYLAND_DISPLAY and,
// with Xwayland, DISPLAY. NULL on error.OutOfMemory.
static char **create_child_env(const char *socket) {
	struct env_map env_map;
	if (!env_map_create(&env_map)) {
		return NULL;
	}

	if (!env_map_put(&env_map, "WAYLAND_DISPLAY", strlen("WAYLAND_DISPLAY"), socket)) {
		env_map_deinit(&env_map);
		return NULL;
	}

#if WLR_HAS_XWAYLAND
	if (server.xwayland != NULL) {
		struct wlr_xwayland *xwayland = server.xwayland;
		if (!env_map_put(&env_map, "DISPLAY", strlen("DISPLAY"), xwayland->display_name)) {
			env_map_deinit(&env_map);
			return NULL;
		}
	}
#endif

	char **env_block = env_map_create_posix_block(&env_map);
	env_map_deinit(&env_map);
	return env_block;
}

// Zig's start code logs `error: <ErrorName>` when main returns an error and
// exits with status 1.
static int main_error(const char *error_name) {
	rv_err(RV_SCOPE_DEFAULT, "%s", error_name);
	return 1;
}

static void print_usage(FILE *stream) {
	fputs(usage, stream);
	fflush(stream);
}

static bool log_scope_from_string(const char *name, size_t len, enum rv_log_scope *scope) {
	for (size_t i = 0; i < RV_SCOPE_COUNT; i++) {
		if (strlen(scope_names[i]) == len && strncmp(scope_names[i], name, len) == 0) {
			*scope = (enum rv_log_scope)i;
			return true;
		}
	}
	return false;
}

int main(int argc, char *argv[]) {
	initial_environ = environ;

	// flags.parser(&.{ h, version, c, log-level, log-scopes, no-xwayland }).parse(args[1..])
	struct {
		bool h;
		bool version;
		const char *c;
		const char *log_level;
		const char *log_scopes;
		bool no_xwayland;
	} flags = { 0 };
	int i = 1;
	for (; i < argc; i++) {
		const char *arg = argv[i];
		const char *missing = NULL;
		if (strcmp(arg, "-h") == 0) {
			flags.h = true;
		} else if (strcmp(arg, "-version") == 0) {
			flags.version = true;
		} else if (strcmp(arg, "-c") == 0) {
			if (++i == argc) {
				missing = "c";
			} else {
				flags.c = argv[i];
			}
		} else if (strcmp(arg, "-log-level") == 0) {
			if (++i == argc) {
				missing = "log-level";
			} else {
				flags.log_level = argv[i];
			}
		} else if (strcmp(arg, "-log-scopes") == 0) {
			if (++i == argc) {
				missing = "log-scopes";
			} else {
				flags.log_scopes = argv[i];
			}
		} else if (strcmp(arg, "-no-xwayland") == 0) {
			flags.no_xwayland = true;
		} else {
			break;
		}
		if (missing != NULL) {
			rv_err(RV_SCOPE_DEFAULT, "option '-%s' requires an argument but none was provided!", missing);
			print_usage(stderr);
			exit(1);
		}
	}
	// Remaining args after the recognized flags
	const int args_start = i;
	const int args_len = argc - args_start;

	if (flags.h) {
		print_usage(stdout);
		exit(0);
	}
	if (args_len != 0) {
		rv_err(RV_SCOPE_DEFAULT, "unknown option '%s'", argv[args_start]);
		print_usage(stderr);
		exit(1);
	}

	if (flags.version) {
		fputs(FULL_VERSION "\n", stdout);
		fflush(stdout);
		exit(0);
	}
	if (flags.log_level != NULL) {
		const char *level = flags.log_level;
		if (strcmp(level, "error") == 0) {
			runtime_log_level = RV_LOG_ERR;
		} else if (strcmp(level, "warning") == 0) {
			runtime_log_level = RV_LOG_WARN;
		} else if (strcmp(level, "info") == 0) {
			runtime_log_level = RV_LOG_INFO;
		} else if (strcmp(level, "debug") == 0) {
			runtime_log_level = RV_LOG_DEBUG;
		} else {
			rv_err(RV_SCOPE_DEFAULT, "invalid log level '%s'", level);
			exit(1);
		}
	}
	if (flags.log_scopes != NULL) {
		// Examples:
		// -log-scopes input,wm     (only default, input, and wm scopes)
		// -log-scopes all,~wlroots (all scopes except wlroots)
		for (size_t s = 0; s < RV_SCOPE_COUNT; s++) {
			log_scopes[s] = false;
		}
		const char *raw = flags.log_scopes;
		while (true) {
			const char *comma = strchr(raw, ',');
			const size_t raw_len = comma != NULL ? (size_t)(comma - raw) : strlen(raw);
			if (raw_len == strlen("all") && strncmp(raw, "all", raw_len) == 0) {
				for (size_t s = 0; s < RV_SCOPE_COUNT; s++) {
					log_scopes[s] = true;
				}
			} else if (raw_len > 0 && raw[0] == '~') {
				// I'd rather use an exclamation mark than a tilde but the
				// former requires quoting in most shells.
				enum rv_log_scope scope;
				if (!log_scope_from_string(raw + 1, raw_len - 1, &scope)) {
					rv_err(RV_SCOPE_DEFAULT, "invalid log scope '%.*s'", (int)raw_len, raw);
					exit(1);
				}
				log_scopes[scope] = false;
			} else {
				enum rv_log_scope scope;
				if (!log_scope_from_string(raw, raw_len, &scope)) {
					rv_err(RV_SCOPE_DEFAULT, "invalid log scope '%.*s'", (int)raw_len, raw);
					exit(1);
				}
				log_scopes[scope] = true;
			}
			if (comma == NULL) {
				break;
			}
			raw = comma + 1;
		}
	}
	const bool runtime_xwayland = !flags.no_xwayland;
	char *startup_command;
	if (flags.c != NULL) {
		startup_command = strdup(flags.c);
		if (startup_command == NULL) {
			return main_error("OutOfMemory");
		}
	} else {
		bool oom;
		startup_command = default_init_path(&oom);
		if (oom) {
			return main_error("OutOfMemory");
		}
	}

	detect_classic(startup_command);

	rv_info(RV_SCOPE_DEFAULT, "initializing rivulet version %s", FULL_VERSION);
#if WLR_HAS_XWAYLAND
	if (!runtime_xwayland) {
		rv_info(RV_SCOPE_DEFAULT, "Xwayland disabled at runtime with -no-xwayland");
	}
#endif

	switch (runtime_log_level) {
	case RV_LOG_DEBUG:
		rv_init_wlroots_log(WLR_DEBUG);
		break;
	case RV_LOG_INFO:
		rv_init_wlroots_log(WLR_INFO);
		break;
	case RV_LOG_WARN:
	case RV_LOG_ERR:
		rv_init_wlroots_log(WLR_ERROR);
		break;
	}

	const char *error_name = NULL;
	if (!rv_server_init(&server, runtime_xwayland, &error_name)) {
		free(startup_command);
		return main_error(error_name);
	}
	// From here on every return runs server.deinit() first (Zig: defer).

	// wlroots starts the Xwayland process from an idle event source, the reasoning being that
	// this gives the compositor time to set up event listeners before Xwayland is actually
	// started. We want Xwayland to be started by wlroots before we modify our rlimits in
	// process.setup() since wlroots does not offer a way for us to reset the rlimit post-fork.
#if WLR_HAS_XWAYLAND
	if (runtime_xwayland) {
		wl_event_loop_dispatch_idle(wl_display_get_event_loop(server.wl_server));
	}
#endif

	rv_process_setup();

	char buf[11];
	const char *socket = add_socket_auto(server.wl_server, buf);
	if (socket == NULL) {
		rv_server_deinit(&server);
		free(startup_command);
		return main_error("AddSocketFailed");
	}
	if (!wlr_backend_start(server.backend)) {
		rv_server_deinit(&server);
		free(startup_command);
		return main_error("BackendStartFailed");
	}

	// Run the child in a new process group so that we can send SIGTERM to all
	// descendants on exit.
	bool has_child_pgid = false;
	pid_t child_pgid = 0;
	if (startup_command != NULL) {
		const char *cmd = startup_command;
		rv_info(RV_SCOPE_DEFAULT, "running init executable '%s'", cmd);

		char **env_block = create_child_env(socket);
		if (env_block == NULL) {
			rv_server_deinit(&server);
			free(startup_command);
			return main_error("OutOfMemory");
		}

		char *const child_args[] = { "/bin/sh", "-c", (char *)cmd, NULL };

		const pid_t pid = fork();
		if (pid < 0) {
			const char *fork_error;
			switch (errno) {
			case EAGAIN:
			case ENOMEM:
				fork_error = "SystemResources";
				break;
			case ENOSYS:
				fork_error = "OperationUnsupported";
				break;
			default:
				fork_error = "Unexpected";
				break;
			}
			env_block_free(env_block);
			rv_server_deinit(&server);
			free(startup_command);
			return main_error(fork_error);
		}

		if (pid == 0) {
			rv_process_cleanup_child();
			if (execve("/bin/sh", child_args, env_block) != 0) {
				exit(1);
			}
		}
		env_block_free(env_block);
		// Since the child has called setsid, the pid is the pgid
		has_child_pgid = true;
		child_pgid = pid;
	}

	rv_info(RV_SCOPE_DEFAULT, "running server");

	wl_display_run(server.wl_server);

	// Deferred in reverse order: kill the init process group, then
	// server.deinit(), then free the startup command.
	if (has_child_pgid) {
		if (kill(-child_pgid, SIGTERM) != 0) {
			const char *kill_error;
			switch (errno) {
			case EINVAL:
				abort(); // unreachable: invalid signal
			case EPERM:
				kill_error = "PermissionDenied";
				break;
			case ESRCH:
				kill_error = "ProcessNotFound";
				break;
			default:
				kill_error = "Unexpected";
				break;
			}
			rv_err(RV_SCOPE_DEFAULT, "failed to kill init process group: %s", kill_error);
		}
	}
	rv_server_deinit(&server);
	free(startup_command);
	return 0;
}
