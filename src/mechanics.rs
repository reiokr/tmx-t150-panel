//! Exact rules lifted from the original `tmeffcpl64.dll` (4.24.2.0).
//!
//! Every constant here has an address in `MECHANICS.md`.  Keep this module the
//! single source of truth: the UI must not invent its own limits.

/// `sub_1800133b0` @ 0x1800159e2: `cmp edi,0x28` / `cmovg edi,0x438`.
pub const ANGLE_MIN_DEG: i32 = 40;
pub const ANGLE_MAX_DEG: i32 = 1080;
/// `sub_180001a50` @ 0x180001aef: `cmp ecx,0x384` / `cmovg ecx,0x384`.
pub const ANGLE_MAX_DEG_LIMITED: i32 = 900;
/// Dialog 106 control 1003 — the page's **Default** button (its template text
/// is literally `Default`), branch at 0x180015597 in `sub_1800133b0`.
/// Non-LED units write 900°, or `mov esi,0x168` (360°) when one of the six
/// model flags 0x1800657b4/7dc/7f4/810/814/818 is set (0x1800155a0–0x1800155f7).
pub const ANGLE_RESET_SMALL: i32 = 360;
/// LED units (`0x18006580c`) index a 15-bit mask with the device id
/// (`mov ecx,0x4408; bt ecx,eax` @ 0x1800155b0); ids above 0xe are rejected
/// by `cmp eax,0xe` @ 0x1800155ab.  Bits 3, 10 and 14 are set.
pub const LED_DEVICE_ID_MASK_360: u32 = 0x4408;
pub const LED_DEVICE_ID_MAX: u32 = 0x0e;
/// The angle trackbar stores the angle divided by ten
/// (`lea eax,[rax+rax*4]; add eax,eax` @ 0x180015c85).
pub const ANGLE_TRACKBAR_DIVISOR: i32 = 10;

/// Trackbar range is built with `TBM_SETRANGE` 0x640000 (0…100) @
/// 0x18000652a, and a single `TBM_SETTIC` (0x414) is placed at 10 % @
/// 0x18000658a.  There is no TBM_SETTICFREQ call in the DLL.
pub const GAIN_MIN_PCT: i32 = 0;
pub const GAIN_MAX_PCT: i32 = 100;
pub const GAIN_TICK_PCT: i32 = 10;
/// Every setter writes `percent * 0x64` to the wheel (`sub_180004bd0` etc.).
pub const GAIN_DEVICE_SCALE: i32 = 100;

/// `sub_180005f60` @ 0x1800081xx: on the LED variant the overall slider is
/// clamped to 0x46 (70) when 0x180067af4 and 0x18006580c are both set.
pub const OVERALL_LED_CAP_PCT: i32 = 70;

/// Official TMX Force Feedback data, read from Thrustmaster's product support
/// page on 2026-11-03:
/// <https://support.thrustmaster.com/en/product/tmxforcefeedback-en/>.
///
/// The page's warning says the wheel's firmware appears in the top right of
/// the Control Panel and that **Firmware 20 is the latest version**.  It also
/// lists the current PC package as `2026_TTRS_2.exe` and describes the TMX as
/// a 900° wheel (its game notes use the automatic re-centring "through 900°").
/// The local hid-t150 driver reports the same firmware byte, so the update
/// dialog now compares against this value instead of the T500's V47.
pub const TMX_LATEST_FIRMWARE: u8 = 20;
pub const TMX_DRIVER_PACKAGE: &str = "2026_TTRS_2";
pub const TMX_SUPPORT_URL: &str =
    "https://support.thrustmaster.com/en/product/tmxforcefeedback-en/";

/// `DIEFFECT.dwGain` is 0..10000 in every `CPL_effects/*.ffe` preset.
pub const EFFECT_GAIN_FULL: u16 = 10_000;

