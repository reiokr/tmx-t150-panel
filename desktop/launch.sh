#!/usr/bin/env bash
# Launch the TTRS wheel control panel (used by the application menu entry).
# Builds the release binary on first start so the menu entry always works.
set -euo pipefail

here="$(dirname "$(readlink -f "$0")")"
app_dir="$(dirname "$here")"
bin="$app_dir/target/release/ttrs-wheel"

fail() {
    echo "ttrs-wheel: $1" >&2
    if command -v kdialog >/dev/null 2>&1; then
        kdialog --title "TTRS Racing Wheel" --error "$1" >/dev/null 2>&1 || true
    elif command -v notify-send >/dev/null 2>&1; then
        notify-send "TTRS Racing Wheel" "$1" || true
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
