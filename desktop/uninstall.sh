#!/usr/bin/env bash
# Remove the control panel from the application menu.
set -euo pipefail

entry_name="tmx-t150-panel.desktop"

if [ "${1:-}" = "--system" ]; then
    prefix="${XDG_DATA_DIRS%%:*}"
    prefix="${prefix:-/usr/local/share}"
    apps_dir="$prefix/applications"
    icon_dir="$prefix/icons/hicolor/scalable/apps"
else
    data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
    apps_dir="$data_home/applications"
    icon_dir="$data_home/icons/hicolor/scalable/apps"
fi

rm -f "$apps_dir/$entry_name" "$icon_dir/tmx-t150-panel.svg"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$apps_dir" >/dev/null 2>&1 || true
fi
for kbuild in kbuildsycoca6 kbuildsycoca5; do
    if command -v "$kbuild" >/dev/null 2>&1; then
        "$kbuild" >/dev/null 2>&1 || true
        break
    fi
done

echo "Removed the menu entry from $apps_dir"
