#!/usr/bin/env bash
# Install the control panel into the desktop application menu (start menu).
#
#   app/desktop/install.sh              # this user only (~/.local/share)
#   sudo app/desktop/install.sh --system # all users (/usr/share)
#
# Re-run after moving the project folder so the menu entry keeps pointing at
# the right place. Remove with: app/desktop/uninstall.sh
set -euo pipefail

here="$(dirname "$(readlink -f "$0")")"
app_dir="$(dirname "$here")"
launch="$here/launch.sh"
desktop_template="$here/ttrs-wheel.desktop.in"
icon="$here/ttrs-wheel.svg"
entry_name="ttrs-wheel.desktop"

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

[ -f "$desktop_template" ] || { echo "missing $desktop_template" >&2; exit 1; }
[ -f "$icon" ] || { echo "missing $icon" >&2; exit 1; }

chmod +x "$launch"
mkdir -p "$apps_dir" "$icon_dir"

sed "s|@LAUNCH_SH@|$launch|g" "$desktop_template" > "$apps_dir/$entry_name"
chmod 644 "$apps_dir/$entry_name"
install -m 644 "$icon" "$icon_dir/ttrs-wheel.svg"

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$apps_dir/$entry_name" || true
fi

# Refresh the menu / icon caches so the entry shows up without logging out.
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$apps_dir" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t "$(dirname "$(dirname "$icon_dir")")" >/dev/null 2>&1 || true
fi
for kbuild in kbuildsycoca6 kbuildsycoca5; do
    if command -v "$kbuild" >/dev/null 2>&1; then
        "$kbuild" >/dev/null 2>&1 || true
        break
    fi
done

echo "Installed the menu entry: $apps_dir/$entry_name"
echo "It runs: $launch"
echo "Look for \"TTRS Racing Wheel\" in the application menu (Settings category, or search \"TTRS\")."
