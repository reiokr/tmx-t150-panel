# What the original does — mechanics sheet

Target: `2026_TTRS_2.exe` (PE32, x86, MSVC, 50,836,032 bytes, image base
0x400000, sha256 `fccd2dff4ecebf4a13616d6d7acb836ba668c0d327c1f5bb990947355842bc09`).

The `.exe` is a **Flexera InstallShield 32.0.68 InstallScript setup launcher**
(`FileDescription: InstallScript Setup Launcher Unicode`, product
`Thrustmaster FFB Racing Wheel`). Its entry point `0x44bf50` is only the
MSVC/InstallShield CRT bootstrap; the UI that the person actually uses lives in
the payload. Everything below was read from the extracted payload with Craft
Anything's native tools, not guessed from strings.

## Where the real code is

* PE sections end at file offset `0x111c00`; the rest is the embedded
  InstallShield payload: an entry table of UTF-16 name / `Disk1\name` /
  version / size / data with **22 entries**, including `data1.cab`,
  `data1.hdr`, `data2.cab`, `ISSetup.dll`, `setup.inx` and the 14 language INI
  files. `data1.hdr` + `data2.cab` unpack to **263 files**.
* The payload stores three InstallShield cabinets (`ISc(` signature) at file
  offsets `0x1623dc` (6,492,283 B), `0x793457` (89,384 B) and `0x7a917f`
  (42,803,393 B). Renaming them `data1.cab`, `data1.hdr`, `data2.cab` and
  running `unshield x data1.cab` yields the 263 files.
* The payload's `Disk1\setup.exe` entry carries the package version
  **2.0.2026.0**.
* The user-facing program is
  `FFB_Drivers_Win10/amd64/tmeffcpl64.dll`, file version **4.24.2.0**, sha256
  `224da2a9663cf356dfaeae0e3244427f57e8d7a4dc5268d28a6ba451f86dede7`,
  image base `0x180000000`. **All addresses below are in this DLL**; it was
  opened as its own target and every rule was read with
  `native_function_pseudocode` / `native_function_instructions_list` on it.
* The exe's own `.rdata` has no panel strings: `native_strings_find` for
  `Auto-Center` returns 0 matches and for `1080` 0 matches there (only
  `Thrustmaster` at 2 places). That is why the payload DLL is the target that
  matters.

The DLL was re-extracted for this sheet: the three install cabinets were
carved out of the exe at `0x1623dc`, `0x793457` and `0x7a917f`, unpacked with
`unshield`, and `tmeffcpl64.dll` hashed to the same
`224da2a9…86dede7`. Every function below was then read in place with
`native_function_pseudocode` / `native_function_instructions_list`.

### Functions read for this sheet

| function | address | owns |
|---|---|---|
| Test Input page proc | `sub_1800133b0` | angle text/trackbar, LED slider and readout, rim buttons, Default angle |
| Settings page proc | `sub_180005f60` | WM_HSCROLL for every gain slider, Default (1002), LED/HexaLed button, mode notice |
| Settings page init | `sub_18000606d` | trackbar ranges/ticks and opening values |
| Angle getter | `sub_180001a50` | property `0x1038`, the 1080/900 split |
| Angle setter | `sub_180004410` | stores `0x18006586c`, property `0xf0010000` |
| Overall gain setter | `sub_180004bd0` | percent × 100, `0x180065828`, property `0xf0012000` |
| Auto-center switch | `sub_180004750` | property `0x1001` = 3 / 6 / 5 |
| Auto-center boolean | `sub_180004830` | property `0x1024` |
| LED intensity write | `sub_180004a00` | property `0x1021`, caches `0x180065894` |
| LED intensity read | `sub_180001ee0` | cached value, 0 in vendor-API mode |
| LED intensity poll | `sub_180003d00` | property `0x1022` once a second |
| Colour → H/S/L | `sub_18000f0d0` | `0x80ff` → H 148, S 255, L 127 |
| H/S/L → colour | `sub_18000f2c0` | the LED-strip colours |
| Effect preset load | `sub_180004f00` → `sub_180005248` | `FFBFXPath` + `NN_Name.ffe` |

## Control panel window — read from RT_DIALOG

