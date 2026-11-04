# Desktop entry (application menu)

`install.sh` adds the panel to the KDE Plasma / GNOME application menu
(`~/.local/share/applications`).

```sh
desktop/install.sh              # this user only
sudo desktop/install.sh --system   # all users (/usr/share or $XDG_DATA_DIRS)
desktop/uninstall.sh            # remove the menu entry
```

The entry is called **TTRS Racing Wheel**, in the *Settings* category; searching
for "TTRS" or "wheel" finds it. Its context menu also opens the setup wizard
(*Run setup wizard*) and the connection check (*Check drivers & connections*).

`install.sh` writes a `.desktop` file that runs `launch.sh`, which in turn starts
`target/release/ttrs-wheel`. If the binary is missing (for example after
`cargo clean`), `launch.sh` builds it and shows a dialog if the build fails.

Files:

- `ttrs-wheel.desktop.in` – menu entry template (`@LAUNCH_SH@` is replaced with
  the path to the launcher)
- `ttrs-wheel.svg` – icon
- `launch.sh` – launcher
- `install.sh`, `uninstall.sh` – install and remove

If you move the project folder, run `install.sh` again, otherwise the menu entry
points at the old location.

# Profiles tab

The fourth tab, **Profiles**, keeps one tuning block per game and runs the
virtual Xbox 360 pad for the games that need it.

**Profiles**

- Pick a profile from the drop-down; **Add** creates a new one, **Duplicate**
  copies the current one, **Delete** removes it (the last one cannot be deleted).
- **Name** and **Steam AppID** (for example `1222680` for NFS Heat). With an
  AppID set you get a **Launch** button and the profile can switch itself on
  when that game starts (**Switch profile automatically when the game starts**).
  The watch reads Steam's `logs/compat_log.txt`, so it also works when the game
  is started from the Steam window rather than from the panel.
- **Use the virtual Xbox pad with this game** — the per-profile switch. The
  scanner and the game notes pre-fill it from their source, but the decision is
  yours: turn it off for a game that finds the wheel itself.
- Everything is stored in `~/.config/ttrs-wheel/profiles.conf`: gain, curve,
  deadzone, pedal swap, button map and the wheel-side rotation/overall.

**Scan Steam library**

Reads `libraryfolders.vdf` and `appmanifest_*.acf`, finds the installed racing
titles and adds a profile for every one that is missing. It uses no network and
no Steam API; a library that cannot be read is reported instead of silently
skipped.

**Game notes** (button next to the profile row) lists the known setups — one row
per game with a `NEEDS BRIDGE` or `NATIVE` badge, AppID, the in-game values, the
source link and a **Create profile** button. Nothing is applied automatically.

The same window shows the **Steam launch option** for the connected wheel,
generated from its VID/PID (TMX `0x044f/0xb67f`, T150 `0x044f/0xb677`):

```
SDL_GAMECONTROLLER_IGNORE_DEVICES=0x<vid>/0x<pid> %command%
```

Only the games marked `NEEDS BRIDGE` need it.

**ProtonDB badge**

Each profile shows the game's ProtonDB verdict (Platinum, Gold, Silver, Bronze,
Borked, Pending) with the number of reports. `BORKED` means the game will not run
under Proton at all — The Crew 2, for example, fails on BattlEye. Results are
cached for 24 hours; the **Check** button asks again.

**Virtual pad**

The manual Start/Stop switch lives on the **Settings** tab. Normally you do not
need it: a profile marked `BRIDGE` starts the pad by itself when it becomes
active (by hand or when its game starts), and a native profile stops it again so
the game sees the real wheel.

## Command line

```sh
target/release/ttrs-wheel --scan               # list racing games in the libraries
target/release/ttrs-wheel --protondb 646910    # ProtonDB verdict for an AppID
target/release/ttrs-wheel --bridge-check       # create the pad for 0.9 s and report
```

# Theme and text size

The ☰ menu in the header holds the text size (100 % … 150 %, default 120 %), the
theme (System / Light / Dark), *Firmware & support* and the *Setup wizard*.

*System* reads KDE's `kdeglobals` (`[Colors:Window] BackgroundNormal`) and
follows the desktop; the choice is stored in `~/.config/ttrs-wheel/ui.conf`.
From a terminal you can force it: `--theme dark|light|system`.