/// Default button (`sub_180005f60`, WM_COMMAND 1002, control 0x3ea).
/// The literals are referenced at 0x180008160 ("60 %"), 0x1800081b3
/// ("65 %"), 0x18000820a ("75 %"), 0x180008266… ("100 %") and
/// 0x18000837e ("50 %").  Rotation, linearity and LED intensity are not
/// written by this handler.
pub const DEFAULT_OVERALL_GENERIC_PCT: i32 = 60;
pub const DEFAULT_OVERALL_T500_PCT: i32 = 65;
pub const DEFAULT_OVERALL_LIMITED_PCT: i32 = 75;
pub const DEFAULT_DETAIL_GAIN_PCT: i32 = 100;
pub const DEFAULT_AUTOCENTER_STRENGTH_PCT: i32 = 50;
/// Newer LED variant branch of the same handler (`0x18006580c != 0`).
///
/// `sub_180005f60` @ 0x180007d44: overall 0x32 (50 %), LED colour 0x80ff
/// (0x180007e89), constant 10 % (`SendMessageW(0x42d, 0x405, 1, 10)`), auto-centre
/// strength 0, and the other three sliders derived from the colour's H/S/L
/// components (0x18000f0d0) with the fixed-point factors below.
pub const DEFAULT_OVERALL_LED_PCT: i32 = 50;
pub const DEFAULT_CONSTANT_LED_PCT: i32 = 10;
pub const LED_DEFAULT_COLOR: u32 = 0x80ff;
/// Combo 0x457 (control 1111) is reset and index 1 selected @ 0x180007e08
/// (`CB_SETCURSEL` 0x14e with wParam 1) after adding string ids
/// 0x5e7..0x5ea = IDS_DDRIVE_MODE_1..4 (Comfort, Sport, Performance, Extreme).
pub const LED_DEFAULT_MODE_INDEX: usize = 1;
pub const MODE_LABELS: [&str; 4] = ["Comfort", "Sport", "Performance", "Extreme"];
/// `0x180008bd8`…`0x180008cdb`: changing the mode asks
/// IDS_CHANGE_BUTTON_STATE 1213 "This setting will restart the steering wheel
/// and the Control Panel." before applying.
pub const MODE_CHANGE_NOTICE: &str =
    "This setting will restart the steering wheel and the Control Panel.";

/// Fixed-point factors from the LED Default branch: `imul eax, …, 0x15`
/// (21) then `sar 8` for periodic, and `imul eax, …, 0xb` (11) then `sar 8`
/// for spring and damper (0x180007eec, 0x180007f22, 0x180007f58).  The
/// components are the 0…255 outputs of the colour decomposition
/// `sub_18000f0d0(0x80ff, &h, &s, &l)`.
pub const LED_PERIODIC_FACTOR: i32 = 0x15;
pub const LED_SPRING_FACTOR: i32 = 0x0b;
pub const LED_DAMPER_FACTOR: i32 = 0x0b;

/// Opening the Settings page on an LED unit is a different path from the
/// Default button (`sub_18000606d`, 0x180006fxx): it reads the colour with
/// `sub_180001e20` (property 0xf001d000), decomposes it and then uses
/// **19** for periodic (`imul eax,0x13; sar 8` @ 0x180006f7x) and 11 for
/// spring/damper.  The three trackbars are narrowed to 0…18 (periodic,
/// `TBM_SETRANGE 0x120000`) and 0…10 (spring/damper, `0xa0000`), so the
/// opening values are smaller than the Default branch's 12 / 10 / 5.
pub const LED_OPENING_PERIODIC_FACTOR: i32 = 0x13;
pub const LED_OPENING_SPRING_FACTOR: i32 = 0x0b;
pub const LED_OPENING_DAMPER_FACTOR: i32 = 0x0b;
pub const LED_PERIODIC_MAX: i32 = 18;
pub const LED_SPRING_MAX: i32 = 10;
pub const LED_DAMPER_MAX: i32 = 10;