The DLL has exactly **three** dialog resources. Parsed from `.rsrc` (the
numbers are the template's dialog units, not pixels):

| resource | title | size (DLU) | controls | page |
|---|---|---|---|---|
| 106 | `tab1` | 358 × 348 | 37 | Test Input |
| 107 | `Tab2` | 358 × 328 | 21 | Test Forces |
| 118 | `Tab3` | 358 × 321 | 40 | Settings |

Font on all three: `MS Shell Dlg` 8 pt.

Messages come from `CPL_Msg/409.msg` (`IDS_TAB_INPUT 1000` … `IDS_WHIPLASH
1112`, `IDS_MASTER 1200`, `IDS_OVERALL 1201`, `IDS_DETAILGAIN 1202`,
`1203 Constant`, `1204 Periodic`, `1205 Spring`, `1206 Damper`,
`1207 (also affects the weight of the wheel)`, `1208 Auto-Center Settings`,
`1209 by the wheel`, `1210 by the game`, `1211 (Recommended)`,
`1213 This setting will restart the steering wheel and the Control Panel.`,
`1214 Normal Mode`, `1215 Advanced Mode (Default)`,
`1300 The device has been removed! …`, `1400 Always close this CONTROL PANEL
window …`, `1502 Adjustment of the force feedback linearity 0% is fully
linear.`, `1510 Mode (change reactivity and damper)`,
`1511 Comfort`, `1512 Sport`, `1513 Performance`, `1514 Extreme`,
`1520 Test force feedback`, `1600 Warning: risk of injury …`).

### Dialog 106 — Test Input (37 controls)

| id | class | template text | at (x,y) |
|---|---|---|---|
| 1078 | trackbar | — | (83,111) 123×19 |
| 1079 | edit | — | (212,113) 29×13 |
| 1103 / 1104 | static | `1080°` / `40°` | end labels |
| 1087 | static | `Rotation/Angle:` | (90,98) |
| 1003 | button | `Default` | (168,96) |
| 1075 / 1094 / 1095 / 1097 / 1077 | button | `GT Wheel`, `FERRARI 458 Challenge Wheel`, `PS Wheel`, `TM Open Wheel`, `FERRARI F1 Wheel` | rim list, five |
| 1092 / 1093 | button | `Normal Mode` / `Advanced Mode (Default)` | (38,325) / (161,325) |
| 1012 / 1013 | button | `Combined` / `Separate (Default)` | pedals |
| 1105 | button | `Test LED` | (276,109) |
| 1063 | trackbar | — | (272,137) 71×15 |
| 1106 | **static** | `Static` | (293,161) 35×14 |
| 1002 | button | `Check for\nUpdate` | (284,309) |
| 1098 / 1082 / 1030 / 1080 / 1081 / 1026 / 1028 | static | `RIM: <Name>`, `S/N: …`, `Firmware: 0`, `Driver: …`, `Package: …`, `www…`, `ts…` | device block |

Note: **1106 is a static label, not a checkbox.** Its template caption is
`Static`; at run time `sub_1800133b0` overwrites it with `"%d %%    "`
(string 0x18004ad10) from the LED intensity (`0x180015d6b`). There is no
`BM_SETCHECK` for it anywhere.

`1003` is the page's **Default** button (angle reset), not a "Reset" control;
`1002` is "Check for Update".

### Dialog 107 — Test Forces (21 controls)

Description static 1093 (`IDS_DESC_TAB_FORCE 1100`) and twelve buttons, top to
bottom: **1077** Engine, **1003** Blown Tire, **1004** Boing, **1005**
Explosion, **1006** Open Sea, **1007** Turbo Boost, **1008** Gong, **1009**
Bumpy Road, **1010** Car Crash, **1011** Punch, **1012** Force Field, **1013**
Whiplash. (The first button reuses 1077; the rest run 1003–1013.)

### Dialog 118 — Settings (40 controls)

Group boxes: 1094 `Master Gain Settings`, 1096 `Details Gain Settings`,
1107 (BOOST, label 1108 `BOOST (Constant + Spring)`), 1101 `Auto-Center
Settings`.

| slider | label static | template text |
|---|---|---|
| 1063 | 1064 | 1095 `Overall Strength of all forces` |
| 1069 | 1070 | 1097 `Constant Gain` |
| 1071 | 1072 | 1098 `Periodic Gain` |
| 1065 | 1066 | 1099 `Spring Gain` |
| 1067 | 1068 | 1100 `Damper Gain` + `(also affect the weight of the wheel)` |
| 1073 | 1074 | auto-center strength |
| 1113 | 1114 | 1115 `Force Feedback Linearity` |

Radio/check controls: 1075 `by the game (Recommended)`, 1076 `by the wheel`,
1109 `ON`, 1110 `OFF (Default)`. Buttons: 1002 `Default`, 1003 `Test FFB`,
1112 `Enable default HexaLed pattern.` (the message table's `1531` is the
runtime label **Default color and lighting effect**), and **1111 is a combo
box** (class ordinal 133) at (20,24) 115×49 — the Mode list. 1102 is the `version 1.02`
static.

## Rotation / angle — exact rule

The text box is control **1079** (`0x437`), the trackbar **1078** (`0x436`).
Both live in the Test Input dialog proc `sub_1800133b0`:

* **Parser**, `0x180015980–0x180015a30`:

```
0x1800159a5  movzx r9d, word ptr [rdx]   ; UTF-16 character
0x1800159a9  lea   eax, [r9-0x30]
0x1800159ad  cmp   ax, 9                 ; decimal digit?
0x1800159b1  jbe   0x1800159b9
0x1800159b3  cmp   r9d, 0x20             ; or a space?
0x1800159b7  jne   0x1800159de           ; else xor esi,esi / mov ebx,esi (invalid)
0x1800159c2  lea   edi, [rdi+rdi*4]      ; acc * 5
0x1800159c8  lea   edi, [rdi-0x18]       ; - 24
0x1800159cf  lea   edi, [r9+rdi*2]       ; = acc*10 + digit
```

An empty string jumps straight to the clamp with the accumulator still 0, so
it becomes 40. Any other character sets `ebx = 0` (invalid) and the panel then
re-reads the live angle via `sub_180001a50` instead of applying anything
(`0x180015a0b: test ebx,ebx; jne apply`).

* **Clamps** in this order:

```
0x1800159e2  cmp  edi, 0x28      ; 40
0x1800159e7  mov  edi, 0x28
0x1800159ee  mov  eax, 0x438     ; 1080
0x1800159f5  cmovg edi, eax
0x1800159f8  cmp  dword [0x1800657bc], 0
0x180015a01  mov  esi, 0x384     ; 900
0x180015a08  cmovg edi, esi
```

`0x1800657bc` is the **900°-wheel flag**, set by device detection — not the
Normal/Advanced radio.

* **Apply**: `0x180015a1a call 0x180004410` stores the value at `0x18006586c`
  and (vendor path) sets property `0xf0010000`. Then
  `0x180015a1f: mov eax,0x66666667; imul edi; sar edx,2` turns degrees into
  the trackbar position (**/ 10**).
* **Trackbar → degrees** is `angle = pos * 10`
  (`0x180015c85: lea ebx,[rax+rax*4]; add ebx,ebx`, after `TBM_GETPOS` 0x400).
  Position 4 = 40°, 108 = 1080°.
* The trackbar itself is `TBM_SETRANGE` `0x5a0004` (4…90) on a 900° wheel
  and `0x6c0004` (4…108) otherwise (`0x180013de2` / `0x18001401a`), with one
  `TBM_SETTIC` at position **8** = 80° (`0x414` at `0x180013eaa`).
* **Default button** (control **1003**, `0x180015583` compares `rdi` with
  `0x3eb`), branch `0x180015597`:

```
0x180015597  cmp  [0x18006580c], r12d   ; LED variant?
0x1800155a0  mov  eax, [0x180065f0c]    ; device id (property 0xf001f000)
0x1800155ab  cmp  eax, 0xe              ; ids above 14 -> no table hit
0x1800155b0  mov  ecx, 0x4408           ; bits 3, 10, 14
0x1800155b5  bt   ecx, eax
0x1800155f7  mov  esi, 0x168            ; 360
0x1800155a6  mov  esi, 0x384            ; 900
```

Non-LED units test six model flags `0x1800657b4 / 7dc / 7f4 / 810 / 814 /
818` (`0x1800155bc–0x1800155f0`): any set → **360°**, otherwise **900°**. The
trackbar is then set from `angle / 10` with the unsigned magic `0xcccccccd`
+ `shr 3` (`0x180015603–0x18001560d`).

* **Getter** `sub_180001a50`: asks the vendor API for property `0x1038`. If
  `0x1800657bc` (900° flag) is clear it returns **1080** outright
  (`0x180001ae6: mov eax,0x438`); otherwise it clamps the reported value to
  **900** (`cmp ecx,0x384; cmovg ecx,0x384`). Its other path (flag
  `0x180065ae8` set) reads property `0xf0010000`.
* **Opening angle**: 900° on a 900° wheel, 1080° otherwise.

## Gains — exact rule

Slider init is `sub_18000606d`, `0x1800064f5–0x1800065ad`:

```
0x18000652a  mov r9d, 0x640000   ; TBM_SETRANGE (0x406): 0…100
0x18000658a  mov edx, 0x414      ; TBM_SETTIC
0x180006592  mov r8d, 0xa        ; … at 10
0x180006548  mov eax, 0x51eb851f ; device / 100, shr 5, then "%d %%"
```

There is **no `TBM_SETTICFREQ`** in the DLL: one tick is placed at 10 %.

* The wheel value is `percent × 0x64` (100) = **0…10000**; e.g. the overall
  setter `sub_180004bd0` does `eax = ecx * 0x64`, stores it at
  `0x180065828` (settings block `0x180065820` **+0x08**) and applies via
  `sub_180004650`; the vendor path writes property `0xf0012000`.
* Settings block `0x180065820` is 0x58 bytes: **+0x08** overall, **+0x0c**
  spring, **+0x10** damper, **+0x14** periodic, **+0x18** constant, **+0x40**
  auto-center wheel flag, **+0x44** auto-center strength.
* Display is integer `"%d %%"` (string 0x180049dd8) after `/ 100`.
* **Overall cap on the LED variant**: in WM_HSCROLL 1063,
  `0x18000920a–0x180009253` tests `[0x180067af4]` and `[0x18006580c]`; when
  both are set and the value is above `0x46` (70) it is clamped and the
  trackbar reset to 70.

* **Default button** (WM_COMMAND **1002**, handled in `sub_180005f60`),
  decision chain `0x1800080f7–0x18000824e`:

```
0x1800080f7  cmp [0x1800657bc], 0   ; 900° wheel -> 75 %
0x180008104  cmp [0x1800657c8], 0   ; or these three flags
0x180008111  cmp [0x1800657cc], 0
0x18000811e  cmp [0x1800657d0], 0
0x18000822a  mov r9d, 0x4b          ; 75  (label "75 %" 0x180049e40)
0x18000812b  cmp [0x1800657a4], 0   ; T500 family -> 65 %
0x1800081d3  mov r9d, 0x41          ; 65  (label "65 %" 0x180049e50)
0x18000817b  mov edi, 0x3c          ; 60  (label "60 %" 0x180049e60)
```

Then Constant 0x42d, Periodic 0x42f, Spring 0x429, Damper 0x42b are all set
to `0x64` (100) (`0x180008273–0x180008412`); auto-center strength 0x431 is
set to `0x32` (50) and the 1073/1074 sliders disabled
(`0x18000838b–0x1800083e9`); BOOST off (uncheck 1109 `0x455`, check 1110
`0x456`, `sub_1800044c0(0)`); auto-center **by the game** (check 1075
`0x433`, uncheck 1076 `0x434`, `sub_180004480(0)`). Rotation, linearity and
LED intensity are **not** written by this handler.

* **LED variant Default branch** (`0x18006580c != 0`,
  `0x180007d44–0x180007f58`):
  * resets combo 0x457 (`CB_RESETCONTENT` 0x14b), adds strings `0x5e7`…
    `0x5ea` (1511 Comfort, 1512 Sport, 1513 Performance, 1514 Extreme) and
    selects **index 1 = Sport** (`0x14e`, `0x180007e08`);
  * overall **50 %** (`0x180007e19`), label `"%d %%"`;
  * LED colour **0x80ff** (`0x180007e8e`), then `sub_180004940`;
  * `sub_18000f0d0(0x80ff, &h, &s, &l)` at `0x180007e96` decomposes the
    colour. COLORREF is `0x00BBGGRR`, so 0x80ff is **R 0, G 0x80, B 0xff**:
    max 255, min 0, L `(max+min)>>1` = **127**, delta 255,
    S = `delta*255/(2L+1)` = **255**, hue for max=blue
    `170 − 42.5·(G−R)/delta` = **148**. The function scales hue by the
    constant **42.5** (`0x18004a558`, i.e. 255/6) with offsets 0 / 85 / 170
    (`0x18004a560`, `0x18004a578`), and when S > 128 it bends it with
    `pow((S−128)/127, 2/3)·127 + 128` (`0x18004a4f8` = 0.6667,
    `0x18004a590` = 127.0f, `0x18004a568/570` = 127/128).
  * constant **10 %** (`0x180008ea0` clamps 0x42d to 10);
  * auto-center strength **0** (`SendMessage(0x431, TBM_SETPOS, 0)` and
    `sub_1800048e0(0)` at `0x180007fa0`);
  * periodic = `H × 0x15 / 0x100` = **148 × 21 >> 8 = 12 %**,
    spring = `S × 0xb / 0x100` = **255 × 11 >> 8 = 10 %**,
    damper = `L × 0xb / 0x100` = **127 × 11 >> 8 = 5 %**
    (`0x180007eec`, `0x180007f22`, `0x180007f58`);
  * sets `0x180067af4 = 1` (which arms the 70 % overall cap above).
  * The LED-variant flag `0x18006580c` is set when the **device/rim id read
    from property `0x100a` is 12** (`0x180003504` reads 0x100a into
    `[rbp-0x78]`, `0x1800035bb` reloads it, `0x1800039d6: cmp ecx,0xc;
    mov [0x18006580c],eax`), together with `0x1800657f4` and `0x1800657e8`.
    This is **not** a profile type: the earlier reading called it "profile
    type 12", but the profile type comes from property `0x100f` (read at
    `0x1800034c0`), is switched at `0x1800036a6`, and every branch of that
    switch jumps back to the same id chain at `0x180003851`, where the
    `0x100a` value is compared with 1…12. A second setter exists in the HID
    path for PID `0xb696`, which stores the result of a feature-report query
    into the same flag.

