#!/usr/bin/env bash
# Remove the prebuilt TMX/T150 Control Panel installed by install.sh.
#
#   ./uninstall.sh              # this user
#   sudo ./uninstall.sh --system
#
# Profiles and settings in ~/.config/tmx-t150-panel are left alone.
set -euo pipefail

if [ "${1:-}" = "--system" ]; then
    bin_dir="/usr/local/bin"
    data_dir="/usr/local/share"
else
    bin_dir="${HOME}/.local/bin"
    data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
fi

apps_dir="$data_dir/applications"
icon_dir="$data_dir/icons/hicolor/scalable/apps"

rm -f "$bin_dir/tmx-t150-panel"
rm -f "$apps_dir/tmx-t150-panel.desktop"
rm -f "$icon_dir/tmx-t150-panel.svg"

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$apps_dir" >/dev/null 2>&1 || true
for kbuild in kbuildsycoca6 kbuildsycoca5; do
    if command -v "$kbuild" >/dev/null 2>&1; then "$kbuild" >/dev/null 2>&1 || true; break; fi
done

echo "Removed the application. Settings in ~/.config/tmx-t150-panel were kept."