/// Angle trackbar: `TBM_SETRANGE 0x5a0004` (4…90) on a 900° wheel and
/// `0x6c0004` (4…108) otherwise, with one tick at position 8
/// (`TBM_SETTIC` 0x414 @ 0x180013de2/0x18001401a, 0x180013eaa).
pub const ANGLE_TICK_POSITION: i32 = 8;
pub const ANGLE_TRACKBAR_MIN_POS: i32 = 4;
pub const ANGLE_TRACKBAR_MAX_900_POS: i32 = 90;
pub const ANGLE_TRACKBAR_MAX_1080_POS: i32 = 108;

/// `sub_18000f0d0` decomposes `LED_DEFAULT_COLOR` into 0…255 H/S/L.
///
/// COLORREF is `0x00BBGGRR`, so 0x80ff is R 0, G 0x80, B 0xff.  The function
/// halves max+min for L, uses `delta*255/(2L+1)` for S (with the
/// `pow((S-128)/127, 2/3)*127+128` bend above 128) and a 42.5-step hue with
/// offsets 0/85/170.  For 0x80ff this gives **H 148, S 255, L 127**, so the
/// LED Default branch lands on periodic 12 %, spring 10 %, damper 5 %.
pub fn color_to_hsl(color: u32) -> (i32, i32, i32) {
    let b = (color & 0xff) as i32;
    let g = ((color >> 8) & 0xff) as i32;
    let r = ((color >> 16) & 0xff) as i32;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) >> 1;
    let delta = max - min;
    if delta == 0 {
        return (0, 0, l);
    }
    let mut s = delta * 255 / (2 * l + 1);
    if s > 0x80 {
        let x = (s - 128) as f64 / 127.0;
        s = (x.powf(2.0 / 3.0) * 127.0 + 128.0) as i32;
    }
    let d = delta as f64;
    let h = if max == r {
        let mut v = 42.5 * (g - b) as f64 / d;
        if v < 0.0 {
            v += 255.0;
        }
        v
    } else if max == g {
        85.0 + 42.5 * (b - r) as f64 / d
    } else {
        170.0 + 42.5 * (r - g) as f64 / d
    };
    (h as i32, s, l)
}

/// The three detail gains the LED Default branch writes
/// (`sub_180005f60` @ 0x180007eec–0x180007f58): component × factor >> 8.
pub fn led_default_detail_gains() -> (i32, i32, i32) {
    let (h, s, l) = color_to_hsl(LED_DEFAULT_COLOR);
    (
        h * LED_PERIODIC_FACTOR >> 8,
        s * LED_SPRING_FACTOR >> 8,
        l * LED_DAMPER_FACTOR >> 8,
    )
}

/// The values `sub_18000606d` puts in the three LED-unit trackbars when the
/// page opens, from the colour read back through `sub_180001e20`.
pub fn led_opening_detail_gains() -> (i32, i32, i32) {
    let (h, s, l) = color_to_hsl(LED_DEFAULT_COLOR);
    (
        h * LED_OPENING_PERIODIC_FACTOR >> 8,
        s * LED_OPENING_SPRING_FACTOR >> 8,
        l * LED_OPENING_DAMPER_FACTOR >> 8,
    )
}

/// `0x180065894` is in `.bss`, so the LED slider opens on **0 %**; the wheel
/// pushes the real value through property `0x1022` (`sub_180003d00`).
pub const LED_INTENSITY_INITIAL_PCT: i32 = 0;

/// Dialog 118 trackbar 1113 (`0x459`) and its static 1114 (`0x45a`) exist in
/// the template but **no instruction in the image uses either id**: the
/// Settings WM_HSCROLL chain compares only 0x427/0x42d/0x42f/0x429/0x42b/0x431.
pub const LINEARITY_CONTROL_ID: u32 = 1113;
pub const LINEARITY_LABEL_ID: u32 = 1114;
pub const LINEARITY_CONNECTED: bool = false;