### LED variant — moving a gain slider recolours the strip

In `sub_180005f60`'s WM_HSCROLL chain, the LED variant does not only write
the gain: every move of Periodic (0x42f), Spring (0x429) or Damper (0x42b)
recomputes three colours and pushes the first one with `sub_180004940`
(`0x1800090xx–0x1800090b0`, e.g. `0x180008f7c` for 0x42b):

```
p = TBM_GETPOS(0x42f) * 255 / 100      ; magic 0x66666667, sar 2
s = TBM_GETPOS(0x429) * 255 / 100
l = TBM_GETPOS(0x42b) * 255 / 100
sub_18000f2c0(p, s, l,  &0x180067ae8)  ; main strip colour
sub_18000f2c0(p, 255, 128, &0x180067aec)
sub_18000f2c0(p, s, 128, &0x180067af0)
```

So H = Periodic %, S = Spring %, L = Damper %, each scaled to 0…255 — the
three sliders literally *are* the LED colour on an LED-variant wheel.

### Every Settings control and the vendor property it writes

`sub_180005f60` is the one place that turns the Settings controls into
settings.  Each branch calls a small setter; those setters are the source of
the property ids below (all were read with `native_function_pseudocode` and,
for the BOOST immediate, checked with `objdump` at `0x180004502`).

