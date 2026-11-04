# hid-t150 (updated)

This is an updated build of the Linux kernel driver for Thrustmaster wheels.
The original source came from `/usr/src/t150-1.0` (the DKMS package `t150`
1.0) and targeted much older kernels.

## What was changed

- **Builds on current kernels** (tested on 7.3.1-1-cachyos). CachyOS/Arch
  kernels are built with clang, so the `Makefile` detects that and adds
  `LLVM=1` itself; gcc-built kernels still work the old way.
- **Real bug fixed:** the `range` sysfs file read an uninitialised
  `dev_max_range` variable when the product ID was unknown. It now defaults to
  1080°.
- **Format string fixed:** `%04hhX` → `%04hX` (the argument is `uint16_t`).
- **Clearer braces** when locating the HID endpoints in `hid-t150.c`, so an
  `else` cannot bind to the wrong `if`.
- **Settings writable without root:** `range`, `gain`, `autocenter` and
  `enable_autocenter` are created with mode 0666 so the control panel can write
  them as a normal user (the Windows original does the same through its
  service). `firmware_version` stays read-only.
- **DKMS and udev installation** (`install.sh`, `60-hid-t150.rules`).

## Supported IDs

| USB ID | Model | Mode |
|---|---|---|
| `044f:b677` | Thrustmaster T150 | PC (hid-t150) |
| `044f:b67f` | Thrustmaster TMX | PC (hid-t150) |

If the wheel is in Xbox/GIP mode (`044f:b67e`), the in-kernel `xpad` driver
claims it. That ID is not in the `hid-t150` table (only `b677` and `b67f` are),
so `modprobe` and a reboot **will not help** — the wheel has to be switched to
PC mode.

- **TMX:** there is no switch, and the MODE button on the rim does not change
  the USB mode (it only changes pedal behaviour). The control panel has a
  **Switch to PC mode** button that sends the same two USB requests as the
  Windows driver (`b67e → b65d → b67f`). From a terminal:
  `ttrs-wheel --tmx-switch` (needs privileges: `sudo .../ttrs-wheel --tmx-switch`).
- **T150:** the slide switch must be in the **PS3** position before the cable
  is connected (Thrustmaster calls this the PC position).

## Installation

```bash
cd driver/t150
sudo ./install.sh
```

Then reconnect the wheel or reboot. Check with:

```bash
dmesg | tail -20
ls /sys/bus/hid/devices/*/range
```

The bottom of the control panel must say "PC mode (hid-t150)". If it says
"Xbox mode (xpad)", the wheel is still in GIP mode or the module is not loaded.

## Sysfs interface (used by the control panel)

`/sys/bus/hid/devices/0003:044F:B677.*/`

- `range` – rotation in degrees (40–1080; 40–900 on the TMX, as in the Windows
  original)
- `gain` – force feedback strength (0–65535, 65535 = 100 %)
- `autocenter` – auto-centre strength (0–65535)
- `enable_autocenter` – `1` / `0`
- `firmware_version` – firmware number (read-only)

## Building without installing

```bash
make            # detects a clang-built kernel
# or for a specific kernel:
make KDIR=/lib/modules/$(uname -r)/build
```

Result: `build/hid-t150.ko`.

## Licence

GNU General Public License version 2 — see [LICENSE](LICENSE). This module is a
separate program from the application and is not covered by the MIT licence at
the repository root.