/// `sub_180004750` @ 0x1800047xx writes vendor property 0x1001.
pub const AUTOCENTER_MODE_GAME: u32 = 3;
pub const AUTOCENTER_MODE_WHEEL: u32 = 6;
pub const AUTOCENTER_MODE_WHEEL_900: u32 = 5;

/// `sub_180004830` writes vendor property 0x1024 as a boolean.
pub const AUTOCENTER_PROPERTY: u32 = 0x1024;
pub const AUTOCENTER_MODE_PROPERTY: u32 = 0x1001;

/// The rest of the vendor properties the Settings page writes.  Each setter
/// is a separate function in the DLL; the addresses are in `MECHANICS.md`.
/// These are Windows-only (`tm_api_set_properties` / the COM vendor API), so
/// the Linux rebuild stores the choice and shows the exact property value
/// instead of pretending hid-t150 has an attribute for it.
pub const MODE_PROPERTY: u32 = 0xf001a000; // sub_1800045b0
pub const HEXALED_PROPERTY: u32 = 0xf001e000; // sub_1800049a0
pub const HEXALED_UNCHECKED_VALUE: u32 = 1;
pub const BOOST_PROPERTY: u32 = 0x1042; // sub_1800044c0
pub const PEDAL_MODE_PROPERTY: u32 = 0xf0011000; // sub_180004c40
pub const AUTOCENTER_STRENGTH_PROPERTY: u32 = 0xf0022000; // sub_1800048e0
pub const CONSTANT_GAIN_PROPERTY: u32 = 0xf001b000; // sub_180004550
pub const LED_COLOR_PROPERTY: u32 = 0xf001d000; // sub_180004940
pub const LED_PATTERN_PROPERTY: u32 = 0xf001c000; // sub_1800046f0

/// `sub_180005f60` WM_COMMAND 0x458 (control 1112) does
/// `BM_GETCHECK` and then `sete`, so it writes **1 while the box is
/// unchecked** and 0 while it is checked (`sub_1800049a0`).
pub fn hexaled_property_value(checked: bool) -> u32 {
    if checked {
        0
    } else {
        1
    }
}

/// `sub_180005f60` WM_COMMAND 0x3f4 (Combined) calls `sub_180004c40(1)`;
/// 0x3f5 (Separate) calls `sub_180004c40(0)`.
pub fn pedal_mode_property_value(separate: bool) -> u32 {
    if separate {
        0
    } else {
        1
    }
}

/// `sub_180005f60` WM_COMMAND 0x455 (BOOST ON) calls `sub_1800044c0(1)`,
/// 0x456 (OFF) calls `sub_1800044c0(0)`; the setter writes property 0x1042.
pub fn boost_property_value(on: bool) -> u32 {
    if on {
        1
    } else {
        0
    }
}

/// `sub_180005f60` WM_COMMAND 0x457: `CB_GETCURSEL` then
/// `sub_1800045b0(index)`, which writes property 0xf001a000.
pub fn mode_property_value(index: usize) -> u32 {
    index.min(MODE_LABELS.len() - 1) as u32
}

/// A wheel class the panel can meet.  The original selects defaults from the
/// same product flags it uses for the 900-degree cap.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WheelClass {
    Generic,
    T500Family,
    Limited900,
}

/// `sub_1800133b0` 0x1800159e2 and `sub_180001a50` 0x180001ae6.
///
/// The 900 limit is the *wheel's* flag `0x1800657bc` (set for 900° units such
/// as the T150/TMX), not the Normal/Advanced mode radio.
pub fn clamp_angle(deg: i32, limited_900: bool) -> i32 {
    let mut out = if deg < ANGLE_MIN_DEG {
        ANGLE_MIN_DEG
    } else if deg > ANGLE_MAX_DEG {
        ANGLE_MAX_DEG
    } else {
        deg
    };
    if limited_900 && out > ANGLE_MAX_DEG_LIMITED {
        out = ANGLE_MAX_DEG_LIMITED;
    }
    out
}

