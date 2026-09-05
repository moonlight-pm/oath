/*
 * libdecor plugin for pkg:gamescope.
 *
 * Ubuntu libdecor's built-in fallback (no plugin) and the cairo plugin
 * both fire xdg_surface.set_window_geometry while content size is still
 * 0. River treats width/height 0 as a protocol error and kills the
 * client. Cairo then also mmap()s a 0-byte shm buffer (EINVAL).
 *
 * Report 1px borders so the first geometry is 2x2. gamescope later
 * commits 1280x720. No CSD drawing.
 */
#define _GNU_SOURCE
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

/* Match libdecor 0.2.2 plugin ABI (LIBDECOR_PLUGIN_API_VERSION 1). */
#define LIBDECOR_PLUGIN_API_VERSION 1
#define LIBDECOR_EXPORT __attribute__((visibility("default")))
#define LIBDECOR_PLUGIN_PRIORITY_HIGH 1000
#define LIBDECOR_PLUGIN_CAPABILITY_BASE (1 << 0)

struct libdecor;
struct libdecor_frame;
struct libdecor_state;
struct libdecor_configuration;
struct libdecor_plugin;

struct libdecor_plugin_private;
struct libdecor_plugin {
	struct libdecor_plugin_private *priv;
};

struct libdecor_frame_private;
struct libdecor_frame {
	struct libdecor_frame_private *priv;
	char _wl_list[16];
};

typedef struct libdecor_plugin *(*libdecor_plugin_constructor)(struct libdecor *context);

struct libdecor_plugin_priority {
	const char *desktop;
	int priority;
};

enum libdecor_plugin_capabilities {
	LIBDECOR_PLUGIN_CAPABILITY_BASE_E = LIBDECOR_PLUGIN_CAPABILITY_BASE,
};

struct libdecor_plugin_description {
	int api_version;
	char *description;
	enum libdecor_plugin_capabilities capabilities;
	const struct libdecor_plugin_priority *priorities;
	libdecor_plugin_constructor constructor;
	char *conflicting_symbols[1024];
};

struct libdecor_plugin_interface {
	void (*destroy)(struct libdecor_plugin *plugin);
	int (*get_fd)(struct libdecor_plugin *plugin);
	int (*dispatch)(struct libdecor_plugin *plugin, int timeout);
	struct libdecor_frame *(*frame_new)(struct libdecor_plugin *plugin);
	void (*frame_free)(struct libdecor_plugin *plugin, struct libdecor_frame *frame);
	void (*frame_commit)(struct libdecor_plugin *plugin, struct libdecor_frame *frame,
			     struct libdecor_state *state, struct libdecor_configuration *configuration);
	void (*frame_property_changed)(struct libdecor_plugin *plugin, struct libdecor_frame *frame);
	void (*frame_popup_grab)(struct libdecor_plugin *plugin, struct libdecor_frame *frame,
				 const char *seat_name);
	void (*frame_popup_ungrab)(struct libdecor_plugin *plugin, struct libdecor_frame *frame,
				   const char *seat_name);
	bool (*frame_get_border_size)(struct libdecor_plugin *plugin, struct libdecor_frame *frame,
				      struct libdecor_configuration *configuration, int *left, int *right,
				      int *top, int *bottom);
	void (*reserved0)(void);
	void (*reserved1)(void);
	void (*reserved2)(void);
	void (*reserved3)(void);
	void (*reserved4)(void);
	void (*reserved5)(void);
	void (*reserved6)(void);
	void (*reserved7)(void);
	void (*reserved8)(void);
	void (*reserved9)(void);
};

int libdecor_plugin_init(struct libdecor_plugin *plugin, struct libdecor *context,
			 struct libdecor_plugin_interface *iface);
void libdecor_plugin_release(struct libdecor_plugin *plugin);
void libdecor_notify_plugin_ready(struct libdecor *context);

struct plugin_oath {
	struct libdecor_plugin plugin;
	struct libdecor *context;
};

static void plugin_destroy(struct libdecor_plugin *plugin) {
	libdecor_plugin_release(plugin);
	free(plugin);
}

static struct libdecor_frame *frame_new(struct libdecor_plugin *plugin) {
	(void)plugin;
	return calloc(1, sizeof(struct libdecor_frame));
}

static void frame_free(struct libdecor_plugin *plugin, struct libdecor_frame *frame) {
	(void)plugin;
	free(frame);
}

static void frame_commit(struct libdecor_plugin *plugin, struct libdecor_frame *frame,
			 struct libdecor_state *state, struct libdecor_configuration *configuration) {
	(void)plugin;
	(void)frame;
	(void)state;
	(void)configuration;
}

static void frame_property_changed(struct libdecor_plugin *plugin, struct libdecor_frame *frame) {
	(void)plugin;
	(void)frame;
}

static void frame_popup_grab(struct libdecor_plugin *plugin, struct libdecor_frame *frame, const char *seat_name) {
	(void)plugin;
	(void)frame;
	(void)seat_name;
}

static void frame_popup_ungrab(struct libdecor_plugin *plugin, struct libdecor_frame *frame, const char *seat_name) {
	(void)plugin;
	(void)frame;
	(void)seat_name;
}

static bool frame_get_border_size(struct libdecor_plugin *plugin, struct libdecor_frame *frame,
				  struct libdecor_configuration *configuration, int *left, int *right, int *top,
				  int *bottom) {
	(void)plugin;
	(void)frame;
	(void)configuration;
	/* 1px so first set_window_geometry is 2x2, not 0x0. */
	if (left)
		*left = 1;
	if (right)
		*right = 1;
	if (top)
		*top = 1;
	if (bottom)
		*bottom = 1;
	return true;
}

static struct libdecor_plugin_interface iface = {
	.destroy = plugin_destroy,
	.frame_new = frame_new,
	.frame_free = frame_free,
	.frame_commit = frame_commit,
	.frame_property_changed = frame_property_changed,
	.frame_popup_grab = frame_popup_grab,
	.frame_popup_ungrab = frame_popup_ungrab,
	.frame_get_border_size = frame_get_border_size,
};

static struct libdecor_plugin *plugin_new(struct libdecor *context) {
	struct plugin_oath *p = calloc(1, sizeof(*p));
	if (!p)
		return NULL;
	libdecor_plugin_init(&p->plugin, context, &iface);
	p->context = context;
	libdecor_notify_plugin_ready(context);
	return &p->plugin;
}

static const struct libdecor_plugin_priority priorities[] = {
	{NULL, LIBDECOR_PLUGIN_PRIORITY_HIGH},
};

LIBDECOR_EXPORT const struct libdecor_plugin_description libdecor_plugin_description = {
	.api_version = LIBDECOR_PLUGIN_API_VERSION,
	.description = "oath dummy: 1px borders so River accepts first xdg geometry",
	.capabilities = LIBDECOR_PLUGIN_CAPABILITY_BASE,
	.priorities = priorities,
	.constructor = plugin_new,
};
