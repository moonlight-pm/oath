/* Feral Interactive's GameMode is not on Oath. This stands in for its
 * gamemode_client.h, which Prism Launcher includes: every request says no,
 * and Prism greys out its GameMode setting. */
#pragma once
#include <sys/types.h>

static inline int gamemode_query_status(void) { return -1; }
static inline int gamemode_request_start_for(pid_t pid) { (void)pid; return -1; }
static inline const char *gamemode_error_string(void) { return "GameMode is not on Oath"; }