/// Angle shown when the Test Input page opens.
///
/// `sub_180001a50` reads property 0x1038, but returns 0x438 (1080) outright
/// when the 900° flag 0x1800657bc is clear (`0x180001ae6: mov eax,0x438`).
/// A 900° wheel reports its own value, clamped to 900.
pub fn opening_angle_deg(limited_900: bool) -> i32 {
    if limited_900 {
        ANGLE_MAX_DEG_LIMITED
    } else {
        ANGLE_MAX_DEG
    }
}

/// Angle written by the page's Default button (dialog 106, control 1003).
///
/// The handler branches on the **LED-variant** flag `0x18006580c`, not on the
/// 900° flag:
///
/// * LED variant — read the device id from `0x180065f0c` (property
///   0xf001f000), reject above 0xe (`cmp eax,0xe` @ 0x1800155ab) and test
///   bit `id` of `0x4408`: ids 3, 10 and 14 get 360°, every other id 900°.
/// * otherwise — 900°, or 360° when any of the six model flags
///   0x1800657b4 / 7dc / 7f4 / 810 / 814 / 818 is set (0x1800155bc–0x1800155f5).
///
/// The 360° literal is `mov esi,0x168` @ 0x1800155f7 and the 900° literal is
/// `mov esi,0x384` @ 0x1800155a6 / 0x1800155f0.
pub fn reset_angle_deg(led_variant: bool, led_device_id: Option<u32>, flagged_model: bool) -> i32 {
    if led_variant {
        match led_device_id {
            Some(id) if id <= LED_DEVICE_ID_MAX && (LED_DEVICE_ID_MASK_360 >> id) & 1 == 1 => {
                ANGLE_RESET_SMALL
            }
            _ => ANGLE_MAX_DEG_LIMITED,
        }
    } else if flagged_model {
        ANGLE_RESET_SMALL
    } else {
        ANGLE_MAX_DEG_LIMITED
    }
}

/// Parse what the original's edit control parser accepts: decimal digits,
/// spaces ignored.  It stops at the first other character and marks the input
/// invalid (`xor esi,esi; mov ebx,esi` @ 0x1800159de).  An empty string is
/// *valid* and parses as zero, which the caller then clamps to 40.
pub fn parse_angle_text(text: &str) -> Option<i32> {
    let mut value: i64 = 0;
    for ch in text.chars() {
        if ch == ' ' {
            continue;
        }
        if let Some(d) = ch.to_digit(10) {
            value = value * 10 + d as i64;
            // The target stores the accumulator in a 32-bit register.
            value = (value as i32) as i64;
        } else {
            return None;
        }
    }
    Some(value as i32)
}

/// Trackbar position -> degrees.  `sub_1800133b0` uses `pos * 10`
/// (`lea eax,[rax+rax*4]; add eax,eax`), so the slider step is ten degrees.
pub fn slider_to_angle(position: i32) -> i32 {
    position * ANGLE_TRACKBAR_DIVISOR
}

/// Degrees -> trackbar position, truncating like the original (`imul` magic
/// 0x66666667 / 10 at 0x180015a1f).
pub fn angle_to_slider(deg: i32) -> i32 {
    deg / ANGLE_TRACKBAR_DIVISOR
}

pub fn clamp_gain(pct: i32) -> i32 {
    pct.clamp(GAIN_MIN_PCT, GAIN_MAX_PCT)
}

/// One slider percent becomes 100 device units (`imul ecx,0x64`).
pub fn device_gain_units(pct: i32) -> i32 {
    clamp_gain(pct) * GAIN_DEVICE_SCALE
}

/// The panel divides the device value by 100 before showing `N %`
/// (magic 0x51EB851F, shift 5).
pub fn display_gain_pct(device_units: i32) -> i32 {
    device_units / GAIN_DEVICE_SCALE
}

