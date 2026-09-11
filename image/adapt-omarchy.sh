#!/usr/bin/env bash
# Adapt a packed Omarchy tree so it can run on Oath (no pacman, no UWSM,
# busybox userland, Quickshell Process without PATH search).
set -euo pipefail

out=${1:?out}

chmod -R u+rwX "$out"
mkdir -p "$out/bin" "$out/applications"

# Quickshell 0.2 Process execs argv[0] without a guaranteed PATH. The
# plugin scanner is bash + find; mkdir is a builtin so it already worked.
reg=$out/shell/services/PluginRegistry.qml
if [[ -f $reg ]]; then
  sed -i 's/\["bash", "-c"/["\/bin\/bash", "-c"/g' "$reg"
fi

# GNU timeout flags are not busybox. qs farm link is easy to break (busybox
# tar strips absolute symlink targets). Point omarchy-shell at known paths.
shell=$out/bin/omarchy-shell
if [[ -f $shell ]]; then
  sed -i 's#timeout --kill-after=1s "\$ipc_timeout" qs ipc#/bin/busybox timeout -k 1 2 /oath/store/pkg/quickshell/libexec/quickshell ipc#g' "$shell"
fi

write_exec() {
  local dest=$1
  cat >"$dest"
  chmod 755 "$dest"
}

write_exec "$out/bin/uwsm-app" <<'SH'
#!/bin/sh
[ "${1:-}" = "--" ] && shift
[ $# -gt 0 ] || { echo "uwsm-app: missing command" >&2; exit 2; }
exec "$@"
SH

write_exec "$out/bin/xdg-terminal-exec" <<'SH'
#!/bin/sh
dir="${HOME:-/home}"
while [ $# -gt 0 ]; do
  case "$1" in
  --dir=*) dir="${1#--dir=}"; shift ;;
  --dir) dir="${2:-$dir}"; shift 2 ;;
  --print-id) echo "sola-terminal.desktop"; exit 0 ;;
  --) shift; break ;;
  -*) shift ;;
  *) break ;;
  esac
done
cd "$dir" 2>/dev/null || cd "${HOME:-/home}" || true
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
if [ ! -S "$XDG_RUNTIME_DIR/sola-bus" ] && [ -x /bin/sola-bus ]; then
  /bin/sola-bus >>/oath/log/sola-bus.log 2>&1 &
  i=0
  while [ "$i" -lt 25 ]; do
    [ -S "$XDG_RUNTIME_DIR/sola-bus" ] && break
    i=$((i + 1))
    /bin/sleep 0.2
  done
fi
exec /bin/sola-terminal
SH

write_exec "$out/bin/gtk-launch" <<'SH'
#!/bin/sh
id="${1:-}"
id="${id%.desktop}"
[ -n "$id" ] || { echo "gtk-launch: missing desktop id" >&2; exit 2; }
shift
for d in \
  "${XDG_DATA_HOME:-$HOME/.local/share}/applications" \
  /oath/store/pkg/omarchy/applications \
  /oath/store/pkg/sola/share/applications \
  /home/.local/share/applications; do
  [ -n "$d" ] || continue
  f="$d/$id.desktop"
  [ -f "$f" ] || continue
  exec_line=$(sed -n 's/^Exec=//p' "$f" | head -n 1)
  [ -n "$exec_line" ] || continue
  exec_line=$(printf '%s\n' "$exec_line" | sed 's/ %[a-zA-Z]//g')
  # shellcheck disable=SC2086
  exec $exec_line "$@"
done
echo "gtk-launch: desktop file not found: $id" >&2
exit 1
SH

# Super+K → omarchy-menu-keybindings → omarchy-menu-select. Upstream select
# uses perl JSON::PP; we have neither perl nor gawk. JSON-escape in bash
# and talk to Quickshell the same way omarchy-menu-toggle does.
write_exec "$out/bin/omarchy-menu-select" <<'SH'
#!/bin/bash
set -euo pipefail

