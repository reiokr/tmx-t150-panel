#!/usr/bin/env bash
# Install the prebuilt TMX/T150 Control Panel from this folder.
#
#   ./install.sh            # this user: ~/.local/bin + ~/.local/share
#   sudo ./install.sh --system   # all users: /usr/local
#
# Only the application is installed. The kernel module (force feedback and
# wheel settings) is a separate component: see driver/t150/README.md.
set -euo pipefail

here="$(dirname "$(readlink -f "$0")")"
binary="$here/tmx-t150-panel"
icon_src="$here/tmx-t150-panel.svg"

[ -x "$binary" ] || { echo "missing $binary" >&2; exit 1; }
[ -f "$icon_src" ] || { echo "missing $icon_src" >&2; exit 1; }

if [ "${1:-}" = "--system" ]; then
    bin_dir="/usr/local/bin"
    data_dir="/usr/local/share"
else
    bin_dir="${HOME}/.local/bin"
    data_dir="${XDG_DATA_HOME:-$HOME/.local/share}"
fi

apps_dir="$data_dir/applications"
icon_dir="$data_dir/icons/hicolor/scalable/apps"

install -d "$bin_dir" "$apps_dir" "$icon_dir"
install -m 755 "$binary" "$bin_dir/tmx-t150-panel"
install -m 644 "$icon_src" "$icon_dir/tmx-t150-panel.svg"

cat > "$apps_dir/tmx-t150-panel.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=TMX/T150 Control Panel
Name[et]=TMX/T150 juhtpaneel
GenericName=Racing wheel control panel
Comment=Force feedback control panel for Thrustmaster T150 / TMX wheels
Exec=$bin_dir/tmx-t150-panel
TryExec=$bin_dir/tmx-t150-panel
Icon=tmx-t150-panel
Terminal=false
Categories=Settings;HardwareSettings;
Keywords=wheel;steering;force;feedback;thrustmaster;t150;tmx;
StartupNotify=true
StartupWMClass=tmx-t150-panel
Actions=Setup;Diagnostics;

[Desktop Action Setup]
Name=Run setup wizard
Exec=$bin_dir/tmx-t150-panel --setup

[Desktop Action Diagnostics]
Name=Check drivers & connections
Exec=$bin_dir/tmx-t150-panel --diagnose
DESKTOP
chmod 644 "$apps_dir/tmx-t150-panel.desktop"

command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$apps_dir" >/dev/null 2>&1 || true
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t "$data_dir/icons/hicolor" >/dev/null 2>&1 || true
fi
for kbuild in kbuildsycoca6 kbuildsycoca5; do
    if command -v "$kbuild" >/dev/null 2>&1; then "$kbuild" >/dev/null 2>&1 || true; break; fi
done

echo "Installed: $bin_dir/tmx-t150-panel"
echo "Menu entry: $apps_dir/tmx-t150-panel.desktop"
echo
echo "Put the wheel in PC mode (044f:b67f TMX / 044f:b677 T150). The panel runs"
echo "without the kernel module, but force feedback and the wheel settings need it:"
echo "  cd driver/t150 && sudo ./install.sh"