| control | dialog id | setter | vendor property | value written |
|---|---|---|---|---|
| Mode combo | 1111 / `0x457` | `sub_1800045b0` | `0xf001a000` | `CB_GETCURSEL` index 0…3 |
| Default colour/lighting box | 1112 / `0x458` | `sub_1800049a0` | `0xf001e000` | **1 while unchecked, 0 while checked** (`BM_GETCHECK` + `sete` at `0x18000774`) |
| BOOST ON / OFF | 1109 `0x455` / 1110 `0x456` | `sub_1800044c0` | `0x1042` | 1 / 0 |
| Pedals Combined / Separate | 1012 `0x3f4` / 1013 `0x3f5` | `sub_180004c40` | `0xf0011000` | 1 / 0 |
| Auto-centre strength | 1073 `0x431` | `sub_1800048e0` | `0xf0022000` | percent × 100 |
| Constant gain | 1069 `0x42d` | `sub_180004550` | `0xf001b000` | percent × 100 |
| LED colour | (LED Default) | `sub_180004940` | `0xf001d000` | COLORREF |
| Default pattern enable | (unlock path) | `sub_1800046f0` | `0xf001c000` | 1 / 0 |

`sub_1800044c0` is the odd one: it does not use
`tm_api_set_properties`, it builds a 0x28-byte block, stores `0x1042` at
`[rsp+0x24]`, puts the value at `[rsp+0x28]`, and calls vtable `+0xc0`
(`0x180004502`–`0x180004512`).  Its return is `sete` on the call result, so
0 means success.