if (( $# < 1 )); then
  echo "Usage: omarchy-menu-select <prompt> [option...] [-- menu args...]" >&2
  exit 1
fi

prompt="$1"
shift

options=()
menu_width=""
menu_maxheight=""

while (( $# > 0 )); do
  if [[ $1 == "--" ]]; then
    shift
    while (( $# > 0 )); do
      case "$1" in
        --width)
          shift
          [[ $# -gt 0 ]] || { echo "omarchy-menu-select: --width requires a value" >&2; exit 1; }
          menu_width="$1"
          ;;
        --height|--maxheight)
          shift
          [[ $# -gt 0 ]] || { echo "omarchy-menu-select: --height requires a value" >&2; exit 1; }
          menu_maxheight="$1"
          ;;
      esac
      shift
    done
    break
  fi
  options+=("$1")
  shift
done

if (( ${#options[@]} == 0 )) && [[ ! -t 0 ]]; then
  mapfile -t options
fi

if (( ${#options[@]} == 0 )); then
  echo "Usage: omarchy-menu-select <prompt> [option...] [-- menu args...]" >&2
  exit 1
fi

json_escape() {
  local s=$1
  s=${s//\\/\\\\}
  s=${s//\"/\\\"}
  s=${s//$'\t'/\\t}
  s=${s//$'\r'/\\r}
  s=${s//$'\n'/\\n}
  printf '%s' "$s"
}

json_array() {
  local first=1 o
  printf '['
  for o in "$@"; do
    [[ $first -eq 1 ]] || printf ','
    first=0
    printf '"%s"' "$(json_escape "$o")"
  done
  printf ']'
}

selection_file=$(mktemp)
done_file=$(mktemp)
rm -f "$done_file"
trap 'rm -f "$selection_file" "$done_file"' EXIT

opts_json=$(json_array "${options[@]}")
payload=$(printf '{"mode":"select","prompt":"%s","options":%s,"selectionFile":"%s","doneFile":"%s"' \
  "$(json_escape "$prompt")" "$opts_json" "$(json_escape "$selection_file")" "$(json_escape "$done_file")")
[[ -n $menu_width ]] && payload+=$(printf ',"width":%s' "$menu_width")
[[ -n $menu_maxheight ]] && payload+=$(printf ',"maxHeight":%s' "$menu_maxheight")
payload+='}'

export HOME="${HOME:-/home}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/1}"
export OMARCHY_PATH="${OMARCHY_PATH:-/oath/store/pkg/omarchy}"
export PATH="/lib/oath:/oath/store/pkg/omarchy/bin:/bin"
if [[ -z ${WAYLAND_DISPLAY:-} ]]; then
  for s in "$XDG_RUNTIME_DIR"/wayland-*; do
    [[ -S $s ]] || continue
    case "$s" in
    *.lock) continue ;;
    esac
    export WAYLAND_DISPLAY=$(basename "$s")
    break
  done
fi
if [[ -z ${HYPRLAND_INSTANCE_SIGNATURE:-} && -d $XDG_RUNTIME_DIR/hypr ]]; then
  export HYPRLAND_INSTANCE_SIGNATURE=$(ls "$XDG_RUNTIME_DIR/hypr" | head -1)
fi

qs=/oath/store/pkg/quickshell/libexec/quickshell
[[ -x $qs ]] || qs=/bin/quickshell
"$qs" ipc -n -p "$OMARCHY_PATH/shell" call -- shell summon omarchy.menu "$payload" >/dev/null

while [[ ! -e $done_file ]]; do
  sleep 0.05
done

if [[ -s $selection_file ]]; then
  cat "$selection_file"
else
  exit 1
fi
SH

# busybox awk has no gawk 3-arg match(); 0.52 binds are named keys, not code:.
# Rename the original (it runs xkbcli) and define cat before the main path.
if [[ -f $out/bin/omarchy-menu-keybindings ]]; then
  sed -i 's/^parse_keycodes() {$/_oath_unused_parse_keycodes() {/' \
    "$out/bin/omarchy-menu-keybindings"
  sed -i '1a\
parse_keycodes() { cat; }
' "$out/bin/omarchy-menu-keybindings"
fi

write_exec "$out/bin/omarchy-pkg-present" <<'SH'
#!/bin/bash
exec /lib/oath/omarchy-pkg-oath present "$@"
SH
write_exec "$out/bin/omarchy-pkg-missing" <<'SH'
#!/bin/bash
exec /lib/oath/omarchy-pkg-oath missing "$@"
SH
write_exec "$out/bin/omarchy-pkg-add" <<'SH'
#!/bin/bash
exec /lib/oath/omarchy-pkg-oath add "$@"
SH
write_exec "$out/bin/omarchy-pkg-drop" <<'SH'
#!/bin/bash
exec /lib/oath/omarchy-pkg-oath drop "$@"
SH

cat >"$out/applications/sola-terminal.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Terminal
GenericName=Terminal
Exec=/bin/sola-terminal
TryExec=/bin/sola-terminal
Icon=utilities-terminal
Terminal=false
Categories=System;TerminalEmulator;
StartupNotify=true
EOF
cat >"$out/applications/sola-browser.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Browser
GenericName=Web Browser
Exec=/bin/sola-browser %u
TryExec=/bin/sola-browser
Icon=web-browser
Terminal=false
Categories=Network;WebBrowser;
MimeType=x-scheme-handler/http;x-scheme-handler/https;text/html;
StartupNotify=true
EOF
cat >"$out/applications/grok.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Grok
Exec=/bin/grok
TryExec=/bin/grok
Icon=utilities-terminal
Terminal=true
Categories=Development;
EOF
