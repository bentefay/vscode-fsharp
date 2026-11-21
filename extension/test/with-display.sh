#!/usr/bin/env bash
set -euo pipefail
if [[ $(uname -s) != Linux ]]; then exec "$@"; fi
FSNAV_TEST_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
FSNAV_DISPLAY_TOOLS="$FSNAV_TEST_ROOT/.tools/display/root"
if [[ -d "$FSNAV_DISPLAY_TOOLS/usr/bin" ]]; then
    export PATH="$FSNAV_DISPLAY_TOOLS/usr/bin:$PATH"
    export FSNAV_OPENBOX_LIBRARIES="$FSNAV_DISPLAY_TOOLS/usr/lib/x86_64-linux-gnu${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    export FSNAV_OPENBOX_DATA="$FSNAV_DISPLAY_TOOLS/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
fi
unset WAYLAND_DISPLAY WAYLAND_SOCKET DISPLAY DBUS_SESSION_BUS_ADDRESS XDG_ACTIVATION_TOKEN DESKTOP_STARTUP_ID
export XDG_SESSION_TYPE=x11 GDK_BACKEND=x11 QT_QPA_PLATFORM=xcb ELECTRON_OZONE_PLATFORM_HINT=x11
FSNAV_DISPLAY_TEMP="$(mktemp -d -t fsnav-display-XXXXXX)"
trap 'rm -rf -- "$FSNAV_DISPLAY_TEMP"' EXIT
export XDG_RUNTIME_DIR="$FSNAV_DISPLAY_TEMP/runtime"
mkdir -m 700 "$XDG_RUNTIME_DIR"
export FSNAV_DISPLAY_LOG_DIR="${FSHARP_DISPLAY_LOG_DIR:-$FSNAV_TEST_ROOT/test-results/display-$(basename "$FSNAV_DISPLAY_TEMP")}" 
mkdir -p "$FSNAV_DISPLAY_LOG_DIR"
export FSNAV_OPENBOX_CONFIG="$FSNAV_DISPLAY_TOOLS/etc/xdg/openbox/rc.xml"
xvfb-run -a -s '-screen 0 1440x1000x24 -nolisten tcp' dbus-run-session --config-file="$FSNAV_TEST_ROOT/extension/test/dbus-session.conf" -- bash -c '
set -euo pipefail
if [[ -f "$FSNAV_OPENBOX_CONFIG" ]]; then
    LD_LIBRARY_PATH="$FSNAV_OPENBOX_LIBRARIES" XDG_DATA_DIRS="$FSNAV_OPENBOX_DATA" openbox --config-file "$FSNAV_OPENBOX_CONFIG" >"$FSNAV_DISPLAY_LOG_DIR/openbox.log" 2>&1 &
else
    openbox >"$FSNAV_DISPLAY_LOG_DIR/openbox.log" 2>&1 &
fi
wm_pid=$!
trap '\''kill "$wm_pid" 2>/dev/null || true'\'' EXIT
"$@"
' bash "$@"
