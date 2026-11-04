#!/usr/bin/env bash
# Build and install the refreshed hid-t150 driver through DKMS.
#
#   sudo ./install.sh
#
# The kernel module gives the wheel its PC-mode input device and exposes the
# settings the control panel writes (range, gain, auto-centre).
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
name="t150"
ver="1.1"

if [[ ${EUID} -ne 0 ]]; then
    echo "Please run with sudo: sudo $0" >&2
    exit 1
fi

if ! command -v dkms >/dev/null; then
    echo "dkms is not installed (pacman -S dkms)." >&2
    exit 1
fi

target="/usr/src/${name}-${ver}"
echo "Installing sources to ${target}"
rm -rf "${target}"
mkdir -p "${target}"
cp -a "${here}/." "${target}/"
rm -rf "${target}/build" "${target}/hid-t150"/*.o "${target}/hid-t150"/*.ko

echo "Building and installing with DKMS"
dkms remove "${name}/${ver}" --all >/dev/null 2>&1 || true
dkms install "${name}/${ver}" --force
depmod -a

install -Dm644 "${here}/60-hid-t150.rules" /etc/udev/rules.d/60-hid-t150.rules
udevadm control --reload-rules
udevadm trigger --subsystem-match=input || true

modprobe hid-t150 || true

cat <<'MSG'

Done.

Next steps:
  1. T150: put the sliding switch on the base in the PS3 position before reconnecting.
     TMX: there is no switch; press "Switch to PC mode" in the control panel
     (it sends the same USB command as the Windows driver).
  2. Reconnect it (or reboot) so the kernel binds hid-t150 instead of xpad.
  3. Start the control panel: it should say "PC mode (hid-t150)".

Verify with:  dmesg | tail -20     ls /sys/bus/hid/devices/*/range
MSG
