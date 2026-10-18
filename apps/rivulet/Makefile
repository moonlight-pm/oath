# rivulet: River 0.4.5 ported to C for Oath (R19). GPL-3.0-only, like the Zig original.

VERSION = 0.1.0
PREFIX ?= /usr/local
PKG_CONFIG ?= pkg-config
BUILD ?= build

PKGS = wlroots-0.20 wayland-server wayland-protocols xkbcommon pixman-1 libinput libevdev
# wlroots' Xwayland header includes xcb-util-wm's headers.
ifeq ($(shell $(PKG_CONFIG) --variable=have_xwayland wlroots-0.20),true)
PKGS += xcb xcb-icccm xcb-ewmh
endif
WAYLAND_SCANNER := $(shell $(PKG_CONFIG) --variable=wayland_scanner wayland-scanner)

CFLAGS ?= -O2 -g
override CFLAGS += -std=c11 -D_POSIX_C_SOURCE=200809L -DWLR_USE_UNSTABLE -DRIVULET_VERSION='"$(VERSION)"'
override CFLAGS += -Wall -Wextra -Wno-unused-parameter -Wno-missing-field-initializers
override CFLAGS += -Isrc -I$(BUILD) $(shell $(PKG_CONFIG) --cflags $(PKGS))
# River asks for xkbcommon 1.12 (xkb_keymap_get_as_string2); older ones fall back.
override CFLAGS += $(shell $(PKG_CONFIG) --atleast-version=1.12 xkbcommon && echo -DRV_HAVE_XKB_KEYMAP_GET_AS_STRING2)
override LDLIBS += $(shell $(PKG_CONFIG) --libs $(PKGS)) -lm

# River's own protocols: server header and glue. The wlr protocols only need
# headers, because wlroots' own headers include them.
RIVER_PROTOCOLS = \
	river-window-management-v1 \
	river-xkb-bindings-v1 \
	river-layer-shell-v1 \
	river-input-management-v1 \
	river-libinput-config-v1 \
	river-xkb-config-v1
HEADER_PROTOCOLS = \
	wlr-layer-shell-unstable-v1 \
	wlr-output-power-management-unstable-v1 \
	virtual-keyboard-unstable-v1

PROTOCOL_HEADERS = $(patsubst %,$(BUILD)/%-protocol.h,$(RIVER_PROTOCOLS) $(HEADER_PROTOCOLS))
PROTOCOL_OBJS = $(patsubst %,$(BUILD)/%-protocol.o,$(RIVER_PROTOCOLS))

SRCS = $(wildcard src/*.c)
OBJS = $(patsubst src/%.c,$(BUILD)/%.o,$(SRCS))

all: $(BUILD)/rivulet

$(BUILD)/rivulet: $(OBJS) $(PROTOCOL_OBJS)
	$(CC) $(LDFLAGS) -o $@ $^ $(LDLIBS)

$(BUILD)/%.o: src/%.c src/*.h $(PROTOCOL_HEADERS) | $(BUILD)
	$(CC) $(CFLAGS) -c -o $@ $<

$(BUILD)/%-protocol.h: protocol/%.xml | $(BUILD)
	$(WAYLAND_SCANNER) server-header $< $@

$(BUILD)/%-protocol.c: protocol/%.xml | $(BUILD)
	$(WAYLAND_SCANNER) private-code $< $@

$(BUILD)/%-protocol.o: $(BUILD)/%-protocol.c $(BUILD)/%-protocol.h
	$(CC) $(CFLAGS) -c -o $@ $<

$(BUILD):
	mkdir -p $(BUILD)

protocols: $(PROTOCOL_HEADERS)

install: $(BUILD)/rivulet
	install -Dm755 $(BUILD)/rivulet $(DESTDIR)$(PREFIX)/bin/rivulet
	for p in $(RIVER_PROTOCOLS); do \
		install -Dm644 protocol/$$p.xml $(DESTDIR)$(PREFIX)/share/river-protocols/stable/$$p.xml; \
	done

clean:
	rm -rf $(BUILD)

.PHONY: all protocols install clean
.SECONDARY: $(patsubst %,$(BUILD)/%-protocol.c,$(RIVER_PROTOCOLS))