The same properties are read back when the page opens (`sub_18000606d`):
`sub_180001c30` reads mode `0xf001a000`, `sub_180001bd0` constant
`0xf001b000`, `sub_180001dc0` auto-centre strength `0xf0022000`,
`sub_180001e20` LED colour `0xf001d000`, `sub_180001e80` the pattern flag
`0xf001e000`, and `sub_180001f00` overall `0xf0012000`.

**Opening values on an LED unit are not the Default values.**  The init path
reads the colour, decomposes it and fills the three narrowed trackbars with
`H × 0x13 >> 8`, `S × 0xb >> 8`, `L × 0xb >> 8` (`0x180006f7x`), while the
trackbars are set to 0…18 (periodic, `TBM_SETRANGE 0x120000` at
`0x180006f8f`) and 0…10 (spring/damper, `0xa0000` at `0x180006f9d` /
`0x180006fab`).  For `0x80ff` that is **10 / 10 / 5**, not the Default
branch's **12 / 10 / 5**.  The Default branch uses the 0…100 trackbars and
the 21/11/11 factors described above.

### Test Input's Normal / Advanced radios are not an input-report switch

Controls 1092 `0x444` (`Normal Mode`) and 1093 `0x445`
(`Advanced Mode (Default)`) in `sub_1800133b0` do this (`0x1800158xx`):

1. show `MessageBoxW` with string `0x4bd` = `IDS_CHANGE_BUTTON_STATE 1213`
   ("This setting will restart the steering wheel and the Control Panel.")
   and `MB_OKCANCEL` (`0x31`);
2. if OK, on the older device path call `sub_180004830(1/0)` (property
   `0x1024`), otherwise `sub_180004750(1/0)` (property `0x1001` = 3 by the
   game, 6 by the wheel, 5 on a 900° wheel);
3. `SendMessageW(GetParent(hDlg), WM_CLOSE, 0, 0)` — the panel really closes;
4. put the clicked radio back to checked and the other to unchecked.

So the earlier description ("Normal = reduced button/axis report") was wrong:
the pair changes the wheel's centring mode and restarts the control panel.
There is no code path in 4.24.2.0 that changes the HID report descriptor from
these radios.

### Force Feedback Linearity is a dead control

Dialog 118 has trackbar **1113** (`0x459`) and static **1114** (`0x45a`)
under label 1115 `Force Feedback Linearity`. No instruction in the image
uses either number:

* `objdump -d` over the whole DLL finds **no** `0x459` immediate at all;
* the Settings proc's WM_HSCROLL chain compares `lParam` only against
  `GetDlgItem` of 0x427, 0x42d, 0x42f, 0x429, 0x42b and 0x431
  (`0x180005f60`, `0x1800090xx` region), then falls through to
  `DefWindowProcW`;
* `sub_18000606d` sets ranges for the same six trackbars only.

So the slider repaints but nothing reads or writes it: it changes no
setting in 4.24.2.0. The rebuild shows it disabled with that explanation
instead of inventing a value.

## Auto-center — exact rule

* Default source is **by the game (Recommended)**: radio 1075 (`0x433`)
  checked, 1076 (`0x434`) cleared, `sub_180004480(0)`.
* There is **no auto-center ON/OFF control** in the template. The ON/OFF pair
  1109/1110 sits in group box 1107 next to label 1108
  `BOOST (Constant + Spring)`; it switches BOOST, not auto-center.
* Two vendor-API paths: `sub_180004830` writes property **0x1024** as a
  boolean (`setne`); `sub_180004750` writes property **0x1001** with a mode:
  argument non-zero → **3** (by the game); zero → **6** (by the wheel), or
  **5** on a 900° device (`test ebx,ebx; … mov ecx,6 / stack_78 = 5`).
* Auto-Center strength is slider 1073, default 50 %, disabled while the game
  owns centring.