/// Default overall value for the wheel class (`sub_180005f60`).
pub fn default_overall_pct(class: WheelClass) -> i32 {
    match class {
        WheelClass::T500Family => DEFAULT_OVERALL_T500_PCT,
        WheelClass::Limited900 => DEFAULT_OVERALL_LIMITED_PCT,
        WheelClass::Generic => DEFAULT_OVERALL_GENERIC_PCT,
    }
}

/// Auto-center property value written on close (`sub_180004750`).
pub fn autocenter_mode(by_wheel: bool, limited_900: bool) -> u32 {
    if !by_wheel {
        AUTOCENTER_MODE_GAME
    } else if limited_900 {
        AUTOCENTER_MODE_WHEEL_900
    } else {
        AUTOCENTER_MODE_WHEEL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_like_the_original() {
        assert_eq!(clamp_angle(0, false), 40);
        assert_eq!(clamp_angle(39, false), 40);
        assert_eq!(clamp_angle(40, false), 40);
        assert_eq!(clamp_angle(700, false), 700);
        assert_eq!(clamp_angle(1080, false), 1080);
        assert_eq!(clamp_angle(1081, false), 1080);
        assert_eq!(clamp_angle(1080, true), 900);
        assert_eq!(clamp_angle(900, true), 900);
        assert_eq!(clamp_angle(899, true), 899);
        assert_eq!(clamp_angle(0, true), 40);
    }

    #[test]
    fn parses_like_the_original() {
        assert_eq!(parse_angle_text("1080"), Some(1080));
        assert_eq!(parse_angle_text(" 90 0 "), Some(900));
        assert_eq!(parse_angle_text(""), Some(0));
        assert_eq!(parse_angle_text("   "), Some(0));
        assert_eq!(parse_angle_text("12deg"), None);
        assert_eq!(parse_angle_text("-5"), None);
    }

    #[test]
    fn slider_is_ten_degrees_per_step() {
        assert_eq!(slider_to_angle(4), 40);
        assert_eq!(slider_to_angle(108), 1080);
        assert_eq!(angle_to_slider(905), 90);
        assert_eq!(angle_to_slider(40), 4);
    }

    #[test]
    fn gains_use_hundredths() {
        assert_eq!(device_gain_units(100), 10_000);
        assert_eq!(device_gain_units(75), 7_500);
        assert_eq!(display_gain_pct(7_500), 75);
        assert_eq!(display_gain_pct(10_000), 100);
    }

    #[test]
    fn reset_angle_matches_the_two_tables() {
        // LED variant: device id table (mask 0x4408).
        assert_eq!(reset_angle_deg(true, Some(3), false), 360);
        assert_eq!(reset_angle_deg(true, Some(10), false), 360);
        assert_eq!(reset_angle_deg(true, Some(14), false), 360);
        assert_eq!(reset_angle_deg(true, Some(2), false), 900);
        assert_eq!(reset_angle_deg(true, None, false), 900);
        // Ids above 0xe fail the `cmp eax,0xe` guard and fall back to 900.
        assert_eq!(reset_angle_deg(true, Some(15), true), 900);
        // Plain units: 900 unless a model flag asks for 360.
        assert_eq!(reset_angle_deg(false, None, false), 900);
        assert_eq!(reset_angle_deg(false, None, true), 360);
        assert_eq!(opening_angle_deg(true), 900);
        assert_eq!(opening_angle_deg(false), 1080);
    }

    #[test]
    fn led_default_branch_is_recorded() {
        // The LED branch of the Default handler (0x180007d44).
        assert_eq!(LED_DEFAULT_MODE_INDEX, 1);
        assert_eq!(MODE_LABELS[LED_DEFAULT_MODE_INDEX], "Sport");
        assert_eq!(LED_DEFAULT_COLOR, 0x80ff);
        assert_eq!(LED_PERIODIC_FACTOR, 21);
        assert_eq!(LED_SPRING_FACTOR, 11);
        assert_eq!(LED_DAMPER_FACTOR, 11);
    }

    #[test]
    fn colour_decomposition_matches_sub_18000f0d0() {
        // 0x80ff is R 0, G 0x80, B 0xff in COLORREF order.
        assert_eq!(color_to_hsl(0x80ff), (148, 255, 127));
        // Primary colours and greys follow the same 42.5/85/170 scale.
        assert_eq!(color_to_hsl(0x0000ff), (170, 255, 127));
        assert_eq!(color_to_hsl(0x00ff00), (85, 255, 127));
        assert_eq!(color_to_hsl(0xff0000), (0, 255, 127));
        assert_eq!(color_to_hsl(0x808080), (0, 0, 128));
        // 0x80ff → H 148, S 255, L 127 → 148*21>>8, 255*11>>8, 127*11>>8.
        assert_eq!(led_default_detail_gains(), (12, 10, 5));
    }

    #[test]
    fn led_intensity_and_linearity_follow_the_binary() {
        // .bss global 0x180065894 starts at zero (no file bytes).
        assert_eq!(LED_INTENSITY_INITIAL_PCT, 0);
        // Trackbar 1113 / static 1114 exist but are never read.
        assert_eq!(LINEARITY_CONTROL_ID, 1113);
        assert_eq!(LINEARITY_LABEL_ID, 1114);
        assert!(!LINEARITY_CONNECTED);
    }

    #[test]
    fn auto_center_modes() {
        assert_eq!(autocenter_mode(false, false), 3);
        assert_eq!(autocenter_mode(true, true), 5);
        assert_eq!(autocenter_mode(true, false), 6);
    }

    #[test]
    fn vendor_property_values_match_the_handlers() {
        // Mode combo: CB_GETCURSEL -> sub_1800045b0(index) -> 0xf001a000.
        assert_eq!(MODE_PROPERTY, 0xf001a000);
        assert_eq!(mode_property_value(0), 0);
        assert_eq!(mode_property_value(3), 3);
        // 0x458: BM_GETCHECK then sete -> 1 while unchecked.
        assert_eq!(HEXALED_PROPERTY, 0xf001e000);
        assert_eq!(hexaled_property_value(false), 1);
        assert_eq!(hexaled_property_value(true), 0);
        // 0x3f4 Combined -> 1, 0x3f5 Separate -> 0.
        assert_eq!(PEDAL_MODE_PROPERTY, 0xf0011000);
        assert_eq!(pedal_mode_property_value(false), 1);
        assert_eq!(pedal_mode_property_value(true), 0);
        // 0x455 ON -> 1, 0x456 OFF -> 0, property 0x1042.
        assert_eq!(BOOST_PROPERTY, 0x1042);
        assert_eq!(boost_property_value(true), 1);
        assert_eq!(boost_property_value(false), 0);
        assert_eq!(AUTOCENTER_STRENGTH_PROPERTY, 0xf0022000);
        assert_eq!(CONSTANT_GAIN_PROPERTY, 0xf001b000);
        assert_eq!(LED_COLOR_PROPERTY, 0xf001d000);
    }

    #[test]
    fn led_opening_path_uses_its_own_factors() {
        // sub_18000606d: H*0x13>>8, S*0xb>>8, L*0xb>>8.
        assert_eq!(led_opening_detail_gains(), (10, 10, 5));
        assert_eq!(led_default_detail_gains(), (12, 10, 5));
        assert_eq!(LED_PERIODIC_MAX, 18);
        assert_eq!(LED_SPRING_MAX, 10);
        assert_eq!(LED_DAMPER_MAX, 10);
    }

    #[test]
    fn angle_trackbar_tick_is_eighty_degrees() {
        assert_eq!(ANGLE_TICK_POSITION, 8);
        assert_eq!(slider_to_angle(ANGLE_TICK_POSITION), 80);
        assert_eq!(ANGLE_TRACKBAR_MIN_POS, 4);
        assert_eq!(ANGLE_TRACKBAR_MAX_900_POS, 90);
        assert_eq!(ANGLE_TRACKBAR_MAX_1080_POS, 108);
    }
}
