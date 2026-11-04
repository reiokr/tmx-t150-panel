#!/usr/bin/env bash
# Launch the TMX/T150 control panel (used by the application menu entry).
# Builds the release binary on first start so the menu entry always works.
set -euo pipefail

here="$(dirname "$(readlink -f "$0")")"
app_dir="$(dirname "$here")"
bin="$app_dir/target/release/tmx-t150-panel"

fail() {
    echo "tmx-t150-panel: $1" >&2
    if command -v kdialog >/dev/null 2>&1; then
        kdialog --title "TMX/T150 Control Panel" --error "$1" >/dev/null 2>&1 || true
    elif command -v notify-send >/dev/null 2>&1; then
        notify-send "TMX/T150 Control Panel" "$1" || true
    fi
    exit 1
}

if [ ! -x "$bin" ]; then
    command -v cargo >/dev/null 2>&1 || fail "cargo not found - install Rust to build the app."
    if ! (cd "$app_dir" && cargo build --release); then
        fail "Build failed. Run 'cargo build --release' in $app_dir to see the error."
    fi
fi

cd "$app_dir"
exec "$bin" "$@"