## Test Forces — the twelve presets

`sub_180004f00` copies the twelve UTF-16 names at `0x180049c10 … 0x180049d48`
(24-byte slots: `01_Engine` … `12_Whiplash`) and tail-calls
`sub_180005248`, which:

* reads `SOFTWARE\Thrustmaster\T500 RS Racing wheel` → `FFBFXPath`, appends
  `\` if missing, the name, and `.ffe`;
* indexes the slot as `index × 0x50` (`lea rcx,[rbx+rbx*4]; shl 4`);
* hands the path to the DirectInput effect loader.

Each `.ffe` is a RIFF `FORC` file. A block is a `DIEFFECT` (`dwSize` 0x38):
`dwDuration` at +0x08 (µs), `dwGain` at +0x10, effect GUID at +0x38
(…C22 square, …C23 sine, …C24 triangle, …C20 constant, …C28 damper), 16-byte
`DIPERIODIC` data (magnitude, offset, phase, period in µs) and a 20-byte
envelope chunk. Values parsed from the extracted files:

| # | preset file | button | duration | gain |
|---|---|---|---|---|
| 1 | `01_Engine.ffe` | Engine | 2.20 s (+3.70 s second block) | 10000 |
| 2 | `02_BlownTire.ffe` | Blown Tire | 1.25 s (+0.90 s constant) | 10000 |
| 3 | `03_Boing.ffe` | Boing | 0.70 s | 10000 |
| 4 | `04_Explosion.ffe` | Explosion | 2.65 / 2.90 / 0.85 s | 7000 / 7000 / 10000 |
| 5 | `05_OpenSea.ffe` | Open Sea | 4.55 s | 10000 |
| 6 | `06_TurboBoost.ffe` | Turbo Boost | 1.25 s (+0.25 s) | 7000 |
| 7 | `07_Gong.ffe` | Gong | 4.50 s (+0.125 s) | 7000 |
| 8 | `08_BumpyRoad.ffe` | Bumpy Road | 1.50 s (+1.00 s) | 10000 |
| 9 | `09_CarCrash.ffe` | Car Crash | 2.70 / 2.80 / 1.50 s | 10000 |
| 10 | `10_Punch.ffe` | Punch | 0.25 s (+0.75 s damper) | 10000 |
| 11 | `11_ForceField.ffe` | Force Field | 1.00 s (+0.50 s) | 10000 |
| 12 | `12_Whiplash.ffe` | Whiplash | 1.00 s (+1.50 s) | 10000 |

Every preset opens with a Periodic block on one axis; Blown Tire adds a
Constant block, Punch adds a Damper, Car Crash adds a third Triangle block.

## Test Input — LED and device block

* LED trackbar is 1063 (0…100, tick 10). Its readout is the **static 1106**,
  filled with `"%d %%    "` from the value cached at `0x180065894`
  (`sub_180001ee0` returns that cache; 0 in vendor-API mode
  `0x180065ae8 != 0`).
* Opening value comes from device property **0x1022**
  (`sub_1800020e0` @ `0x180003ab1` reads it into `0x180003ae0
  mov [0x180065894], eax`); writes go to **0x1021** (`sub_180004a00`,
  which also caches the value).
* **There is no panel-side LED literal.** `0x180065894` sits in `.bss`
  (Craft Anything cannot read a u32 there — it has no file bytes), so it is
  zero until the wheel answers. A one-second timer (`SetTimer(hDlg, 2,
  0x3e8, 0)` in `sub_1800133b0`) calls `sub_180003d00`, which re-reads
  property `0x1022` and returns 1 when the value changed. With no wheel the
  page therefore shows **0 %**, not a stored default.
* Rim list, five entries: 1097 `TM Open Wheel`, 1095 `PS Wheel`, 1075
  `GT Wheel`, 1094 `FERRARI 458 Challenge Wheel`, 1077 `FERRARI F1 Wheel`.
* `Normal Mode` 1092 / `Advanced Mode (Default)` 1093.
* `Combined` 1012 / `Separate (Default)` 1013.
* Device block: `RIM:` 1098, `S/N:` 1082, `Firmware:` 1030, `Driver:` 1080,
  `Package:` 1081, `www…` 1026, `ts…` 1028.

## Opening values — read from the wheel, not typed in

The Settings page init `sub_18000606d` does **not** open the sliders on the
Default button's numbers. It calls one getter per trackbar and sets the
position from the result (`SendMessage(0x405, TBM_SETPOS, value)`), dividing
by 100 for the display:

| slider | control | getter | cached block |
|---|---|---|---|
| Overall Strength | 0x427 / 1063 | `sub_180001f00` (property `0xf0012000`) | `0x180065828` +0x08 |
| Constant Gain | 0x42d / 1069 | `sub_180001bb0` | `0x180065838` +0x18 |
| Periodic Gain | 0x42f / 1071 | `sub_1800020a0` | `0x180065834` +0x14 |
| Spring Gain | 0x429 / 1065 | `sub_1800020c0` | `0x18006582c` +0x0c |
| Damper Gain | 0x42b / 1067 | `sub_180001c90` | `0x180065830` +0x10 |
| Auto-Center Strength | 0x431 / 1073 | `sub_180001b20` | `0x180065864` |

So a wheel that was set somewhere else (a game, an older run, the Default
button) opens showing **its** values.

The Test Input angle is the exception: that page opens on the class default
(900° on a 900° wheel, 1080° otherwise) and only calls the angle getter
`sub_180001a50` when the typed text is not a number. On Linux the driver's
`range` attribute is just as readable, so the rebuild also mirrors it at open;
otherwise a range a game had changed would be silently overwritten by the next
Apply while the page still claimed 900°.

On Linux `hid-t150` exposes four of those settings, so the rebuild mirrors them
when a wheel appears (and on Rescan, never over unsaved edits):

| panel value | hid-t150 attribute | mapping |
|---|---|---|
| Rotation | `range` | degrees, 40…1080 |
| Overall Strength | `gain` | 0…65535 → 0…100 % |
| Auto-Center Strength | `autocenter` | 0…65535 → 0…100 % |
| Auto-Center source | `enable_autocenter` | `y` → by the wheel, `n` → by the game |

Constant, periodic, spring and damper have no hid-t150 attribute, so they keep
the documented defaults. The read is `device::read_settings`; the page copies
it in `Panel::adopt_wheel_settings`. Reading writes nothing — only Apply/OK
and the Test buttons touch the wheel.

## Setup wizard

InstallShield property-sheet templates in the launcher: classic pages
**332 × 218** (RT_DIALOG 103/105/106/125) with the blue banner; modern pages
**460 × 305** (127/128/129/130, 3003/3004). `setup.ini`: `Product=Thrustmaster
FFB Racing Wheel`, `Default=0x0409`, **14 supported languages**, `ScriptDriven=4`,
`CheckMD5=Y`, `SmallProgress=N`. `0x0409.ini` holds `1125 Choose Setup
Language`, `1126 Select the language …`, `1205 /S Hide initialization dialog…`,
`1624 Preparing to Install...`. The wizard strings are resource blobs decoded
by the InstallShield runtime at run time; `0x44bf50` is only that runtime's
bootstrap (`sub_44c61e`, `sub_44b608`, `sub_44c6cb`).

## Real force feedback on Linux

The original sends each preset to the wheel as a DirectInput effect. On Linux
the same 12 buttons upload a periodic effect through the kernel's evdev
force-feedback interface (`app/src/ffb.rs`):

* the joystick node (`/dev/input/js*`) cannot carry effects, so the panel finds
  the sibling `/dev/input/event*` node through sysfs and opens it read/write;
* `EVIOCSFF` uploads a periodic effect and `EV_FF` starts it; its replay length
  is the preset duration and its magnitude is
  `dwGain / 10000 × Overall Strength`;
* `FF_GAIN` carries Overall Strength even in Xbox mode; `FF_AUTOCENTER`
  carries the auto-centre strength when "by the wheel" is selected;
* the connected TMX reports 16 effect slots and supports rumble, periodic,
  square, triangle, sine and gain.

A part's own duration is capped at the preset's headline duration so one Test
Forces press is one run on screen. `hid-t150` advertises sine, constant, spring
and damper, but not square or triangle; its upload packet maps both of those to
sine. Car Crash and Force Field are built only from square/triangle parts, so
the panel now sends those as sine when the wheel does not list them; otherwise
both presets were skipped completely and the wheel stayed still. Parts that use
a force the wheel truly lacks are still skipped and the card says so.

Blown Tire was the last silent preset: its constant part reached the wheel, but
`t150_ff_prepare_update` scales a constant by `sin(direction)`
(`level = level * fixp_sin16(direction / (0xFFFF / 360)) >> 15`).  With the
evdev default direction 0 the sine is 0, so the whole -10000 part was zero and
only the preset's 8.5 % square remained (and that one is sent as sine on this
wheel).  Constant effects therefore now use direction `0x4000` (90°), which
leaves the level unchanged; periodic parts keep direction 0 because that driver
path ignores it.

## Installer shortcut

The Windows installer registers Start-menu shortcuts. The rebuild's Finish
page runs `desktop/install.sh` (or writes a `.desktop` file for the running
executable) when the box is ticked.

## What the rebuild now uses

`app/src/mechanics.rs` holds every number above with a comment pointing at the
function/address; `app/src/panel.rs` and `app/src/device.rs` call it:

* angle entry parses digits/spaces, clamps **40…1080** and **900** on a
  900° wheel (`device::is_900_degree`, the original's `0x1800657bc`); it
  snaps to 10° steps, opens on 900° (1080° on a full-range wheel) and reads
  the live value back when the text is not a number;
* the angle button on Test Input is labelled **Default**, as control 1003's
  template text is, and writes 900° (360° on the flagged models / LED ids
  3, 10, 14) — `mechanics::reset_angle_deg` now models the two real branches
  (LED-variant mask vs the six model flags) instead of treating the 900° flag
  as the LED flag;
* gains are integer 0…100, displayed `N %`, wheel value `percent × 100`;
  the page opens on the wheel's live `range`/`gain`/`autocenter` (the
  original's `sub_18000606d` getter calls), and Default sets
  **75 / 100 / 100 / 100 / 100**, auto-center strength **50**, boost off,
  auto-center **by game**, leaving rotation and linearity alone;
* **LED 1106 is now a static percentage readout**, matching the template's
  `Static` placeholder and the `%d %%` fill at `0x180015d6b`; it is no longer
  drawn as a checkbox;
* the LED slider opens on **0 %**, which is what `0x180065894` really holds
  before the wheel answers property `0x1022` (it is `.bss`, zero at load);
  the page still shows the value the device poll `sub_180003d00` caches;
* the Settings page has the **Mode combo** (Comfort / Sport / Performance /
  Extreme, opening on **Sport** — the LED Default's index 1) with the
  original's notice "This setting will restart the steering wheel and the
  Control Panel." when it changes, and it reports the value it would send:
  property `0xf001a000` = the selected index;
* **control 1112** is now present as the **Default colour and lighting
  effect** checkbox, including the original's inverted write
  (`0xf001e000` = 1 while unchecked);
* the auto-center card exposes the strength slider and shows the real
  property values (0x1001 = 3/5/6, 0x1024 boolean, strength
  `0xf0022000` = percent × 100);
* **Normal / Advanced** on Test Input now models the real behaviour
  (property `0x1001` = 3 / 6 / 5 after the 1213 notice and a panel restart),
  not the invented reduced input report;
* BOOST is its own group with ON / OFF (Default) radios and reports property
  `0x1042` = 1 / 0;
* Combined / Separate pedals report property `0xf0011000` = 1 / 0;
* Cancel restores the opening gain snapshot (`sub_180003e00` on PSN_RESET);
* **Force Feedback Linearity** is shown disabled and labelled as not connected
  in 4.24.2.0 (no code reads 0x459/0x45a), instead of pretending to set
  something;
* the twelve effect buttons use the real durations, gains, waveforms, periods
  and envelopes from the `.ffe` files and play them through evdev when the
  wheel advertises periodic force feedback, with Overall Strength as
  `FF_GAIN`;
* the footer shows the DLL's real file version **4.24.2.0** and the package's
  **2.0.2026.0**;
* `mechanics::color_to_hsl` reproduces `sub_18000f0d0` (42.5 hue scale,
  2L+1 saturation, the >128 gamma bend) and `mechanics::led_default_detail_gains`
  returns the LED Default's **12 / 10 / 5 %** from colour 0x80ff, while
  `mechanics::led_opening_detail_gains` returns the page-open values
  **10 / 10 / 5** from the init path's 19/11/11 factors; the Settings rules
  card prints both sets, the H/S/L triple, the 0…18 / 0…10 trackbar limits,
  and the gain→strip-colour rule.

Deliberately different, because the Windows driver stack cannot run on Arch
Linux: settings that only the vendor driver exposes fall back to a simulated
wheel when no device is present; the setup writes a menu shortcut rather than
Windows drivers and firmware; the banner art is redrawn, not copied; two
languages instead of fourteen.

The bundled `hid-t150` driver (`app/driver/t150`) accepts the original's
**40**…900/1080 range (the vendor's limit, not the Linux driver's 270…900) and
still builds against the running 7.3.1 CachyOS kernel (`make` with `LLVM=1`).

## Official TMX data (Thrustmaster support page)

Read from
<https://support.thrustmaster.com/en/product/tmxforcefeedback-en/> on
2026-11-03, to settle values the DLL cannot name:

* the page's warning states the wheel's firmware is shown in the Control
  Panel's top right and that **Firmware 20 is the latest version** — the
  `hid-t150` `firmware_version` attribute on the connected TMX reports the
  same 20, so the update dialog compares against 20, not the T500's V47;
* the current PC package is **`2026_TTRS_2.exe`** (version 2.0.2026.0), which
  is the package this project was rebuilt from;
* the game notes describe the TMX as a **900°** wheel ("automatic re-centring
  … through 900°"), matching `device::is_900_degree` and the 900° cap above.

`mechanics.rs` keeps `TMX_LATEST_FIRMWARE`, `TMX_DRIVER_PACKAGE` and
`TMX_SUPPORT_URL`; the update dialog and the device card use them, and the
device card links straight to the official page.

## Still open (recorded as unknowns in Craft Anything)

* **Which wheel or rim reports device id `0x100a` = 12.** The LED-variant
  flag `0x18006580c` is set by that comparison (`0x1800039d6`), and the panel
  never names the value; the vendor driver's profile table
  (`tmHidUsb.sys`, `.data` at `0x1400861e0`, 28-byte records of
  vendor/PID/…/family) maps each product id to a family (T150 `0xb677` → 6,
  TMX `0xb67f` → 7) but does not contain the `0x100a` id. Linux exposes no
  equivalent property through `hid-t150`, so the rebuild documents and
  computes the LED-variant Default branch but cannot decide when to enter it.
  The earlier note said "profile type 12"; the instruction trace above shows
  the compared value is the `0x100a` id, not the profile type.
* **LED intensity value with a real wheel.** The panel only mirrors property
  `0x1022`; what a T150/TMX sends there was not measured on hardware, so the
  rebuild shows the zero-initialised value 0 until a device reports one.
