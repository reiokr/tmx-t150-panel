use crate::bridge::{self, Bridge, PadButton, Profiles};
use crate::device::{self, DeviceInfo, DeviceMonitor};
use crate::ffb::{self, ForceFeedback};
use crate::mechanics as mech;
use crate::protondb;
use crate::steam;
use crate::theme;
use crate::theme::*;
use eframe::egui::{
    self, Align, Align2, Color32, FontId, Id, Layout, Pos2, Rect, Response, RichText, Rounding,
    Sense, Stroke, Ui, Vec2,
};

pub const ROTATION_MIN: f32 = mech::ANGLE_MIN_DEG as f32;
pub const ROTATION_MAX: f32 = mech::ANGLE_MAX_DEG as f32;
pub const ROTATION_MAX_LIMITED: f32 = mech::ANGLE_MAX_DEG_LIMITED as f32;
pub const GAIN_MAX: f32 = mech::GAIN_MAX_PCT as f32;

/// Dialog 106, controls 1097/1095/1075/1094/1077, left to right.
pub const RIMS: [&str; 5] = [
    "TM Open Wheel",
    "PS Wheel",
    "GT Wheel",
    "FERRARI 458 Challenge Wheel",
    "FERRARI F1 Wheel",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Input,
    Forces,
    Settings,
    Profiles,
}

pub enum PanelAction {
    Setup,
}

/// `HID_NAME` repeats the vendor on Thrustmaster wheels ("Thrustmaster
/// Thrustmaster TMX Racing Wheel"); show the repeated word once.
fn dedup_words(name: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for word in name.split_whitespace() {
        if out
            .last()
            .map(|prev| prev.eq_ignore_ascii_case(word))
            .unwrap_or(false)
        {
            continue;
        }
        out.push(word);
    }
    out.join(" ")
}

#[derive(Clone, Copy)]
pub enum Wave {
    Engine,
    Tire,
    Boing,
    Explosion,
    Sea,
    Turbo,
    Gong,
    Bumpy,
    Crash,
    Punch,
    Field,
    Whiplash,
}

pub struct Effect {
    pub name: &'static str,
    pub desc: &'static str,
    pub kind: Wave,
    /// `DIEFFECT.dwDuration` from the matching `CPL_effects/*.ffe` preset,
    /// stored in microseconds at file offset 0x70.
    pub dur: f32,
    /// The preset's file stem (`012_..`) and `DIEFFECT.dwGain` (0..10000).
    pub preset: &'static str,
    pub gain: u16,
}

pub const EFFECTS: [Effect; 12] = [
    Effect {
        name: "Engine",
        desc: "A steady combustion rumble, the way a running engine is felt through the rim.",
        kind: Wave::Engine,
        dur: 2.20,
        preset: "01_Engine",
        gain: 10000,
    },
    Effect {
        name: "Blown Tire",
        desc: "A rough, flapping vibration that fades as the wheel slows down.",
        kind: Wave::Tire,
        dur: 1.25,
        preset: "02_BlownTire",
        gain: 10000,
    },
    Effect {
        name: "Boing",
        desc: "A springy bounce with a fast decay.",
        kind: Wave::Boing,
        dur: 0.70,
        preset: "03_Boing",
        gain: 10000,
    },
    Effect {
        name: "Explosion",
        desc: "A single heavy impact followed by a short, dirty rumble.",
        kind: Wave::Explosion,
        dur: 2.65,
        preset: "04_Explosion",
        gain: 7000,
    },
    Effect {
        name: "Open Sea",
        desc: "A slow rolling swell, like a boat on open water.",
        kind: Wave::Sea,
        dur: 4.55,
        preset: "05_OpenSea",
        gain: 10000,
    },
    Effect {
        name: "Turbo Boost",
        desc: "A vibration that climbs in frequency as the boost builds.",
        kind: Wave::Turbo,
        dur: 1.25,
        preset: "06_TurboBoost",
        gain: 7000,
    },
    Effect {
        name: "Gong",
        desc: "A deep strike with a long, ringing tail.",
        kind: Wave::Gong,
        dur: 4.50,
        preset: "07_Gong",
        gain: 7000,
    },
    Effect {
        name: "Bumpy Road",
        desc: "Repeated sharp bumps, as if crossing a rough kerb.",
        kind: Wave::Bumpy,
        dur: 1.50,
        preset: "08_BumpyRoad",
        gain: 10000,
    },
    Effect {
        name: "Car Crash",
        desc: "A hard jolt and the rattle that follows.",
        kind: Wave::Crash,
        dur: 2.70,
        preset: "09_CarCrash",
        gain: 10000,
    },
    Effect {
        name: "Punch",
        desc: "One very short, very strong kick.",
        kind: Wave::Punch,
        dur: 0.25,
        preset: "10_Punch",
        gain: 10000,
    },
    Effect {
        name: "Force Field",
        desc: "A constant resistance pushing back against the rim.",
        kind: Wave::Field,
        dur: 1.00,
        preset: "11_ForceField",
        gain: 10000,
    },
    Effect {
        name: "Whiplash",
        desc: "A fast left-right snap that settles quickly.",
        kind: Wave::Whiplash,
        dur: 1.00,
        preset: "12_Whiplash",
        gain: 10000,
    },
];

/// One `DIEFFECT` block from the matching `CPL_effects/*.ffe` file.  The files
/// hold one to three of these per preset; durations, gains, waveforms, periods
/// and envelopes below were read from the RIFF `FORC` data (DIEFFECT at file
/// offset 0x68, effect GUID at +0x38, `data` chunks after it).
#[derive(Clone, Copy)]
pub struct FxPart {
    /// `FF_SINE` 0x5a, `FF_SQUARE` 0x58, `FF_TRIANGLE` 0x59, `FF_CONSTANT` 0x52.
    pub waveform: u16,
    /// `DIPERIODIC.dwPeriod`, microseconds.
    pub period_us: u32,
    /// `DIPERIODIC.dwMagnitude` (0..10000); negative for a constant force.
    pub magnitude: i32,
    /// `DIPERIODIC.lOffset` / constant level sign, -10000..10000.
    pub offset: i32,
    /// `DIPERIODIC.dwPhase`, hundredths of a degree.
    pub phase: u16,
    pub attack_level: i32,
    pub attack_us: u32,
    pub fade_level: i32,
    pub fade_us: u32,
    /// `DIEFFECT.dwDuration`, milliseconds.
    pub millis: u32,
    /// `DIEFFECT.dwGain`, 0..10000.
    pub gain: u32,
}

impl FxPart {
    const fn periodic(
        waveform: u16,
        period_us: u32,
        magnitude: i32,
        offset: i32,
        phase: u16,
        envelope: (i32, u32, i32, u32),
        millis: u32,
        gain: u32,
    ) -> Self {
        Self {
            waveform,
            period_us,
            magnitude,
            offset,
            phase,
            attack_level: envelope.0,
            attack_us: envelope.1,
            fade_level: envelope.2,
            fade_us: envelope.3,
            millis,
            gain,
        }
    }

    const fn constant(level: i32, envelope: (i32, u32, i32, u32), millis: u32, gain: u32) -> Self {
        Self::periodic(0x52, 0, level, 0, 0, envelope, millis, gain)
    }
}

const SINE: u16 = 0x5a;
const SQUARE: u16 = 0x58;
const TRIANGLE: u16 = 0x59;

/// The effect blocks in the same order as `EFFECTS`.  A part's own duration is
/// capped at the preset's headline duration so one Test Forces press stays one
/// run on screen.
pub const PARTS: [&[FxPart]; 12] = [
    // 01_Engine.ffe: Square 2.2 s + Sine 3.7 s
    &[
        FxPart::periodic(
            SQUARE,
            536_585,
            7_203,
            -490,
            0,
            (10_000, 341_463, 0, 1_393_171),
            2_200,
            10_000,
        ),
        FxPart::periodic(
            SINE,
            107_317,
            7_829,
            0,
            0,
            (0, 2_572_093, 1_888, 448_780),
            2_200,
            10_000,
        ),
    ],
    // 02_BlownTire.ffe: Square 1.25 s + Constant 0.9 s
    &[
        FxPart::periodic(
            SQUARE,
            546_341,
            852,
            0,
            0,
            (0, 217_674, 6_899, 682_325),
            1_250,
            10_000,
        ),
        FxPart::constant(-10_000, (10_000, 0, 1, 0), 900, 10_000),
    ],
    // 03_Boing.ffe: Sine 0.7 s
    &[FxPart::periodic(
        SINE,
        33_330,
        5_038,
        0,
        0,
        (10_000, 149_767, 1, 485_116),
        700,
        10_000,
    )],
    // 04_Explosion.ffe: Sine 2.65 s + Sine 2.9 s + Square 0.85 s
    &[
        FxPart::periodic(
            SINE,
            100_000,
            4_418,
            0,
            0,
            (9_534, 325_581, 1, 534_883),
            2_650,
            7_000,
        ),
        FxPart::periodic(
            SINE,
            624_390,
            7_762,
            -1_548,
            17_912,
            (1, 0, 1, 0),
            2_650,
            7_000,
        ),
        FxPart::periodic(SQUARE, 770_732, 10_000, 0, 0, (0, 0, 0, 0), 850, 10_000),
    ],
    // 05_OpenSea.ffe: Sine 4.55 s
    &[FxPart::periodic(
        SINE,
        2_000_000,
        8_322,
        0,
        0,
        (0, 0, 0, 0),
        4_550,
        10_000,
    )],
    // 06_TurboBoost.ffe: Sine 1.25 s + Square 0.25 s
    &[
        FxPart::periodic(
            SINE,
            20_000,
            5_000,
            0,
            0,
            (1, 400_000, 1, 850_000),
            1_250,
            7_000,
        ),
        FxPart::periodic(
            SQUARE,
            450_000,
            10_000,
            0,
            0,
            (1, 177_209, 1, 18_139),
            250,
            7_000,
        ),
    ],
    // 07_Gong.ffe: Sine 4.5 s + Sine 0.125 s
    &[
        FxPart::periodic(
            SINE,
            10_000,
            5_000,
            0,
            0,
            (10_000, 750_000, 1_000, 3_750_000),
            4_500,
            7_000,
        ),
        FxPart::periodic(SINE, 250_000, 7_519, 0, 4_500, (0, 0, 0, 0), 125, 7_000),
    ],
    // 08_BumpyRoad.ffe: Sine 1.5 s + Sine 1.0 s
    &[
        FxPart::periodic(SINE, 90_000, 5_000, 0, 0, (0, 0, 0, 0), 1_500, 10_000),
        FxPart::periodic(SINE, 333_300, 2_000, 0, 0, (0, 0, 0, 0), 1_000, 10_000),
    ],
    // 09_CarCrash.ffe: Square 2.7 s + Square 2.8 s + Triangle 1.5 s
    &[
        FxPart::periodic(
            SQUARE,
            3_800_000,
            5_813,
            0,
            18_000,
            (10_000, 893_023, 8_449, 715_813),
            2_700,
            10_000,
        ),
        FxPart::periodic(
            SQUARE,
            400_000,
            0,
            0,
            0,
            (0, 1_588_837, 4_263, 1_172_093),
            2_700,
            10_000,
        ),
        FxPart::periodic(
            TRIANGLE,
            1_500_000,
            4_108,
            0,
            0,
            (0, 0, 0, 0),
            1_500,
            10_000,
        ),
    ],
    // 10_Punch.ffe: Sine 0.25 s (its second block is a Damper, which evdev
    // cannot express on this wheel, so only the Sine kick is sent)
    &[FxPart::periodic(
        SINE,
        750_000,
        10_000,
        0,
        9_000,
        (0, 0, 0, 0),
        250,
        10_000,
    )],
    // 11_ForceField.ffe: Square 1.0 s + Square 0.5 s
    &[
        FxPart::periodic(
            SQUARE,
            31_332,
            10_000,
            0,
            0,
            (2_250, 304_794, 0, 530_821),
            1_000,
            10_000,
        ),
        FxPart::periodic(
            SQUARE,
            1_032_761,
            10_000,
            0,
            34_000,
            (0, 0, 0, 239_726),
            500,
            10_000,
        ),
    ],
    // 12_Whiplash.ffe: Sine 1.0 s + Sine 1.5 s
    &[
        FxPart::periodic(SINE, 546_341, 6_364, 0, 0, (0, 0, 0, 0), 1_000, 10_000),
        FxPart::periodic(
            SINE,
            975_610,
            8_601,
            0,
            0,
            (210, 1_500_000, 10_000, 1_000_000),
            1_000,
            10_000,
        ),
    ],
];

pub struct Play {
    pub idx: usize,
    pub t: f32,
}

/// The 0x58-byte settings block the original snapshots when the page opens
/// (`sub_1800020e0`) and copies back on Cancel/PSN_RESET (`sub_180003e00`).
/// It holds the gains and the auto-center choice, not rotation or linearity.
#[derive(Clone, Copy, Default)]
pub struct GainSnapshot {
    overall: f32,
    constant: f32,
    periodic: f32,
    spring: f32,
    damper: f32,
    autocenter_strength: f32,
    autocenter_game: bool,
    boost: bool,
}

pub struct Panel {
    pub tab: Tab,
    pub rotation: f32,
    pub angle_text: String,
    pub overall: f32,
    pub constant: f32,
    pub periodic: f32,
    pub spring: f32,
    pub damper: f32,
    pub linearity: f32,
    pub autocenter_strength: f32,
    pub boost: bool,
    pub autocenter_game: bool,
    pub led_intensity: f32,
    pub leds_on: bool,
    /// Dialog 118 control 1112 (`0x458`), the "Default color and lighting
    /// effect" checkbox.  Its property is written inverted (see
    /// `mechanics::hexaled_property_value`).
    pub hexaled: bool,
    /// Mode combo (dialog 118 control 1111); 1 = Sport, the LED Default.
    pub mode: usize,
    pub mode_notice: bool,
    pub advanced: bool,
    pub rim: usize,
    pub steer: f32,
    pub pedals: [f32; 3],
    pub buttons: [f32; 12],
    pub pov: usize,
    pub effect: Option<Play>,
    pub t: f32,
    pub last_input: f32,
    pub show_update: bool,
    pub dirty: bool,
    pub sep: bool,
    pub status: String,
    pub glow: f32,
    pub device: DeviceMonitor,
    pub dev_info: Option<DeviceInfo>,
    pub live: bool,
    pub dev_events: u64,
    pub diag: Option<device::Diagnostics>,
    /// A running "Switch to PC mode" job; its result arrives on this channel.
    mode_switch: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    pub switching: bool,
    pub keep_awake: bool,
    pub hw_idle: f32,
    pub sleep_blocker: device::SleepBlocker,
    /// Real force feedback through the wheel's evdev node (see ffb.rs).
    pub ffb: ForceFeedback,
    ffb_node: Option<std::path::PathBuf>,
    /// Node whose live settings this page has already read back.
    synced_node: Option<std::path::PathBuf>,
    pub ffb_message: Option<String>,
    /// Per-game tuning and the virtual Xbox 360 pad.
    pub bridge: Bridge,
    pub profiles: Profiles,
    /// Text buffer for the profile's Steam AppID field.
    pub profile_appid: String,
    profile_timer: f32,
    /// AppID Steam reported last time the profile follower looked.
    pub running_app: Option<u32>,
    /// The "Game notes" window: known setups for specific games.
    pub show_notes: bool,
    /// Result line of the last Steam library scan.
    pub notes_report: String,
    /// Light, dark, or whatever the desktop uses.
    pub theme: theme::ThemeMode,
    /// UI zoom, 1.0 = 100 %.
    pub scale: f32,
    /// ProtonDB verdicts, cached on disk.
    pub protondb: protondb::Cache,
    protondb_rx: Option<std::sync::mpsc::Receiver<(u32, Option<protondb::Status>)>>,
    protondb_timer: f32,
    /// `--effect N` asks for an effect before the first frame, when the wheel
    /// has not been opened yet; the first update sends it.
    pub pending_effect: Option<(usize, f32)>,
    saved: GainSnapshot,
}

fn wave(kind: Wave, t: f32, dur: f32) -> f32 {
    let fade = (1.0 - (t / dur)).clamp(0.0, 1.0);
    let n = |x: f32| ((x * 12347.13).sin() * 43758.5453).fract() * 2.0 - 1.0;
    match kind {
        Wave::Engine => {
            (0.55 * (t * 2.0 * std::f32::consts::PI * 19.0).sin()
                + 0.20 * (t * 2.0 * std::f32::consts::PI * 47.0 + 1.3).sin()
                + 0.08 * n(t * 30.0))
                * (0.72 + 0.28 * (t * 2.0 * std::f32::consts::PI * 3.0).sin())
        }
        Wave::Tire => {
            (0.6 * (t * 2.0 * std::f32::consts::PI * 11.0).sin()
                * (t * 2.0 * std::f32::consts::PI * 29.0).sin()
                + 0.18 * n(t * 60.0))
                * fade
        }
        Wave::Boing => 0.95 * (t * 2.0 * std::f32::consts::PI * 5.5).sin() * (-3.2 * t).exp(),
        Wave::Explosion => (0.85 * (-5.0 * t).exp() + 0.25 * n(t * 45.0) * (-2.0 * t).exp()) * fade,
        Wave::Sea => {
            0.5 * (t * 2.0 * std::f32::consts::PI * 0.55).sin()
                + 0.22 * (t * 2.0 * std::f32::consts::PI * 1.15 + 0.8).sin()
        }
        Wave::Turbo => 0.7 * (t * 2.0 * std::f32::consts::PI * (2.0 + 6.0 * t) * t).sin() * fade,
        Wave::Gong => {
            (0.7 * (t * 2.0 * std::f32::consts::PI * 2.7).sin() * (-1.5 * t).exp()
                + 0.3 * (t * 2.0 * std::f32::consts::PI * 4.6).sin() * (-2.2 * t).exp())
                * fade
        }
        Wave::Bumpy => {
            let s = if (t * 9.0).sin() > 0.0 { 1.0 } else { -1.0 };
            s * 0.55 * (0.5 + 0.5 * (t * 13.0).sin()) * fade
        }
        Wave::Crash => {
            (0.9 * (-9.0 * t).exp()
                + 0.35 * (t * 2.0 * std::f32::consts::PI * 17.0).sin() * (-1.4 * t).exp()
                + 0.2 * n(t * 50.0) * fade)
                * fade
        }
        Wave::Punch => {
            if t < 0.16 {
                (t * 2.0 * std::f32::consts::PI * 9.0).sin() * 1.0
            } else {
                0.0
            }
        }
        Wave::Field => 0.85 * fade.sqrt(),
        Wave::Whiplash => {
            let saw = ((t * 2.4) % 1.0) * 2.0 - 1.0;
            saw * (-1.2 * t).exp()
        }
    }
}

impl Panel {
    /// `sub_1800133b0` / `sub_180001a50` gate the 900° limit on the wheel
    /// flag 0x1800657bc, not on the Normal/Advanced radio.  No wheel (the
    /// demo) behaves like the bundled hid-t150 units: a 900° wheel.
    fn limited_900(&self) -> bool {
        match &self.dev_info {
            Some(info) => device::is_900_degree(info.product),
            None => true,
        }
    }

    /// Which default/limit group the original would put this wheel in.
    fn wheel_class(&self) -> mech::WheelClass {
        if self.limited_900() {
            mech::WheelClass::Limited900
        } else if let Some(info) = &self.dev_info {
            if info.product == 0xb65d || info.product == 0xb66e {
                mech::WheelClass::T500Family
            } else {
                mech::WheelClass::Generic
            }
        } else {
            mech::WheelClass::Generic
        }
    }

    pub fn new() -> Self {
        let profiles = bridge::load_profiles();
        let overall = mech::default_overall_pct(mech::WheelClass::Limited900) as f32;
        let start_angle = mech::opening_angle_deg(true);
        let mut panel = Self {
            tab: Tab::Input,
            rotation: start_angle as f32,
            angle_text: format!("{start_angle}°"),
            overall,
            constant: mech::DEFAULT_DETAIL_GAIN_PCT as f32,
            periodic: mech::DEFAULT_DETAIL_GAIN_PCT as f32,
            spring: mech::DEFAULT_DETAIL_GAIN_PCT as f32,
            damper: mech::DEFAULT_DETAIL_GAIN_PCT as f32,
            linearity: 0.0,
            autocenter_strength: mech::DEFAULT_AUTOCENTER_STRENGTH_PCT as f32,
            boost: false,
            autocenter_game: true,
            led_intensity: mech::LED_INTENSITY_INITIAL_PCT as f32,
            leds_on: false,
            hexaled: false,
            mode: mech::LED_DEFAULT_MODE_INDEX,
            mode_notice: false,
            advanced: true,
            rim: 0,
            steer: 0.0,
            pedals: [0.0, 0.0, 0.0],
            buttons: [0.0; 12],
            pov: 0,
            effect: None,
            t: 0.0,
            last_input: 10.0,
            show_update: false,
            dirty: false,
            sep: true,
            status: String::from("Wheel connected."),
            glow: 0.0,
            device: DeviceMonitor::new(),
            dev_info: None,
            live: false,
            dev_events: 0,
            diag: None,
            mode_switch: None,
            switching: false,
            keep_awake: true,
            hw_idle: 0.0,
            sleep_blocker: device::SleepBlocker::new(),
            ffb: ForceFeedback::new(),
            ffb_node: None,
            synced_node: None,
            ffb_message: None,
            bridge: {
                let mut bridge = Bridge::new();
                bridge.settings = profiles.current().settings.clone();
                bridge
            },
            profiles,
            profile_appid: String::new(),
            profile_timer: 0.0,
            running_app: None,
            show_notes: false,
            notes_report: String::new(),
            theme: theme::mode(),
            scale: theme::load_scale(),
            protondb: protondb::Cache::load(),
            protondb_rx: None,
            protondb_timer: 0.0,
            pending_effect: None,
            saved: GainSnapshot::default(),
        };
        panel.profile_appid = panel
            .profiles
            .current()
            .appid
            .map(|v| v.to_string())
            .unwrap_or_default();
        panel.saved = panel.snapshot();
        panel
    }

    /// What the page would copy out when it opens: the gain block and the
    /// auto-center choice only (rotation, linearity and LED are not in it).
    fn snapshot(&self) -> GainSnapshot {
        GainSnapshot {
            overall: self.overall,
            constant: self.constant,
            periodic: self.periodic,
            spring: self.spring,
            damper: self.damper,
            autocenter_strength: self.autocenter_strength,
            autocenter_game: self.autocenter_game,
            boost: self.boost,
        }
    }

    /// PSN_RESET / Cancel: copy the opening snapshot back over the gains.
    fn restore_snapshot(&mut self) {
        let s = self.saved;
        self.overall = s.overall;
        self.constant = s.constant;
        self.periodic = s.periodic;
        self.spring = s.spring;
        self.damper = s.damper;
        self.autocenter_strength = s.autocenter_strength;
        self.autocenter_game = s.autocenter_game;
        self.boost = s.boost;
        self.dirty = false;
        self.status = String::from("Changes discarded; the applied settings are back.");
    }

    fn reset_defaults(&mut self) {
        // `sub_180005f60`, WM_COMMAND 1002 (control 0x3ea): the 900-degree
        // branch of the Default handler writes only the gain block:
        // overall 75 %, Constant/Periodic/Spring/Damper 100 %,
        // auto-center strength 50 %, boost off, auto-center by the game.
        // It does not touch rotation, linearity, LED intensity or the
        // Advanced/HexaLed switches, so neither do we.
        let class = self.wheel_class();
        self.overall = mech::default_overall_pct(class) as f32;
        self.constant = mech::DEFAULT_DETAIL_GAIN_PCT as f32;
        self.periodic = mech::DEFAULT_DETAIL_GAIN_PCT as f32;
        self.spring = mech::DEFAULT_DETAIL_GAIN_PCT as f32;
        self.damper = mech::DEFAULT_DETAIL_GAIN_PCT as f32;
        self.autocenter_strength = mech::DEFAULT_AUTOCENTER_STRENGTH_PCT as f32;
        self.boost = false;
        self.autocenter_game = true;
        self.dirty = true;
        self.status = String::from("Default settings restored.");
        self.sync_wheel_forces();
    }

    pub fn update(&mut self, dt: f32, ctx: &egui::Context) {
        self.t += dt;
        self.last_input += dt;
        self.glow = (self.glow - dt * 2.0).max(0.0);
        self.bridge.poll();

        // ProtonDB lookup for the active profile, one request at a time.
        if let Some(rx) = &self.protondb_rx {
            if let Ok((appid, status)) = rx.try_recv() {
                match status {
                    Some(status) => self.protondb.insert(appid, status),
                    None => {
                        // Offline or rate limited: remember the miss, retry in ~an hour.
                        self.protondb.insert(
                            appid,
                            protondb::Status {
                                tier: protondb::Tier::Unknown,
                                trending: protondb::Tier::Unknown,
                                total: 0,
                                fetched: protondb::now_secs() - 23 * 60 * 60,
                            },
                        );
                    }
                }
                self.protondb_rx = None;
            }
        }
        self.protondb_timer += dt;
        if self.protondb_rx.is_none() && self.protondb_timer > 1.5 {
            self.protondb_timer = 0.0;
            if let Some(appid) = self.profiles.current().appid {
                if self.protondb.needs_fetch(appid) {
                    self.protondb_rx = Some(protondb::spawn(appid));
                }
            }
        }

        // Follow the running Steam app once a second: switching games switches
        // the whole tuning block with it.
        self.profile_timer += dt;
        if self.profile_timer >= 1.0 {
            self.profile_timer = 0.0;
            self.running_app = bridge::running_appid();
            if self.profiles.follow {
                if let Some(appid) = self.running_app {
                    if let Some(index) = self.profiles.index_of_appid(appid) {
                        if index != self.profiles.active {
                            self.activate_profile(index);
                        }
                    }
                }
            }
        }

        // A finished "Switch to PC mode" job rescans the wheel and refreshes
        // the diagnostic list.
        self.poll_mode_switch();

        // keyboard steering
        if ctx.input(|i| i.key_down(egui::Key::ArrowLeft) || i.key_down(egui::Key::A)) {
            self.steer = (self.steer - dt * 1.6).clamp(-1.0, 1.0);
            self.last_input = 0.0;
        }
        if ctx.input(|i| i.key_down(egui::Key::ArrowRight) || i.key_down(egui::Key::D)) {
            self.steer = (self.steer + dt * 1.6).clamp(-1.0, 1.0);
            self.last_input = 0.0;
        }

        for b in self.buttons.iter_mut() {
            *b = (*b - dt).max(0.0);
        }

        let mut effect_finished = false;
        if let Some(p) = &mut self.effect {
            p.t += dt;
            if p.t > EFFECTS[p.idx].dur {
                effect_finished = true;
            }
        }
        if effect_finished {
            self.effect = None;
            // The kernel stops a periodic effect at the end of its replay
            // length; remove it so the next test can reuse the slot.
            self.ffb.stop();
        }

        if self.leds_on && self.t % 2.0 < 0.02 {
            self.leds_on = false;
        }


        // `sub_180001a50`: a 900° wheel opens on its live angle (900 in the
        // demo); a full-range wheel reports 1080.  `sub_1800133b0` caps input
        // at 40…1080, or 900 on the 900° flag 0x1800657bc.
        let clamped = mech::clamp_angle(self.rotation.round() as i32, self.limited_900());
        if clamped as f32 != self.rotation {
            self.rotation = clamped as f32;
        }

        // simulated hardware so the page is alive without a real wheel
        if self.last_input > 1.8 {
            let demo = 0.30 * (self.t * 0.55).sin() + 0.07 * (self.t * 1.9 + 0.7).sin();
            self.steer += (demo - self.steer) * (dt * 2.6).min(1.0);
            let gas = 0.55 + 0.28 * (self.t * 0.8 + 1.0).sin();
            let brake = 0.16 + 0.14 * (self.t * 0.63).sin().max(0.0);
            let clutch = 0.08 + 0.06 * (self.t * 0.51 + 2.0).sin().max(0.0);
            self.pedals[0] += (clutch - self.pedals[0]) * (dt * 2.0).min(1.0);
            self.pedals[1] += (brake - self.pedals[1]) * (dt * 2.0).min(1.0);
            self.pedals[2] += (gas - self.pedals[2]) * (dt * 2.0).min(1.0);
        }
        if self.effect.is_none() {
            let v = if self.t % 7.0 < 0.9 { 1.0 } else { 0.0 };
            if v > 0.0 && self.t % 7.0 < dt {
                for i in [2, 5] {
                    self.buttons[i] = 0.18;
                }
            }
        }

        // Real hardware wins over the demo: when a wheel is present its state
        // is pushed in after the simulated bits so nothing can overwrite it.
        let snap = self.device.poll(dt);
        let wheel_moved = snap.connected && snap.events != self.dev_events;
        self.live = snap.connected;
        self.dev_info = self.device.info.clone();

        // `sub_18000606d` fills the sliders from the wheel when the page
        // opens (`sub_180001f00`, `sub_180001bb0`, `sub_1800020a0`,
        // `sub_180001b20`), rather than from the Default button's numbers.
        // Do the same through hid-t150 whenever a wheel appears or is
        // rescanned, but never over unsaved edits.
        let node = self.dev_info.as_ref().map(|i| i.node.clone());
        if node != self.synced_node {
            self.synced_node = node;
            if !self.dirty {
                self.adopt_wheel_settings();
            }
        }

        // The force-feedback handle follows the joystick node: js0 -> event9.
        let wanted = self.dev_info.as_ref().map(|i| i.node.clone());
        if wanted != self.ffb_node {
            self.ffb.attach(self.dev_info.as_ref());
            self.ffb_node = wanted;
        }
        if let Some((idx, t)) = self.pending_effect.take() {
            self.start_effect_at(idx, t);
        }
        if self.live {
            self.dev_events = snap.events;
            self.steer = snap.steer;
            self.pedals = snap.pedals;
            self.pov = snap.pov;
            for (i, b) in self.buttons.iter_mut().enumerate() {
                *b = if snap.buttons.get(i).copied().unwrap_or(false) {
                    1.0
                } else {
                    0.0
                };
            }
            self.last_input = 0.0;
        }
        if wheel_moved {
            self.hw_idle = 0.0;
        } else {
            self.hw_idle += dt;
        }

        // KDE Plasma can idle-sleep or lock the screen while a wheel is in use,
        // because the wheel sends no pointer or keyboard events.  Hold an idle
        // and sleep inhibitor while the wheel is moving, and drop it again as
        // soon as the wheel has been still for two minutes (or the panel exits).
        self.sleep_blocker
            .set(self.keep_awake && self.live && self.hw_idle < 120.0);
    }

    fn force(&self) -> f32 {
        if let Some(p) = &self.effect {
            let preset_gain = EFFECTS[p.idx].gain as f32 / mech::EFFECT_GAIN_FULL as f32;
            wave(EFFECTS[p.idx].kind, p.t, EFFECTS[p.idx].dur)
                * preset_gain
                * (self.overall / GAIN_MAX)
        } else {
            0.0
        }
    }

    /// One cycle of the periodic effect, in milliseconds.  The presets are all
    /// periodic (`MECHANICS.md`); only duration and gain were recovered from
    /// the `.ffe` files, so the period and waveform are an estimate chosen to
    /// match the sound each preset is named after.
    pub fn start_effect(&mut self, idx: usize) {
        self.start_effect_at(idx, 0.0);
    }

    /// Start an effect on screen and on the wheel.  The wheel never has to be
    /// present: the graph still animates and the reason is shown in the card.
    fn start_effect_at(&mut self, idx: usize, t: f32) {
        let idx = idx.min(EFFECTS.len() - 1);
        let name = EFFECTS[idx].name;
        let preset_seconds = EFFECTS[idx].dur;
        self.effect = Some(Play { idx, t });
        let overall = self.overall / GAIN_MAX;
        let pulses: Vec<ffb::Pulse> = PARTS[idx]
            .iter()
            .map(|p| ffb::Pulse {
                name,
                waveform: p.waveform,
                period_us: p.period_us,
                magnitude: p.magnitude,
                offset: p.offset,
                phase: p.phase,
                attack_level: p.attack_level,
                attack_us: p.attack_us,
                fade_level: p.fade_level,
                fade_us: p.fade_us,
                seconds: (p.millis as f32 / 1000.0).min(preset_seconds),
                gain: p.gain,
                overall,
            })
            .collect();
        match self.ffb.play(&pulses) {
            Ok(msg) => {
                self.status = msg.clone();
                self.ffb_message = Some(msg);
            }
            Err(err) => {
                self.ffb_message = Some(err.clone());
                self.status = format!("Playing {name} on screen. {err}");
            }
        }
    }

    /// FF_GAIN and FF_AUTOCENTER are global wheel settings, unlike the effect
    /// slots; keep them in step with the page.
    fn sync_wheel_forces(&mut self) {
        self.ffb.set_gain(self.overall);
        let strength = if self.autocenter_game {
            0.0
        } else {
            self.autocenter_strength
        };
        self.ffb.set_autocenter(strength);
    }

    fn apply_now(&mut self) {
        // Whatever lands on the wheel also belongs to the active profile.
        if let Some(profile) = self.profiles.list.get_mut(self.profiles.active) {
            profile.rotation = self.rotation;
            profile.overall = self.overall;
            profile.autocenter_game = self.autocenter_game;
            profile.autocenter_strength = self.autocenter_strength;
        }
        bridge::save_profiles(&self.profiles);

        // Overall strength and auto-centre also exist as evdev force-feedback
        // controls, which work in Xbox mode where hid-t150 attributes do not.
        self.sync_wheel_forces();
        let ff_note = if self.ffb.is_ready() {
            " Overall strength and auto-centre went to the wheel over force feedback."
        } else {
            ""
        };
        let Some(info) = self.dev_info.clone() else {
            self.status = format!(
                "No wheel detected: settings are kept until a wheel is connected.{ff_note}"
            );
            return;
        };
        let settings = device::WheelSettings {
            rotation: self.rotation,
            gain: self.overall,
            autocenter: !self.autocenter_game,
            autocenter_strength: self.autocenter_strength,
        };
        self.status = match device::apply_settings(&info, &settings) {
            Ok(msg) => format!("{msg}{ff_note}"),
            Err(msg) => msg,
        };
    }

    /// Copy the wheel's live `range` / `gain` / `autocenter` back into the
    /// page, the way `sub_18000606d` reads them through the vendor API.
    /// Constant, periodic, spring and damper have no hid-t150 attribute, so
    /// they keep the documented defaults.
    fn adopt_wheel_settings(&mut self) {
        let Some(info) = self.dev_info.clone() else {
            return;
        };
        let Some(s) = device::read_settings(&info) else {
            return;
        };
        self.rotation = mech::clamp_angle(s.rotation.round() as i32, self.limited_900()) as f32;
        self.angle_text = format!("{}°", self.rotation as i32);
        self.overall = s.gain.round().clamp(0.0, GAIN_MAX);
        self.autocenter_strength = s.autocenter_strength.round().clamp(0.0, GAIN_MAX);
        self.autocenter_game = !s.autocenter;
        self.saved = self.snapshot();
        self.dirty = false;
        self.status = format!(
            "Read from the wheel: {}°, {:.0} % overall, auto-centre {}.",
            self.rotation as i32,
            self.overall,
            if self.autocenter_game {
                "by the game"
            } else {
                "by the wheel"
            }
        );
        // Reading must not write: the original's open path only calls the
        // getters, so the wheel's gain and auto-centre are left untouched
        // until the person presses Apply/OK.
    }

    pub fn open_diagnostics(&mut self) {
        self.device.rescan();
        self.dev_info = self.device.info.clone();
        self.live = self.device.snapshot.connected;
        self.adopt_wheel_settings();
        self.synced_node = self.dev_info.as_ref().map(|i| i.node.clone());
        self.diag = Some(device::diagnose(
            self.keep_awake,
            self.sleep_blocker.is_active(),
        ));
        self.status = String::from("Checked the USB connection, input device and drivers.");
    }

    fn check_state_color(state: device::CheckState) -> Color32 {
        match state {
            device::CheckState::Ok => pal().ok_green,
            device::CheckState::Warn => pal().warn,
            device::CheckState::Fail => Color32::from_rgb(190, 58, 52),
        }
    }

    /// True when the attached wheel is a TMX still running Xbox/GIP firmware.
    pub fn gip_wheel(&self) -> bool {
        self.dev_info
            .as_ref()
            .map(|info| device::is_gip_wheel(info.product))
            .unwrap_or(false)
    }

    fn start_mode_switch(&mut self) {
        if self.switching {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(device::switch_gip_to_pc_elevated());
        });
        self.mode_switch = Some(rx);
        self.switching = true;
        self.status = String::from(
            "Switching the wheel to PC mode - confirm the administrator password if you are asked for it.",
        );
    }

    fn poll_mode_switch(&mut self) {
        let Some(rx) = &self.mode_switch else {
            return;
        };
        let received = match rx.try_recv() {
            Ok(result) => Some(result),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(String::from(
                "The switch stopped without reporting a result.",
            ))),
        };
        let Some(result) = received else {
            return;
        };
        self.mode_switch = None;
        self.switching = false;
        self.open_diagnostics();
        self.status = match result {
            Ok(message) => message,
            Err(message) => message,
        };
    }

    /// The one action that takes a TMX out of Xbox firmware, with the plain
    /// reason next to it.  Safe to call whenever the wheel is in GIP mode.
    fn mode_switch_controls(&mut self, ui: &mut Ui, primary: bool) {
        ui.horizontal(|ui| {
            let label = if self.switching {
                "Switching to PC mode..."
            } else {
                "Switch to PC mode"
            };
            if button(ui, label, primary, !self.switching).clicked() {
                self.start_mode_switch();
            }
            if self.switching {
                ui.add(egui::Spinner::new().size(14.0));
            }
        });
        ui.label(
            RichText::new(
                "If your account may not open the USB port, you are asked for the administrator password once. The wheel keeps PC mode until it is unplugged.",
            )
            .size(10.5)
            .color(pal().muted),
        );
    }

    fn diagnostics_window(&mut self, ctx: &egui::Context) {
        let Some(diag) = self.diag.clone() else {
            return;
        };
        // The force-feedback channel is not part of device::diagnose because it
        // needs the open handle, so summarise it here for the same list.
        let (ff_state, ff_detail) = if self.ffb.is_ready() {
            (
                device::CheckState::Ok,
                format!(
                    "{} plays periodic effects ({} slots): {}.",
                    self.ffb
                        .node()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                    self.ffb.effects_max,
                    ffb::capability_summary(self.ffb.caps)
                ),
            )
        } else if self.live {
            (
                device::CheckState::Warn,
                self.ffb.error.clone().unwrap_or_else(|| {
                    String::from("The wheel has no periodic force-feedback capability.")
                }),
            )
        } else {
            (
                device::CheckState::Warn,
                String::from(
                    "No wheel is connected, so there is no force-feedback channel to open.",
                ),
            )
        };
        let mut open = true;
        egui::Window::new("Connection & driver check")
            .id(Id::new("connection_diagnostics"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(560.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(560.0);
                ui.horizontal(|ui| {
                    dot(ui, Self::check_state_color(diag.headline_state), 5.0);
                    ui.label(
                        RichText::new(&diag.summary)
                            .size(13.0)
                            .strong()
                            .color(pal().ink),
                    );
                });
                ui.add_space(9.0);
                // The list grew with the driver checks; keep the action at the
                // bottom reachable on a short screen.
                let list_h = (ctx.screen_rect().height() - 300.0).clamp(220.0, 520.0);
                egui::ScrollArea::vertical()
                    .max_height(list_h)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for check in &diag.checks {
                            ui.horizontal_top(|ui| {
                                ui.add_space(2.0);
                                dot(ui, Self::check_state_color(check.state), 4.0);
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(check.title)
                                            .size(12.5)
                                            .strong()
                                            .color(pal().ink),
                                    );
                                    ui.label(RichText::new(&check.detail).size(11.5).color(pal().muted));
                                    if let Some(hint) = &check.hint {
                                        ui.add_space(1.0);
                                        ui.label(
                                            RichText::new(hint)
                                                .size(11.0)
                                                .color(Self::check_state_color(check.state)),
                                        );
                                    }
                                });
                            });
                            ui.add_space(7.0);
                        }
                        ui.horizontal_top(|ui| {
                            ui.add_space(2.0);
                            dot(ui, Self::check_state_color(ff_state), 4.0);
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("Force feedback")
                                        .size(12.5)
                                        .strong()
                                        .color(pal().ink),
                                );
                                ui.label(RichText::new(&ff_detail).size(11.5).color(pal().muted));
                            });
                        });
                        ui.add_space(4.0);
                    });
                ui.add_space(7.0);
                let mut again = false;
                if self.gip_wheel() {
                    ui.add_space(2.0);
                    divider(ui);
                    ui.add_space(7.0);
                    ui.label(
                        RichText::new(
                            "This wheel is running Xbox firmware. Sending the PC-mode command is the only thing that changes it - the MODE button on the rim and a reboot do not.",
                        )
                        .size(11.0)
                        .color(pal().warn),
                    );
                    ui.add_space(4.0);
                    self.mode_switch_controls(ui, true);
                    ui.add_space(7.0);
                }
                ui.add_space(2.0);
                divider(ui);
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    again = button(ui, "Check again", true, true).clicked();
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Checking again only reads; no administrator rights are needed.")
                            .size(10.5)
                            .color(pal().muted),
                    );
                });
                if again {
                    self.open_diagnostics();
                }
            });
        if !open {
            self.diag = None;
        }
    }

    pub fn status_bar(&mut self, ui: &mut Ui) -> Option<PanelAction> {
        ui.horizontal(|ui| {
            dot(ui, if self.dirty { pal().warn } else { pal().ok_green }, 4.5);
            ui.label(
                RichText::new(if self.dirty {
                    String::from("Unsaved changes")
                } else if self.live {
                    format!(
                        "{} - live, {}",
                        self.dev_info
                            .as_ref()
                            .map(|d| d.product_name())
                            .unwrap_or_else(|| String::from("wheel")),
                        self.dev_info
                            .as_ref()
                            .map(|d| d.mode_label())
                            .unwrap_or("input")
                    )
                } else {
                    String::from("No wheel detected - simulated demo")
                })
                .size(11.5)
                .color(pal().muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if button(ui, "OK", true, true).clicked() {
                    self.apply_now();
                    self.saved = self.snapshot();
                    self.dirty = false;
                }
                if button(ui, "Apply", false, self.dirty).clicked() {
                    self.apply_now();
                    self.saved = self.snapshot();
                    self.dirty = false;
                }
                if button(ui, "Cancel", false, self.dirty).clicked() {
                    self.restore_snapshot();
                }
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                        .size(11.0)
                        .color(pal().muted),
                );
            });
        });
        None
    }

    pub fn ui(&mut self, ui: &mut Ui) -> Option<PanelAction> {
        let mut action = None;
        // header
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(Vec2::new(46.0, 46.0), Sense::hover());
            let p = ui.painter();
            p.circle_filled(r.center(), 21.0, pal().metal);
            p.circle_filled(r.center(), 14.0, pal().bg);
            p.line_segment(
                [
                    r.center() + Vec2::new(0.0, -13.0),
                    r.center() + Vec2::new(0.0, -4.0),
                ],
                Stroke::new(3.0, pal().metal_light),
            );
            p.line_segment(
                [
                    r.center() + Vec2::new(-11.0, 8.0),
                    r.center() + Vec2::new(0.0, 4.0),
                ],
                Stroke::new(3.0, pal().metal_light),
            );
            p.line_segment(
                [
                    r.center() + Vec2::new(11.0, 8.0),
                    r.center() + Vec2::new(0.0, 4.0),
                ],
                Stroke::new(3.0, pal().metal_light),
            );
            p.circle_filled(r.center(), 4.0, pal().accent);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("TMX/T150")
                        .size(21.0)
                        .strong()
                        .color(pal().ink),
                );
                ui.label(
                    RichText::new("Force feedback control panel")
                        .size(11.5)
                        .color(pal().muted),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button(RichText::new("\u{2630}").size(19.0), |ui| {
                    ui.set_min_width(212.0);
                    ui.label(
                        RichText::new("TEXT SIZE")
                            .size(10.0)
                            .strong()
                            .color(pal().muted),
                    );
                    for step in theme::SCALES {
                        let picked = (self.scale - step).abs() < 1e-3;
                        if ui
                            .selectable_label(picked, format!("{:.0}%", step * 100.0))
                            .clicked()
                        {
                            self.scale = step;
                            ui.ctx().set_zoom_factor(step);
                            theme::save_ui(self.theme, step);
                            ui.close_menu();
                        }
                    }
                    ui.add_space(4.0);
                    ui.separator();
                    ui.label(
                        RichText::new("THEME")
                            .size(10.0)
                            .strong()
                            .color(pal().muted),
                    );
                    for mode in theme::ThemeMode::ALL {
                        let picked = self.theme == mode;
                        if ui.selectable_label(picked, mode.name()).clicked() {
                            self.theme = mode;
                            theme::set_mode(mode);
                            theme::install_style(ui.ctx());
                            theme::save_ui(mode, self.scale);
                            ui.close_menu();
                        }
                    }
                    ui.add_space(4.0);
                    ui.separator();
                    if ui.button("Firmware & support").clicked() {
                        self.show_update = true;
                        ui.close_menu();
                    }
                    if ui.button("Setup wizard").clicked() {
                        action = Some(PanelAction::Setup);
                        ui.close_menu();
                    }
                })
                .response
                .on_hover_text("Display, theme and setup");
                ui.add_space(10.0);
                if self.live {
                    tag(ui, "CONNECTED", pal().ok_green);
                } else {
                    tag(ui, "SIMULATED", pal().warn);
                }
                ui.add_space(10.0);
                ui.label(
                    RichText::new(match &self.dev_info {
                        Some(info) => match device::firmware_version(info) {
                            Some(v) => format!("Firmware {v}   |   {}", info.mode_label()),
                            None => format!("Firmware n/a   |   {}", info.mode_label()),
                        },
                        None => String::from("Firmware n/a   |   no input device"),
                    })
                    .size(11.5)
                    .color(pal().muted),
                );
            });
        });
        ui.add_space(10.0);
        divider(ui);
        ui.add_space(8.0);

        // tabs
        ui.horizontal(|ui| {
            if tab_button(ui, "Test Input", self.tab == Tab::Input).clicked() {
                self.tab = Tab::Input;
            }
            ui.add_space(2.0);
            if tab_button(ui, "Test Forces", self.tab == Tab::Forces).clicked() {
                self.tab = Tab::Forces;
            }
            ui.add_space(2.0);
            if tab_button(ui, "Settings", self.tab == Tab::Settings).clicked() {
                self.tab = Tab::Settings;
            }
            ui.add_space(2.0);
            if tab_button(ui, "Profiles", self.tab == Tab::Profiles).clicked() {
                self.tab = Tab::Profiles;
            }
        });
        ui.add_space(8.0);

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                match self.tab {
                    Tab::Input => self.input_tab(ui),
                    Tab::Forces => self.forces_tab(ui),
                    Tab::Settings => self.settings_tab(ui),
                    Tab::Profiles => self.profiles_tab(ui),
                }
                ui.add_space(14.0);
            });

        if self.show_update {
            self.update_dialog(ui.ctx());
        }
        self.diagnostics_window(ui.ctx());
        self.notes_window(ui.ctx());
        action
    }

    fn column(ui: &mut Ui, w: f32, add: impl FnOnce(&mut Ui)) {
        // The column must grow with its content, not stop at the visible
        // height: otherwise a taller page (Settings) is clipped and the
        // scroll area never learns that there is more to scroll to.
        ui.vertical(|ui| {
            ui.set_width(w);
            add(ui);
        });
    }

    fn input_tab(&mut self, ui: &mut Ui) {
        if !self.live {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    dot(ui, pal().warn, 5.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Wheel not detected")
                                .size(13.0)
                                .strong()
                                .color(pal().ink),
                        );
                        ui.label(
                            RichText::new(
                                "The panel is showing simulated input. If the wheel is plugged in, check the cable, the USB port and the driver.",
                            )
                            .size(11.5)
                            .color(pal().muted),
                        );
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if button(ui, "Check drivers & connections", true, true).clicked() {
                            self.open_diagnostics();
                        }
                    });
                });
            });
            ui.add_space(10.0);
        }
        if self.live && self.gip_wheel() {
            card(ui, |ui| {
                ui.horizontal(|ui| {
                    dot(ui, pal().warn, 5.0);
                    ui.label(
                        RichText::new("Wheel is in Xbox mode")
                            .size(13.0)
                            .strong()
                            .color(pal().ink),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = if self.switching {
                            "Switching to PC mode..."
                        } else {
                            "Switch to PC mode"
                        };
                        if button(ui, label, true, !self.switching).clicked() {
                            self.start_mode_switch();
                        }
                    });
                });
                ui.add_space(2.0);
                small(
                    ui,
                    "It reports 044f:b67e, and the t150 module only handles 044f:b677 and 044f:b67f, so it cannot take over. This button sends the same USB command as the Windows driver; the wheel re-plugs itself as 044f:b67f and the settings become writable.",
                );
            });
            ui.add_space(10.0);
        }
        let total = ui.available_width();
        let gap = 12.0;
        let left_w = (total - gap) * 0.60;
        let right_w = total - gap - left_w;
        ui.horizontal_top(|ui| {
            Self::column(ui, left_w, |ui| {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        section_label(ui, "Wheel");
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if pill_button(ui, "Set Wheel Center", false).clicked() {
                                self.steer = 0.0;
                                self.last_input = 0.0;
                                self.status = String::from("Wheel centre stored.");
                            }
                        });
                    });
                    ui.add_space(2.0);
                    let wheel_angle = self.steer * self.rotation * 0.5 + self.force() * 6.0;
                    let wheel_resp = draw_wheel(
                        ui,
                        Vec2::new(ui.available_width(), 132.0),
                        wheel_angle,
                        self.leds_on,
                        self.glow,
                    );
                    if wheel_resp.dragged() {
                        self.steer = (self.steer + wheel_resp.drag_delta().x * 0.008)
                            .clamp(-1.0, 1.0);
                        self.last_input = 0.0;
                    }
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Rotation / Angle:")
                                .size(12.0)
                                .color(pal().muted),
                        );
                        let max = if self.limited_900() {
                            ROTATION_MAX_LIMITED
                        } else {
                            ROTATION_MAX
                        };
                        let w = (ui.available_width() - 150.0).max(100.0);
                        let before = self.rotation;
                        slider(ui, &mut self.rotation, ROTATION_MIN, max, w, pal().accent);
                        let mut edited = (self.rotation - before).abs() > f32::EPSILON;
                        if edited {
                            // Trackbar 1078 stores angle / 10; step is ten degrees.
                            self.rotation = mech::clamp_angle(
                                mech::slider_to_angle(mech::angle_to_slider(
                                    self.rotation.round() as i32,
                                )),
                                self.limited_900(),
                            ) as f32;
                        }
                        let resp = ui.add_sized(
                            [52.0, 20.0],
                            egui::TextEdit::singleline(&mut self.angle_text).char_limit(6),
                        );
                        // `sub_1800133b0` 0x1800159e2: parse digits/spaces, then
                        // clamp; unparseable text falls back to the live angle.
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            if let Some(v) = mech::parse_angle_text(&self.angle_text) {
                                self.rotation = mech::clamp_angle(v, self.limited_900()) as f32;
                                edited = true;
                            }
                        }
                        if !resp.has_focus() {
                            self.angle_text = format!("{:.0}", self.rotation);
                        }
                        ui.label(
                            RichText::new("°")
                                .size(12.0)
                                .strong()
                                .color(pal().accent_dark),
                        );
                        // Dialog 106 control 1003, whose template text is
                        // "Default" (`sub_1800133b0` @ 0x180015597).  It
                        // writes 900°, or 360° on a flagged model / LED id
                        // 3, 10, 14.
                        if ui
                            .small_button("Default")
                            .on_hover_text("Angle default: 900°, or 360° on the models that reset small.")
                            .clicked()
                        {
                            self.rotation = mech::reset_angle_deg(false, None, false) as f32;
                            self.angle_text = format!("{:.0}", self.rotation);
                            edited = true;
                        }
                        if edited {
                            self.dirty = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        small(ui, format!("{:.0}°", ROTATION_MIN));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            small(
                                ui,
                                format!(
                                    "{:.0}°",
                                    if self.limited_900() {
                                        ROTATION_MAX_LIMITED
                                    } else {
                                        ROTATION_MAX
                                    }
                                ),
                            );
                        });
                    });
                    let axis = (self.steer * 0.5 + 0.5).clamp(0.0, 1.0);
                    ui.add_space(2.0);
                    axis_bar(ui, axis, "X Axis", ui.available_width() - 70.0, pal().accent);
                    axis_bar(
                        ui,
                        (0.5 + 0.08 * (self.t * 0.7).sin()).clamp(0.0, 1.0),
                        "Y Axis",
                        ui.available_width() - 70.0,
                        Color32::from_rgb(96, 150, 200),
                    );
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        section_label(ui, "Pedals");
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if pill_button(ui, "Separate", self.pedals_separate()).clicked() {
                                self.set_pedals_separate(true);
                            }
                            if pill_button(ui, "Combined", !self.pedals_separate()).clicked() {
                                self.set_pedals_separate(false);
                            }
                        });
                    });
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if self.pedals_separate() {
                            self.pedal(ui, 0, "Clutch", Color32::from_rgb(120, 140, 160));
                            ui.add_space(14.0);
                            self.pedal(ui, 1, "Brake", Color32::from_rgb(190, 84, 72));
                            ui.add_space(14.0);
                            self.pedal(ui, 2, "Gas", Color32::from_rgb(64, 140, 96));
                        } else {
                            let combined = ((self.pedals[1] + self.pedals[2]) * 0.5).min(1.0);
                            let mut c = combined;
                            pedal_bar(ui, &mut c, "Brake + Gas", Color32::from_rgb(96, 126, 168));
                            ui.add_space(18.0);
                            self.pedal(ui, 0, "Clutch", Color32::from_rgb(120, 140, 160));
                        }
                    });
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        section_label(ui, "Buttons");
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            small(ui, "Click a button to light it");
                        });
                    });
                    ui.add_space(4.0);
                    for row in 0..2 {
                        ui.horizontal(|ui| {
                            for i in 0..6 {
                                let idx = row * 6 + i;
                                let lit = self.buttons[idx] > 0.0;
                                let (r, resp) = ui.allocate_exact_size(
                                    Vec2::new(32.0, 24.0),
                                    Sense::click(),
                                );
                                let fill = if lit {
                                    pal().accent
                                } else {
                                    Color32::from_rgb(238, 241, 245)
                                };
                                let p = ui.painter();
                                p.rect_filled(r, Rounding::same(5.0), fill);
                                p.rect_stroke(
                                    r,
                                    Rounding::same(5.0),
                                    Stroke::new(1.0, if lit { pal().accent_dark } else { pal().line }),
                                );
                                p.text(
                                    r.center(),
                                    Align2::CENTER_CENTER,
                                    format!("{}", idx + 1),
                                    FontId::proportional(12.0),
                                    if lit { Color32::WHITE } else { pal().muted },
                                );
                                if resp.clicked() {
                                    self.buttons[idx] = 0.45;
                                    self.last_input = 0.0;
                                }
                            }
                        });
                        ui.add_space(2.0);
                    }
                });
            });
            ui.add_space(gap);
            Self::column(ui, right_w, |ui| {
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        section_label(ui, "Point of View");
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            small(ui, "Click the pad");
                        });
                    });
                    ui.add_space(2.0);
                    let size = Vec2::new(ui.available_width(), 62.0);
                    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
                    let c = r.center();
                    let rad = 25.0;
                    let p = ui.painter();
                    p.circle_filled(c, rad + 10.0, Color32::from_rgb(238, 241, 245));
                    for i in 0..8 {
                        let a = i as f32 * std::f32::consts::TAU / 8.0
                            - std::f32::consts::FRAC_PI_2;
                        let dir = Vec2::new(a.cos(), a.sin());
                        let lit = self.pov == i;
                        p.circle_filled(
                            c + dir * (rad * 0.72),
                            9.0,
                            if lit { pal().accent } else { Color32::from_rgb(206, 213, 221) },
                        );
                    }
                    p.circle_filled(c, 17.0, Color32::from_rgb(226, 230, 236));
                    p.text(
                        c,
                        Align2::CENTER_CENTER,
                        match self.pov {
                            0 => "N",
                            1 => "NE",
                            2 => "E",
                            3 => "SE",
                            4 => "S",
                            5 => "SW",
                            6 => "W",
                            7 => "NW",
                            _ => "-",
                        },
                        FontId::proportional(12.0),
                        pal().ink,
                    );
                    if resp.clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let d = pos - c;
                            let mut a = d.y.atan2(d.x) + std::f32::consts::FRAC_PI_2;
                            if a < 0.0 {
                                a += std::f32::consts::TAU;
                            }
                            self.pov = ((a / std::f32::consts::TAU * 8.0).round() as usize) % 8;
                            self.last_input = 0.0;
                        }
                    }
                });
                ui.add_space(7.0);
                card(ui, |ui| {
                    section_label(ui, "Rim");
                    ui.add_space(2.0);
                    egui::ComboBox::from_id_salt("rim_pick")
                        .selected_text(RIMS[self.rim])
                        .width(ui.available_width())
                        .show_ui(ui, |ui| {
                            for (i, r) in RIMS.iter().enumerate() {
                                if ui.selectable_value(&mut self.rim, i, *r).clicked() {
                                    self.status = format!("{r} selected.");
                                }
                            }
                        });
                    ui.add_space(5.0);
                    section_label(ui, "Mode");
                    ui.horizontal(|ui| {
                        if pill_button(ui, "Normal", !self.advanced).clicked() {
                            self.advanced = false;
                            // `sub_1800133b0` WM_COMMAND 0x444: MessageBox
                            // 1213, then sub_180004750(1) -> property 0x1001
                            // = 3 (by the game), or sub_180004830(1) -> 0x1024
                            // = 1 on the older device path.
                            self.autocenter_game = true;
                            self.dirty = true;
                            self.status = format!(
                                "Normal Mode: property 0x{:04X} = {} (centring by the game). The original closes the panel after this.",
                                mech::AUTOCENTER_MODE_PROPERTY,
                                mech::AUTOCENTER_MODE_GAME
                            );
                            self.sync_wheel_forces();
                        }
                        if pill_button(ui, "Advanced", self.advanced).clicked() {
                            self.advanced = true;
                            // 0x445: sub_180004750(0) -> 0x1001 = 6, or 5 on
                            // a 900-degree wheel; the older path writes 0x1024
                            // = 0.
                            self.autocenter_game = false;
                            self.dirty = true;
                            self.status = format!(
                                "Advanced Mode: property 0x{:04X} = {} ({} on a 900° wheel). The original closes the panel after this.",
                                mech::AUTOCENTER_MODE_PROPERTY,
                                mech::AUTOCENTER_MODE_WHEEL,
                                mech::AUTOCENTER_MODE_WHEEL_900
                            );
                            self.sync_wheel_forces();
                        }
                    });
                    small(
                        ui,
                        if self.advanced {
                            "Advanced Mode (Default): sets the wheel's centring mode and restarts the panel."
                        } else {
                            "Normal Mode: sets centring by the game and restarts the panel."
                        },
                    );
                    small(
                        ui,
                        format!(
                            "Original: control 1092/1093 -> property 0x{:04X} = {} / {}; MessageBox 1213 first, then WM_CLOSE.",
                            mech::AUTOCENTER_MODE_PROPERTY,
                            mech::AUTOCENTER_MODE_GAME,
                            mech::AUTOCENTER_MODE_WHEEL
                        ),
                    );
                });
                ui.add_space(7.0);
                card(ui, |ui| {
                    section_label(ui, "LED Settings");
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        if button(ui, "Test LED", false, true).clicked() {
                            self.leds_on = !self.leds_on;
                            self.glow = 1.0;
                        }
                        ui.label(
                            RichText::new(format!("{:.0} %", self.led_intensity))
                                .size(12.0)
                                .color(pal().muted),
                        );
                    });
                    slider(
                        ui,
                        &mut self.led_intensity,
                        0.0,
                        GAIN_MAX,
                        ui.available_width(),
                        Color32::from_rgb(200, 160, 60),
                    );
                    // Dialog 106 control 1106 is a static label (class
                    // "Static"), not a checkbox: `sub_1800133b0` fills it with
                    // `"%d %%    "` (0x18004ad10) from the LED intensity at
                    // 0x180015d6b.  Its template caption "Static" is only the
                    // placeholder text before the first repaint.
                    ui.label(
                        RichText::new(format!("{:.0} %    ", self.led_intensity))
                            .size(12.0)
                            .color(pal().muted),
                    );
                });
                ui.add_space(7.0);
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        section_label(ui, "Device");
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if button(ui, "Rescan", false, true).clicked() {
                                self.device.rescan();
                                self.dev_info = self.device.info.clone();
                                self.live = self.device.snapshot.connected;
                                self.adopt_wheel_settings();
                                self.synced_node =
                                    self.dev_info.as_ref().map(|i| i.node.clone());
                                self.status = if self.live {
                                    String::from("Wheel re-detected.")
                                } else {
                                    String::from("No wheel found. Open Check drivers & connections.")
                                };
                            }
                            if button(ui, "Check drivers & connections", false, true).clicked() {
                                self.open_diagnostics();
                            }
                        });
                    });
                    ui.add_space(2.0);
                    if let Some(info) = &self.dev_info {
                        if info.driver != "hid-t150" {
                            ui.add_space(2.0);
                            small(
                                ui,
                                if device::is_gip_wheel(info.product) {
                                    "This wheel is in Xbox firmware (044f:b67e). The t150 module only handles 044f:b677 and 044f:b67f, so modprobe and a restart cannot change it, and the MODE button on the rim does not either."
                                } else {
                                    "Input is live, settings are read-only in this mode. Set the wheel to PC mode and reconnect so the hid-t150 driver can carry them."
                                },
                            );
                        }
                    }
                    if self.gip_wheel() {
                        ui.add_space(4.0);
                        self.mode_switch_controls(ui, true);
                    }
                    let rows: Vec<(String, String)> = match &self.dev_info {
                        Some(info) => vec![
                            (String::from("Device"), info.product_name()),
                            (
                                String::from("Detected as"),
                                dedup_words(info.kernel_name()),
                            ),
                            (
                                String::from("Node"),
                                info.node.display().to_string(),
                            ),
                            (String::from("Driver"), info.mode_label().to_string()),
                            (
                                String::from("Firmware"),
                                device::firmware_version(info)
                                    .map(|v| v.to_string())
                                    .unwrap_or_else(|| String::from("n/a")),
                            ),
                            (String::from("Events"), self.dev_events.to_string()),
                            (String::from("Rim"), RIMS[self.rim].to_string()),
                        ],
                        None => vec![
                            (String::from("Device"), String::from("none")),
                            (String::from("Node"), String::from("-")),
                            (String::from("Driver"), String::from("-")),
                            (String::from("Rim"), RIMS[self.rim].to_string()),
                            (String::from("Version"), String::from(env!("CARGO_PKG_VERSION"))),
                        ],
                    };
                    for (k, v) in rows {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(k).size(11.5).color(pal().muted));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(v).size(11.5).color(pal().ink));
                            });
                        });
                    }
                    ui.add_space(4.0);
                    ui.hyperlink_to(
                        RichText::new("Official TMX support page").size(11.0),
                        mech::TMX_SUPPORT_URL,
                    );
                    if let Some(info) = &self.dev_info {
                        if info.has_generic_kernel_name() {
                            ui.add_space(4.0);
                            small(
                                ui,
                                "KDE Plasma and Steam may list this wheel as \"Generic X-Box pad\". That is the same device in Xbox mode, not a separate controller.",
                            );
                        }
                    }
                });
            });
        });
    }

    fn pedals_separate(&self) -> bool {
        self.sep
    }

    fn set_pedals_separate(&mut self, sep: bool) {
        self.sep = sep;
        self.dirty = true;
        self.status = format!(
            "{} Property 0x{:04X} = {}.",
            if sep {
                "Pedals report as separate axes."
            } else {
                "Pedals report as one combined axis."
            },
            mech::PEDAL_MODE_PROPERTY,
            mech::pedal_mode_property_value(sep)
        );
    }

    fn pedal(&mut self, ui: &mut Ui, idx: usize, label: &str, color: Color32) {
        let before = self.pedals[idx];
        let mut v = before;
        let resp = pedal_bar(ui, &mut v, label, color);
        if resp.dragged() || resp.clicked() {
            self.pedals[idx] = v;
            self.last_input = 0.0;
        }
    }

    fn forces_tab(&mut self, ui: &mut Ui) {
        let total = ui.available_width();
        let gap = 12.0;
        let left_w = (total - gap) * 0.36;
        let right_w = total - gap - left_w;
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(
                        "Preview each force-feedback effect. Games send real forces through the kernel driver.",
                    )
                    .size(12.5)
                    .color(pal().ink),
                );
            });
        });
        ui.add_space(10.0);
        ui.horizontal_top(|ui| {
            Self::column(ui, left_w, |ui| {
                card(ui, |ui| {
                    section_label(ui, "Force Feedback");
                    ui.add_space(4.0);
                    for row in 0..6 {
                        ui.horizontal(|ui| {
                            for col in 0..2 {
                                let i = row * 2 + col;
                                let playing = self
                                    .effect
                                    .as_ref()
                                    .map(|p| p.idx == i)
                                    .unwrap_or(false);
                                if pill_button(ui, EFFECTS[i].name, playing).clicked() {
                                    self.start_effect(i);
                                }
                            }
                        });
                        ui.add_space(3.0);
                    }
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    section_label(ui, "Wheel output");
                    ui.add_space(3.0);
                    if self.ffb.is_ready() {
                        ui.horizontal(|ui| {
                            dot(ui, pal().ok_green, 4.0);
                            ui.label(
                                RichText::new(format!(
                                    "Periodic effects ready - {} effect slots",
                                    self.ffb.effects_max
                                ))
                                .size(11.5)
                                .color(pal().ink),
                            );
                        });
                        small(
                            ui,
                            format!(
                                "{}  -  {}",
                                self.ffb
                                    .node()
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_default(),
                                ffb::capability_summary(self.ffb.caps)
                            ),
                        );
                        if let Some(name) = self.ffb.last_played() {
                            small(
                                ui,
                                format!(
                                    "Last sent: {name} ({} of {} effect parts)",
                                    self.ffb.last_count(),
                                    self.ffb.last_total()
                                ),
                            );
                        }
                    } else if self.live {
                        ui.horizontal_top(|ui| {
                            dot(ui, pal().warn, 4.0);
                            ui.label(
                                RichText::new(
                                    self.ffb
                                        .error
                                        .clone()
                                        .unwrap_or_else(|| String::from(
                                            "This wheel has no force-feedback channel.",
                                        )),
                                )
                                .size(11.0)
                                .color(pal().muted),
                            );
                        });
                        small(ui, "The effect still plays on screen.");
                    } else {
                        small(
                            ui,
                            "No wheel connected; effects are shown on screen only.",
                        );
                    }
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    section_label(ui, "How it works");
                    ui.add_space(2.0);
                    for line in [
                        "Each effect is a short force pattern; the graph shows its shape.",
                        "The slider gain scales what you feel.",
                        "The wheel's own force feedback is driven by the game through the driver.",
                        "Effects stop by themselves when the pattern ends.",
                    ] {
                        ui.label(RichText::new(format!("-  {line}")).size(11.5).color(pal().muted));
                    }
                });
            });
            ui.add_space(gap);
            Self::column(ui, right_w, |ui| {
                card(ui, |ui| {
                    let (title, desc) = if let Some(p) = &self.effect {
                        (EFFECTS[p.idx].name, EFFECTS[p.idx].desc)
                    } else {
                        (
                            "No effect playing",
                            "The wheel is idle. Choose an effect on the left or press the Test FFB button in Settings.",
                        )
                    };
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(title).size(17.0).strong().color(pal().ink));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if button(ui, "Stop", false, self.effect.is_some()).clicked() {
                                self.effect = None;
                                self.ffb.stop();
                                self.status = String::from("Force feedback stopped.");
                            }
                        });
                    });
                    ui.label(RichText::new(desc).size(12.0).color(pal().muted));
                    if let Some(pl) = &self.effect {
                        let fx = &EFFECTS[pl.idx];
                        small(
                            ui,
                            format!(
                                "Preset {}  -  {:.2} s  -  preset gain {:.0} %",
                                fx.preset,
                                fx.dur,
                                fx.gain as f32 / 100.0
                            ),
                        );
                    }
                    ui.add_space(8.0);
                    let h = 236.0;
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(rect, Rounding::same(6.0), Color32::from_rgb(22, 27, 34));
                    let inner = rect.shrink(12.0);
                    // grid
                    for i in 0..=8 {
                        let x = inner.left() + inner.width() * i as f32 / 8.0;
                        p.line_segment(
                            [Pos2::new(x, inner.top()), Pos2::new(x, inner.bottom())],
                            Stroke::new(1.0, Color32::from_rgb(38, 45, 55)),
                        );
                    }
                    for i in 0..=4 {
                        let y = inner.top() + inner.height() * i as f32 / 4.0;
                        p.line_segment(
                            [Pos2::new(inner.left(), y), Pos2::new(inner.right(), y)],
                            Stroke::new(1.0, Color32::from_rgb(38, 45, 55)),
                        );
                    }
                    let mid = inner.center().y;
                    p.line_segment(
                        [Pos2::new(inner.left(), mid), Pos2::new(inner.right(), mid)],
                        Stroke::new(1.0, Color32::from_rgb(60, 70, 84)),
                    );
                    // waveform of the active effect
                    let mut pts = Vec::new();
                    let n = 260;
                    let (kind, dur, preset_gain, tnow) = if let Some(pl) = &self.effect {
                        let fx = &EFFECTS[pl.idx];
                        (
                            fx.kind,
                            fx.dur,
                            fx.gain as f32 / mech::EFFECT_GAIN_FULL as f32,
                            pl.t,
                        )
                    } else {
                        (Wave::Field, 1.0, 1.0, 0.0)
                    };
                    for k in 0..n {
                        let f = k as f32 / (n - 1) as f32;
                        let vt = f * dur;
                        let amp = if self.effect.is_some() {
                            wave(kind, vt, dur) * preset_gain
                        } else {
                            0.02 * (vt * 40.0).sin()
                        } * (self.overall / GAIN_MAX);
                        let x = inner.left() + inner.width() * f;
                        let y = mid - amp.clamp(-1.0, 1.0) * inner.height() * 0.42;
                        pts.push(Pos2::new(x, y));
                    }
                    p.add(egui::Shape::line(
                        pts,
                        Stroke::new(1.8, Color32::from_rgb(90, 190, 255)),
                    ));
                    if self.effect.is_some() {
                        let x = inner.left() + inner.width() * (tnow / dur).clamp(0.0, 1.0);
                        p.line_segment(
                            [Pos2::new(x, inner.top()), Pos2::new(x, inner.bottom())],
                            Stroke::new(1.5, Color32::from_rgb(255, 190, 70)),
                        );
                    }
                    let label = if let Some(pl) = &self.effect {
                        format!(
                            "{}  -  {:.2} s / {:.2} s",
                            EFFECTS[pl.idx].name, pl.t, EFFECTS[pl.idx].dur
                        )
                    } else {
                        String::from("Idle output")
                    };
                    p.text(
                        Pos2::new(inner.left() + 2.0, inner.top() + 4.0),
                        Align2::LEFT_TOP,
                        label,
                        FontId::monospace(11.0),
                        Color32::from_rgb(150, 190, 220),
                    );
                    // small wheel that reacts
                    let wc = Pos2::new(inner.right() - 70.0, inner.bottom() - 62.0);
                    let wr = 44.0;
                    p.circle_stroke(wc, wr, Stroke::new(7.0, Color32::from_rgb(80, 90, 104)));
                    let a = self.force() * 0.9
                        + if let Some(pl) = &self.effect {
                            wave(EFFECTS[pl.idx].kind, pl.t, EFFECTS[pl.idx].dur) * 0.6
                        } else {
                            0.0
                        };
                    for k in 0..3 {
                        let ang = a
                            + std::f32::consts::FRAC_PI_2
                            + k as f32 * std::f32::consts::TAU / 3.0;
                        p.line_segment(
                            [
                                wc,
                                wc + Vec2::new(ang.cos(), ang.sin()) * (wr - 4.0),
                            ],
                            Stroke::new(5.0, Color32::from_rgb(110, 122, 138)),
                        );
                    }
                    p.circle_filled(wc, 9.0, Color32::from_rgb(120, 135, 155));
                    if self.effect.is_some() {
                        ui.add_space(8.0);
                        let progress = self
                            .effect
                            .as_ref()
                            .map(|pl| (pl.t / EFFECTS[pl.idx].dur).clamp(0.0, 1.0))
                            .unwrap_or(0.0);
                        let (pr, _) = ui.allocate_exact_size(
                            Vec2::new(ui.available_width(), 8.0),
                            Sense::hover(),
                        );
                        ui.painter()
                            .rect_filled(pr, Rounding::same(4.0), pal().track);
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                pr.min,
                                Vec2::new(pr.width() * progress, pr.height()),
                            ),
                            Rounding::same(4.0),
                            pal().accent,
                        );
                    }
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    section_label(ui, "Effect chain");
                    ui.add_space(2.0);
                    for (name, val) in [
                        ("Overall strength", self.overall),
                        ("Constant", self.constant),
                        ("Periodic", self.periodic),
                        ("Spring", self.spring),
                        ("Damper", self.damper),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(name).size(11.5).color(pal().muted));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(format!("{:.0} %", val))
                                        .size(11.5)
                                        .color(pal().ink),
                                );
                            });
                        });
                    }
                });
            });
        });
    }

    /// Make a profile current: copy its tuning into the page and, when a wheel
    /// is attached, write the wheel-side part (rotation, overall, auto-centre).
    fn activate_profile(&mut self, index: usize) {
        self.profiles.active = index.min(self.profiles.list.len() - 1);
        let profile = self.profiles.current().clone();
        self.bridge.settings = profile.settings.clone();
        self.profile_appid = profile.appid.map(|v| v.to_string()).unwrap_or_default();
        self.rotation = profile.rotation;
        self.overall = profile.overall;
        self.autocenter_game = profile.autocenter_game;
        self.autocenter_strength = profile.autocenter_strength;
        self.bridge.push_settings();
        let mut pad_note = "";
        if self.dev_info.is_some() {
            self.apply_now();
            // A profile that needs the pad starts it, and a native profile
            // stops it again so the game sees the real wheel.
            if profile.needs_bridge && !self.bridge.live.running {
                if let Some(info) = self.dev_info.clone() {
                    self.bridge.start(&info);
                }
                pad_note = ", virtual pad started";
            } else if !profile.needs_bridge && self.bridge.live.running {
                self.bridge.stop();
                pad_note = ", virtual pad stopped";
            }
        }
        self.status = format!(
            "Profile {}: {}{}",
            profile.name,
            if self.dev_info.is_some() {
                "tuning and wheel settings applied"
            } else {
                "kept until a wheel is connected"
            },
            pad_note
        );
    }

    /// Scan the Steam libraries and add a profile for every installed racing
    /// game that does not have one yet.
    fn scan_steam(&mut self) {
        let (games, problems) = steam::scan();
        let racing = games.iter().filter(|g| steam::is_racing(&g.name)).count();
        let mut added: Vec<String> = Vec::new();
        for game in &games {
            if !steam::is_racing(&game.name) {
                continue;
            }
            if self
                .profiles
                .list
                .iter()
                .any(|profile| profile.appid == Some(game.appid))
            {
                continue;
            }
            let hint = steam::suggest(&game.name);
            let mut profile = bridge::Profile::new(&game.name);
            profile.appid = Some(game.appid);
            profile.rotation = hint.rotation;
            profile.overall = hint.overall;
            profile.settings.gain = hint.gain;
            profile.settings.curve = hint.curve;
            profile.settings.deadzone = hint.deadzone;
            profile.needs_bridge = hint.bridge;
            let note = if hint.source.starts_with("default") {
                String::new()
            } else {
                format!(" ({})", hint.source)
            };
            added.push(format!("{}{note}", game.name));
            self.profiles.list.push(profile);
        }

        if !added.is_empty() {
            let index = self.profiles.list.len() - 1;
            self.activate_profile(index);
            bridge::save_profiles(&self.profiles);
        }

        let mut report = if added.is_empty() {
            format!("Steam: {racing} racing game(s) found, nothing new to add.")
        } else {
            format!(
                "Steam: added {}. {racing} racing game(s) found.",
                added.join(", ")
            )
        };
        if !problems.is_empty() {
            report.push_str(&format!(" {} library path(s) unreadable.", problems.len()));
        }
        self.notes_report = report;
        self.status = self.notes_report.clone();
    }

    /// Known setups for specific games.  Kept out of the profile list on
    /// purpose: a profile only exists once the person creates one from here.
    fn notes_window(&mut self, ctx: &egui::Context) {
        if !self.show_notes {
            return;
        }
        let mut open = self.show_notes;
        let (vendor, product) = match &self.dev_info {
            Some(info) => (info.vendor, info.product),
            None => (0x044f, 0xb67f),
        };
        let option = bridge::launch_option(vendor, product);
        let mut create: Option<usize> = None;
        let mut scan = false;
        egui::Window::new("Game notes")
            .id(Id::new("game_notes"))
            .collapsible(false)
            .resizable(true)
            .default_width(580.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.set_min_width(540.0);
                small(
                    ui,
                    "What is known about specific games. Nothing here is applied automatically - \
                     each row can create a profile with the suggested values.",
                );
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    if button(ui, "Scan Steam library", true, true).clicked() {
                        scan = true;
                    }
                    ui.add_space(4.0);
                    small(
                        ui,
                        "Adds a profile for every installed racing game that lacks one.",
                    );
                });
                if !self.notes_report.is_empty() {
                    ui.add_space(3.0);
                    ui.label(RichText::new(&self.notes_report).size(11.0).color(pal().muted));
                }
                ui.add_space(9.0);
                section_label(ui, "Launch option for this wheel");
                ui.label(RichText::new(&option).monospace().size(11.5).color(pal().ink));
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if button(ui, "Copy", false, true).clicked() {
                        ui.ctx().copy_text(option.clone());
                    }
                    ui.add_space(4.0);
                    small(ui, "Only the games marked NEEDS BRIDGE use it.");
                });
                ui.add_space(9.0);
                divider(ui);
                ui.add_space(7.0);
                egui::ScrollArea::vertical()
                    .max_height(430.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for (index, reference) in steam::REFERENCES.iter().enumerate() {
                            ui.horizontal_top(|ui| {
                                dot(ui, if reference.bridge { pal().warn } else { pal().ok_green }, 4.5);
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new(reference.name)
                                                .size(12.5)
                                                .strong()
                                                .color(pal().ink),
                                        );
                                        tag(
                                            ui,
                                            if reference.bridge { "NEEDS BRIDGE" } else { "NATIVE" },
                                            if reference.bridge { pal().warn } else { pal().ok_green },
                                        );
                                        if let Some(id) = reference.appid {
                                            ui.label(
                                                RichText::new(format!("AppID {id}"))
                                                    .size(10.5)
                                                    .color(pal().muted),
                                            );
                                        }
                                        ui.label(
                                            RichText::new(format!("{:.0} deg", reference.rotation))
                                                .size(10.5)
                                                .color(pal().muted),
                                        );
                                    });
                                    ui.label(
                                        RichText::new(reference.ingame).size(11.0).color(pal().muted),
                                    );
                                    ui.label(
                                        RichText::new(reference.caveat).size(10.0).color(pal().muted),
                                    );
                                    ui.hyperlink_to(
                                        RichText::new("briankoponen.com").size(10.0),
                                        reference.url,
                                    );
                                    ui.add_space(3.0);
                                    if button(ui, "Create profile", false, true).clicked() {
                                        create = Some(index);
                                    }
                                });
                            });
                            ui.add_space(9.0);
                        }
                    });
            });
        if scan {
            self.scan_steam();
        }
        if let Some(index) = create {
            let reference = &steam::REFERENCES[index];
            self.profiles.list.push(bridge::Profile::from_reference(reference));
            self.profiles.active = self.profiles.list.len() - 1;
            self.bridge.settings = self.profiles.current().settings.clone();
            self.profile_appid = self
                .profiles
                .current()
                .appid
                .map(|v| v.to_string())
                .unwrap_or_default();
            bridge::save_profiles(&self.profiles);
            self.bridge.push_settings();
            self.status = format!("Profile created: {}", reference.name);
            self.show_notes = false;
        }
        if !open {
            self.show_notes = false;
        }
    }

    /// Profiles: one tuning set per game, plus the virtual pad controls.
    fn tier_color(tier: protondb::Tier) -> Color32 {
        match tier {
            protondb::Tier::Platinum => Color32::from_rgb(110, 165, 205),
            protondb::Tier::Gold => Color32::from_rgb(196, 156, 60),
            protondb::Tier::Silver => Color32::from_rgb(150, 152, 158),
            protondb::Tier::Bronze => Color32::from_rgb(168, 118, 78),
            protondb::Tier::Borked => Color32::from_rgb(198, 78, 78),
            protondb::Tier::Pending | protondb::Tier::Unknown => pal().muted,
        }
    }

    fn profiles_tab(&mut self, ui: &mut Ui) {
        let mut changed = false;
        let mut switched = false;
        let mut apply_reference = false;
        let mut bridge_toggled = false;
        let mut check_protondb = false;

        card(ui, |ui| {
            section_label(ui, "Profile");
            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                let current = self.profiles.current().name.clone();
                egui::ComboBox::from_id_salt("profile_select")
                    .selected_text(current)
                    .width(230.0)
                    .show_ui(ui, |ui| {
                        for index in 0..self.profiles.list.len() {
                            let label = self.profiles.list[index].name.clone();
                            if ui
                                .selectable_value(&mut self.profiles.active, index, label)
                                .changed()
                            {
                                switched = true;
                            }
                        }
                    });
                if self.profiles.current().needs_bridge {
                    tag(ui, "BRIDGE", pal().warn);
                }
                if button(ui, "Add", false, true).clicked() {
                    self.profiles.add("New profile");
                    switched = true;
                }
                if button(ui, "Duplicate", false, true).clicked() {
                    self.profiles.duplicate();
                    switched = true;
                }
                if button(ui, "Delete", false, self.profiles.list.len() > 1).clicked() {
                    self.profiles.remove();
                    switched = true;
                }
                if button(ui, "Scan Steam", false, true).clicked() {
                    self.scan_steam();
                }
                if button(ui, "Game notes", false, true).clicked() {
                    self.show_notes = true;
                }
            });
            if self.profiles.list.len() == 1 {
                ui.add_space(3.0);
                ui.label(
                    RichText::new(
                        "Only the default profile so far. Press Scan Steam to add one per \
                         installed racing game, or Add to make one by hand.",
                    )
                    .size(11.0)
                    .color(pal().warn),
                );
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Name").size(12.0).color(pal().ink));
                ui.add_space(8.0);
                if ui
                    .add_sized(
                        [230.0, 24.0],
                        egui::TextEdit::singleline(&mut self.profiles.current_mut().name),
                    )
                    .changed()
                {
                    changed = true;
                }
            });
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Steam AppID").size(12.0).color(pal().ink));
                if ui
                    .add_sized(
                        [110.0, 24.0],
                        egui::TextEdit::singleline(&mut self.profile_appid),
                    )
                    .changed()
                {
                    self.profiles.current_mut().appid =
                        self.profile_appid.trim().parse::<u32>().ok();
                    changed = true;
                }
                if let Some(id) = self.profiles.current().appid {
                    if button(ui, "Launch", false, true).clicked() {
                        let _ = std::process::Command::new("steam")
                            .arg(format!("steam://rungameid/{id}"))
                            .spawn();
                    }
                }
            });
            small(
                ui,
                "AppID is only used for the Launch button, the ProtonDB lookup and the automatic profile switch.",
            );
            ui.add_space(4.0);
            let appid = self.profiles.current().appid;
            ui.horizontal(|ui| {
                match appid.and_then(|id| self.protondb.get(id)) {
                    Some(status) => {
                        tag(
                            ui,
                            &format!("PROTONDB {}", status.tier.name().to_uppercase()),
                            Self::tier_color(status.tier),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{} report(s), trending {}{}",
                                status.total,
                                status.trending.name(),
                                if status.tier.is_bad() {
                                    " - it will not run under Proton"
                                } else {
                                    ""
                                }
                            ))
                            .size(10.5)
                            .color(if status.tier.is_bad() {
                                pal().warn
                            } else {
                                pal().muted
                            }),
                        );
                    }
                    None => small(ui, "ProtonDB: not checked yet."),
                }
                ui.add_space(4.0);
                if button(ui, "Check", false, appid.is_some()).clicked() {
                    check_protondb = true;
                }
            });
            if let Some(reference) = steam::reference_for(&self.profiles.current().name) {
                let rotation = self.profiles.current().rotation;
                let overall = self.profiles.current().overall;
                if (rotation - reference.rotation).abs() > 0.5
                    || (overall - reference.overall).abs() > 0.5
                {
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!(
                                "Reference says {:.0} deg and overall {:.0}%, this profile has \
                                 {:.0} deg and {:.0}%.",
                                reference.rotation, reference.overall, rotation, overall
                            ))
                            .size(11.0)
                            .color(pal().warn),
                        );
                        if button(ui, "Apply reference", false, true).clicked() {
                            apply_reference = true;
                        }
                    });
                }
            }
            ui.add_space(4.0);
            if ui
                .checkbox(
                    &mut self.profiles.follow,
                    "Switch profile automatically when the game starts",
                )
                .changed()
            {
                changed = true;
            }
            small(
                ui,
                match self.running_app {
                    Some(id) => format!("Steam is running app {id}."),
                    None => String::from("No Steam game is running."),
                },
            );
            ui.add_space(6.0);
            divider(ui);
            ui.add_space(6.0);
            if ui
                .checkbox(
                    &mut self.profiles.current_mut().needs_bridge,
                    "Use the virtual Xbox pad with this game",
                )
                .changed()
            {
                changed = true;
                bridge_toggled = true;
            }
            small(
                ui,
                "Needed by the Frostbite titles under Wine. For untested games this is only a \
                 guess: turn it off and see whether the game finds the wheel itself.",
            );
        });
        if bridge_toggled {
            let index = self.profiles.active;
            self.activate_profile(index);
        }
        if check_protondb {
            if let Some(appid) = self.profiles.current().appid {
                if self.protondb_rx.is_none() {
                    self.protondb_rx = Some(protondb::spawn(appid));
                }
            }
        }
        if switched {
            let index = self.profiles.active;
            self.activate_profile(index);
            bridge::save_profiles(&self.profiles);
        }
        if apply_reference {
            let name = self.profiles.current().name.clone();
            if let Some(reference) = steam::reference_for(&name) {
                self.rotation = reference.rotation;
                self.overall = reference.overall;
                self.profiles.current_mut().rotation = reference.rotation;
                self.profiles.current_mut().overall = reference.overall;
                bridge::save_profiles(&self.profiles);
                if self.dev_info.is_some() {
                    self.apply_now();
                }
                self.status = format!(
                    "Applied the reference for {}: {:.0} deg, overall {:.0}%.",
                    reference.name, reference.rotation, reference.overall
                );
            }
        }
        ui.add_space(10.0);

        card(ui, |ui| {
            section_label(ui, "Steering");
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Gain").size(12.0).color(pal().ink));
                ui.add_space(6.0);
                changed |= slider(ui, &mut self.bridge.settings.gain, 0.5, 2.0, 220.0, pal().accent)
                    .changed();
                ui.label(
                    RichText::new(format!("{:.2}x", self.bridge.settings.gain))
                        .size(11.5)
                        .color(pal().muted),
                );
            });
            small(
                ui,
                "Gain 1.30 gives full lock 77% of the way to the stop, in each direction.",
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Curve").size(12.0).color(pal().ink));
                ui.add_space(6.0);
                changed |= slider(ui, &mut self.bridge.settings.curve, 0.3, 1.5, 220.0, pal().accent)
                    .changed();
                ui.label(
                    RichText::new(format!("{:.2}", self.bridge.settings.curve))
                        .size(11.5)
                        .color(pal().muted),
                );
            });
            small(ui, "Below 1.0 is sharper near the centre; 1.0 is linear.");
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Deadzone").size(12.0).color(pal().ink));
                ui.add_space(2.0);
                let mut pct = self.bridge.settings.deadzone * 100.0;
                if slider(ui, &mut pct, 0.0, 30.0, 220.0, pal().accent).changed() {
                    self.bridge.settings.deadzone = pct / 100.0;
                    changed = true;
                }
                ui.label(
                    RichText::new(format!("{:.0}%", pct))
                        .size(11.5)
                        .color(pal().muted),
                );
            });
            small(ui, "Leave at 0% if the game's own deadzone is already off.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                small(ui, "Live wheel input (read only):");
                ui.label(
                    RichText::new(format!("{:+.0} %", self.steer * 100.0))
                        .size(11.5)
                        .color(pal().muted),
                );
            });
            bipolar_bar(ui, self.steer, "Steer", 240.0, pal().accent);
        });
        ui.add_space(10.0);

        card(ui, |ui| {
            section_label(ui, "Pedals");
            ui.add_space(3.0);
            if ui
                .checkbox(
                    &mut self.bridge.settings.swap_pedals,
                    "Swap throttle and brake",
                )
                .changed()
            {
                changed = true;
            }
            ui.add_space(6.0);
            small(ui, "Live pedal input (read only):");
            axis_bar(ui, self.pedals[2], "Gas pedal", 240.0, pal().ok_green);
            ui.add_space(3.0);
            axis_bar(ui, self.pedals[1], "Brake pedal", 240.0, pal().warn);
            ui.add_space(4.0);
            small(
                ui,
                "NFS Heat has a single brake action: it reverses while the car stands still. \
                 That is the game's design and no mapping can split it.",
            );
        });
        ui.add_space(10.0);

        card(ui, |ui| {
            section_label(ui, "Button mapping");
            ui.add_space(3.0);
            small(
                ui,
                "Left: the wheel button code from the kernel. Right: what the game receives.",
            );
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                for index in 0..bridge::WHEEL_BUTTONS.len() {
                    let code = bridge::WHEEL_BUTTONS[index];
                    ui.vertical(|ui| {
                        ui.set_width(94.0);
                        ui.label(RichText::new(format!("{code:#05x}")).size(10.5).color(pal().muted));
                        egui::ComboBox::from_id_salt(("bridge_map", index))
                            .selected_text(self.bridge.settings.map[index].name())
                            .width(86.0)
                            .show_ui(ui, |ui| {
                                for option in PadButton::ALL {
                                    if ui
                                        .selectable_value(
                                            &mut self.bridge.settings.map[index],
                                            option,
                                            option.name(),
                                        )
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });
                    });
                }
            });
            ui.add_space(4.0);
            small(
                ui,
                "Map a paddle to LT or RT to drive with it; the pedals keep working because the \
                 bridge takes the larger of the two.",
            );
        });
        ui.add_space(10.0);

        if changed {
            self.profiles.current_mut().settings = self.bridge.settings.clone();
            bridge::save_profiles(&self.profiles);
            self.bridge.push_settings();
        }
    }

    fn settings_tab(&mut self, ui: &mut Ui) {
        let running = self.bridge.live.running;
        let mut toggle_pad = false;
        card(ui, |ui| {
            section_label(ui, "Virtual Xbox pad");
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                dot(ui, self.bridge.live.dot(), 4.5);
                ui.label(
                    RichText::new(if running { "Running" } else { "Stopped" })
                        .size(12.5)
                        .strong()
                        .color(pal().ink),
                );
                if !self.bridge.live.message.is_empty() {
                    ui.add_space(4.0);
                    small(ui, &self.bridge.live.message);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let enabled = running || self.dev_info.is_some();
                    if button(ui, if running { "Stop" } else { "Start" }, false, enabled)
                        .clicked()
                    {
                        toggle_pad = true;
                    }
                });
            });
            ui.add_space(4.0);
            small(
                ui,
                "Only needed for games that refuse to see the wheel (Frostbite titles under Wine). \
                 Each profile has its own switch on the Profiles page, and a profile marked BRIDGE \
                 starts the pad by itself; this is the manual override.",
            );
        });
        if toggle_pad {
            if running {
                self.bridge.stop();
                self.status = String::from("Virtual pad stopped.");
            } else if let Some(info) = self.dev_info.clone() {
                self.bridge.push_settings();
                self.bridge.start(&info);
                self.status = String::from("Virtual pad started.");
            }
        }
        ui.add_space(10.0);

        // Dialog 118 control 1111 is the mode combo: `sub_180005f60` rebuilds
        // it with string ids 0x5e7..0x5ea (IDS_DDRIVE_MODE_1..4) and selects
        // the current index; the LED Default branch lands on index 1 (Sport).
        // Changing it in the original asks IDS_CHANGE_BUTTON_STATE 1213 first.
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Mode").size(12.0).strong().color(pal().ink));
                let before = self.mode;
                egui::ComboBox::from_id_salt("drive_mode")
                    .selected_text(mech::MODE_LABELS[self.mode])
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in mech::MODE_LABELS.iter().enumerate() {
                            ui.selectable_value(&mut self.mode, i, *name);
                        }
                    });
                if self.mode != before {
                    self.mode_notice = true;
                    self.status = format!(
                        "{} Property 0x{:04X} = {} ({}).",
                        mech::MODE_CHANGE_NOTICE,
                        mech::MODE_PROPERTY,
                        mech::mode_property_value(self.mode),
                        mech::MODE_LABELS[self.mode]
                    );
                }
            });
            small(ui, "Mode (change reactivity and damper)");
            small(
                ui,
                format!(
                    "Original: combo 1111 -> property 0x{:04X} = {} (0…3).",
                    mech::MODE_PROPERTY,
                    mech::mode_property_value(self.mode)
                ),
            );
            if self.mode_notice {
                ui.add_space(2.0);
                ui.label(
                    RichText::new(mech::MODE_CHANGE_NOTICE)
                        .size(11.0)
                        .color(pal().warn),
                );
            }
            ui.add_space(6.0);
            if ui
                .checkbox(&mut self.hexaled, "Default color and lighting effect")
                .changed()
            {
                self.dirty = true;
                self.status = format!(
                    "HexaLed pattern {}: property 0x{:04X} = {}.",
                    if self.hexaled { "on" } else { "off" },
                    mech::HEXALED_PROPERTY,
                    mech::hexaled_property_value(self.hexaled)
                );
            }
            small(
                ui,
                format!(
                    "Original control 1112 (0x458): BM_GETCHECK then sete, so 0x{:04X} = {} while unchecked and 0 while checked.",
                    mech::HEXALED_PROPERTY,
                    mech::HEXALED_UNCHECKED_VALUE
                ),
            );
        });
        ui.add_space(10.0);

        let (state_color, state_text) = if self.sleep_blocker.is_active() {
            (pal().ok_green, "Blocking idle sleep and screen locking now.")
        } else if !self.keep_awake {
            (
                pal().warn,
                "Automatic blocking is off; Plasma may sleep or lock during play.",
            )
        } else {
            (pal().muted, "Ready: blocking starts when the wheel sends input.")
        };
        card(ui, |ui| {
            ui.horizontal(|ui| {
                dot(ui, state_color, 5.0);
                ui.label(
                    RichText::new("Sleep & screen lock")
                        .size(13.0)
                        .strong()
                        .color(pal().ink),
                );
            });
            ui.label(
                RichText::new(
                    "KDE Plasma warns under Settings > Game Controller that the system may automatically go to sleep or lock the screen while the controller is in use, because the wheel sends no pointer or keyboard events.",
                )
                .size(11.5)
                .color(pal().muted),
            );
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                if ui
                    .checkbox(
                        &mut self.keep_awake,
                        "Keep the screen awake while the wheel is in use",
                    )
                    .changed()
                {
                    self.sleep_blocker.stop();
                }
                ui.add_space(6.0);
                ui.label(RichText::new(state_text).size(11.0).color(state_color));
            });
            if let Some(err) = self.sleep_blocker.error() {
                ui.label(RichText::new(err).size(10.5).color(pal().warn));
            }
            ui.add_space(1.0);
            ui.label(
                RichText::new(
                    "For games started outside this panel, allow the wheel as pointer and keyboard in KDE Settings > Game Controller.",
                )
                .size(10.5)
                .color(pal().muted),
            );
        });
        ui.add_space(10.0);

        let total = ui.available_width();
        let gap = 12.0;
        let left_w = (total - gap) * 0.6;
        let right_w = total - gap - left_w;
        ui.horizontal_top(|ui| {
            Self::column(ui, left_w, |ui| {
                card(ui, |ui| {
                    section_label(ui, "Master Gain Settings");
                    ui.add_space(2.0);
                    self.gain_row(ui, "Overall Strength of all forces", 0);
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    section_label(ui, "Detailed Gain Settings");
                    ui.add_space(2.0);
                    self.gain_row(ui, "Constant Gain", 1);
                    self.gain_row(ui, "Periodic Gain", 2);
                    self.gain_row(ui, "Spring Gain", 3);
                    self.gain_row(ui, "Damper Gain", 4);
                    ui.add_space(2.0);
                    small(ui, "Damper also affects the weight of the wheel.");
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Force Feedback Linearity")
                                .size(12.0)
                                .color(pal().ink),
                        );
                        small(ui, "0 % is fully linear");
                    });
                    // Dialog 118 has trackbar 1113 (0x459) and static 1114
                    // (0x45a), but no instruction in 4.24.2.0 reads either id:
                    // the WM_HSCROLL chain in `sub_180005f60` compares only
                    // 0x427/0x42d/0x42f/0x429/0x42b/0x431.  The control is
                    // real UI with no effect, so it is shown disabled.
                    ui.add_enabled_ui(mech::LINEARITY_CONNECTED, |ui| {
                        ui.horizontal(|ui| {
                            let w = ui.available_width() - 56.0;
                            slider(ui, &mut self.linearity, 0.0, GAIN_MAX, w, pal().accent);
                            ui.label(
                                RichText::new(format!("{:.0} %", self.linearity))
                                    .size(12.0)
                                    .color(pal().muted),
                            );
                        });
                    });
                    small(
                        ui,
                        format!(
                            "The original has this slider (control {}) and label ({}), but never reads them, so it changes nothing.",
                            mech::LINEARITY_CONTROL_ID,
                            mech::LINEARITY_LABEL_ID
                        ),
                    );
                    ui.add_space(4.0);
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    section_label(ui, "BOOST (Constant + Spring)");
                    ui.add_space(4.0);
                    // Dialog 118: the ON/OFF radios 1109/1110 sit in group box
                    // 1107 with the BOOST label 1108 (y 250-280), above the
                    // Auto-Center group 1101 which starts at y 285.
                    ui.horizontal(|ui| {
                        if ui.radio(self.boost, "ON").clicked() {
                            self.boost = true;
                            self.dirty = true;
                            self.status = format!(
                                "BOOST on: property 0x{:04X} = {}.",
                                mech::BOOST_PROPERTY,
                                mech::boost_property_value(true)
                            );
                        }
                        if ui.radio(!self.boost, "OFF (Default)").clicked() {
                            self.boost = false;
                            self.dirty = true;
                            self.status = format!(
                                "BOOST off: property 0x{:04X} = {}.",
                                mech::BOOST_PROPERTY,
                                mech::boost_property_value(false)
                            );
                        }
                    });
                    ui.add_space(2.0);
                    small(
                        ui,
                        "Adds constant and spring forces on top of the normal gains.",
                    );
                    small(
                        ui,
                        format!(
                            "Original: radios 1109/1110 -> property 0x{:04X} = {} / {}.",
                            mech::BOOST_PROPERTY,
                            mech::boost_property_value(true),
                            mech::boost_property_value(false)
                        ),
                    );
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    section_label(ui, "About these settings");
                    ui.add_space(2.0);
                    for line in [
                        "Master gain scales every force the wheel receives.",
                        "Constant and periodic shape the road and engine feel.",
                        "Spring and damper set how heavy the rim feels to turn.",
                        "Rotation is the lock-to-lock travel, 40 to 1080 degrees.",
                    ] {
                        ui.label(RichText::new(format!("-  {line}")).size(11.5).color(pal().muted));
                    }
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    ui.horizontal(|ui| {
                        if button(ui, "Default", false, true).clicked() {
                            self.reset_defaults();
                        }
                        if button(ui, "Test FFB", true, true).clicked() {
                            self.start_effect(0);
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            small(ui, concat!("version ", env!("CARGO_PKG_VERSION")));
                        });
                    });
                });
            });
            ui.add_space(gap);
            Self::column(ui, right_w, |ui| {
                card(ui, |ui| {
                    section_label(ui, "Auto-Center Settings");
                    ui.add_space(4.0);
                    if ui
                        .radio(self.autocenter_game, "by the game (Recommended)")
                        .clicked()
                    {
                        self.autocenter_game = true;
                        self.dirty = true;
                        self.sync_wheel_forces();
                    }
                    if ui.radio(!self.autocenter_game, "by the wheel").clicked() {
                        self.autocenter_game = false;
                        self.dirty = true;
                        self.sync_wheel_forces();
                    }
                    ui.add_space(8.0);
                    // The Windows page has no auto-center ON/OFF: its radios
                    // are 1075 by the game / 1076 by the wheel.  The ON/OFF
                    // pair 1109/1110 is the BOOST switch and lives in its card.
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Strength").size(12.0).color(pal().ink));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{:.0} %", self.autocenter_strength))
                                    .size(12.0)
                                    .strong()
                                    .color(pal().accent_dark),
                            );
                            let w = (ui.available_width() - 8.0).max(100.0);
                            // Control 1073: 50 % by default, disabled while the
                            // game owns the centring force.
                            let mut strength = self.autocenter_strength;
                            let resp = ui.add_enabled_ui(!self.autocenter_game, |ui| {
                                slider(ui, &mut strength, 0.0, GAIN_MAX, w, pal().accent)
                            });
                            if resp.inner.dragged() {
                                self.autocenter_strength =
                                    mech::clamp_gain(strength.round() as i32) as f32;
                                self.dirty = true;
                                self.sync_wheel_forces();
                            }
                        });
                    });
                    ui.add_space(4.0);
                    let mode = mech::autocenter_mode(!self.autocenter_game, self.limited_900());
                    small(
                        ui,
                        format!(
                            "{} (wheel mode {}).",
                            if self.autocenter_game {
                                "The game decides the centring force"
                            } else {
                                "The wheel pulls itself back to the centre"
                            },
                            mode
                        ),
                    );
                    small(
                        ui,
                        format!(
                            "Original: 0x{:04X} = {}, 0x{:04X} = {}; strength 0x{:04X} = {} ({} × 100).",
                            mech::AUTOCENTER_MODE_PROPERTY,
                            mode,
                            mech::AUTOCENTER_PROPERTY,
                            if self.autocenter_game { 0 } else { 1 },
                            mech::AUTOCENTER_STRENGTH_PROPERTY,
                            mech::device_gain_units(self.autocenter_strength.round() as i32),
                            self.autocenter_strength.round() as i32
                        ),
                    );
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    section_label(ui, "Rotation");
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        let max = if self.limited_900() {
                            ROTATION_MAX_LIMITED
                        } else {
                            ROTATION_MAX
                        };
                        let w = ui.available_width() - 78.0;
                        let before = self.rotation;
                        if slider(ui, &mut self.rotation, ROTATION_MIN, max, w, pal().accent).dragged() {
                            self.rotation = mech::clamp_angle(
                                mech::slider_to_angle(mech::angle_to_slider(
                                    self.rotation.round() as i32,
                                )),
                                self.limited_900(),
                            ) as f32;
                            self.angle_text = format!("{:.0}", self.rotation);
                            self.dirty = true;
                        } else if (self.rotation - before).abs() > f32::EPSILON {
                            self.dirty = true;
                        }
                        ui.label(
                            RichText::new(format!("{:.0}°", self.rotation))
                                .size(12.5)
                                .strong()
                                .color(pal().accent_dark),
                        );
                    });
                    ui.horizontal(|ui| {
                        small(ui, format!("{:.0}°", ROTATION_MIN));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            small(
                                ui,
                                format!(
                                    "{:.0}°",
                                    if self.limited_900() {
                                        ROTATION_MAX_LIMITED
                                    } else {
                                        ROTATION_MAX
                                    }
                                ),
                            );
                        });
                    });
                    small(
                        ui,
                        if self.limited_900() {
                            "This wheel's 900° flag limits the range to 40 - 900 degrees."
                        } else {
                            "The wheel reports the full 40 - 1080 degree range."
                        },
                    );
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    section_label(ui, "Rules from the original");
                    ui.add_space(2.0);
                    small(
                        ui,
                        format!(
                            "Angle {}–{}°, gains {}–{} %, and the same Default values as the Windows panel.",
                            mech::ANGLE_MIN_DEG,
                            mech::ANGLE_MAX_DEG,
                            mech::GAIN_MIN_PCT,
                            mech::GAIN_MAX_PCT
                        ),
                    );
                    egui::CollapsingHeader::new(
                        RichText::new("Show the exact rules").size(11.5).color(pal().muted),
                    )
                    .id_salt("rules_detail")
                    .default_open(false)
                    .show(ui, |ui| {
                        for line in [
                            format!(
                                "Angle: stored as angle / {} on the trackbar; range {}…{} / {}…{}, tick at {} ({}°).",
                                mech::ANGLE_TRACKBAR_DIVISOR,
                                mech::ANGLE_TRACKBAR_MIN_POS,
                                mech::ANGLE_TRACKBAR_MAX_900_POS,
                                mech::ANGLE_TRACKBAR_MIN_POS,
                                mech::ANGLE_TRACKBAR_MAX_1080_POS,
                                mech::ANGLE_TICK_POSITION,
                                mech::slider_to_angle(mech::ANGLE_TICK_POSITION)
                            ),
                            format!(
                                "Gains: wheel value = % × {}, one trackbar tick at {} %.",
                                mech::GAIN_DEVICE_SCALE,
                                mech::GAIN_TICK_PCT
                            ),
                            format!(
                                "Default angle: {}° on the flagged models / LED ids 3, 10, 14; {}° otherwise.",
                                mech::ANGLE_RESET_SMALL,
                                mech::ANGLE_MAX_DEG_LIMITED
                            ),
                            format!(
                                "The LED variant caps overall gain at {} %.",
                                mech::OVERALL_LED_CAP_PCT
                            ),
                            format!(
                                "LED Default: overall {} %, constant {} %, colour 0x{:04X}, mode {} ({}).",
                                mech::DEFAULT_OVERALL_LED_PCT,
                                mech::DEFAULT_CONSTANT_LED_PCT,
                                mech::LED_DEFAULT_COLOR,
                                mech::LED_DEFAULT_MODE_INDEX,
                                mech::MODE_LABELS[mech::LED_DEFAULT_MODE_INDEX]
                            ),
                            {
                                let (h, s, l) = mech::color_to_hsl(mech::LED_DEFAULT_COLOR);
                                let (p, sp, d) = mech::led_default_detail_gains();
                                format!(
                                    "LED Default colour 0x{:04X} = H {} S {} L {} → periodic {} %, spring {} %, damper {} %.",
                                    mech::LED_DEFAULT_COLOR, h, s, l, p, sp, d
                                )
                            },
                            String::from(
                                "On LED units the sliders are the strip colour: H = periodic × 255/100, S = spring × 255/100, L = damper × 255/100.",
                            ),
                            String::from(
                                "Force Feedback Linearity (1113) is in the original's dialog but never read by its code.",
                            ),
                            format!(
                                "Auto-centre: {} = game, {} = wheel, {} = wheel on 900° units.",
                                mech::AUTOCENTER_MODE_GAME,
                                mech::AUTOCENTER_MODE_WHEEL,
                                mech::AUTOCENTER_MODE_WHEEL_900
                            ),
                            format!(
                                "LED-variant defaults: overall {} %, constant {} %.",
                                mech::DEFAULT_OVERALL_LED_PCT,
                                mech::DEFAULT_CONSTANT_LED_PCT
                            ),
                            format!(
                                "Mode list: {}.",
                                mech::MODE_LABELS.join(", ")
                            ),
                            format!(
                                "Older path writes property 0x{:04X}; the newer one writes 0x{:04X}.",
                                mech::AUTOCENTER_PROPERTY,
                                mech::AUTOCENTER_MODE_PROPERTY
                            ),
                            format!(
                                "Vendor properties: mode 0x{:04X}, HexaLed 0x{:04X}, pattern 0x{:04X}, BOOST 0x{:04X}, pedals 0x{:04X}, auto-centre strength 0x{:04X}.",
                                mech::MODE_PROPERTY,
                                mech::HEXALED_PROPERTY,
                                mech::LED_PATTERN_PROPERTY,
                                mech::BOOST_PROPERTY,
                                mech::PEDAL_MODE_PROPERTY,
                                mech::AUTOCENTER_STRENGTH_PROPERTY
                            ),
                            format!(
                                "HexaLed writes 0x{:04X} = {} while the box is unchecked (BM_GETCHECK then sete).",
                                mech::HEXALED_PROPERTY,
                                mech::HEXALED_UNCHECKED_VALUE
                            ),
                            {
                                let (p, sp, d) = mech::led_opening_detail_gains();
                                format!(
                                    "LED units open with H×{}>>8, S×{}>>8, L×{}>>8 = {} / {} / {}, trackbars 0…{} / 0…{} / 0…{}.",
                                    mech::LED_OPENING_PERIODIC_FACTOR,
                                    mech::LED_OPENING_SPRING_FACTOR,
                                    mech::LED_OPENING_DAMPER_FACTOR,
                                    p,
                                    sp,
                                    d,
                                    mech::LED_PERIODIC_MAX,
                                    mech::LED_SPRING_MAX,
                                    mech::LED_DAMPER_MAX
                                )
                            },
                            format!(
                                "Constant gain 0x{:04X}; LED colour 0x{:04X}; both are read back by the page-open getters.",
                                mech::CONSTANT_GAIN_PROPERTY,
                                mech::LED_COLOR_PROPERTY
                            ),
                        ] {
                            ui.label(
                                RichText::new(format!("-  {line}")).size(10.5).color(pal().muted),
                            );
                        }
                    });
                });
                ui.add_space(10.0);
                card(ui, |ui| {
                    section_label(ui, "Wheel test");
                    ui.add_space(2.0);
                    let _ = draw_wheel(
                        ui,
                        Vec2::new(ui.available_width(), 130.0),
                        self.steer * self.rotation * 0.5 + self.force() * 6.0,
                        self.leds_on,
                        self.glow,
                    );
                    ui.add_space(2.0);
                    small(
                        ui,
                        "Close this control panel before starting a game so the game can take control of the wheel.",
                    );
                });
            });
        });
    }

    fn gain_row(&mut self, ui: &mut Ui, label: &str, which: u8) {
        let value = match which {
            0 => &mut self.overall,
            1 => &mut self.constant,
            2 => &mut self.periodic,
            3 => &mut self.spring,
            _ => &mut self.damper,
        };
        let before = *value;
        ui.horizontal(|ui| {
            ui.label(RichText::new(label).size(12.0).color(pal().ink));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} %",
                        mech::display_gain_pct(mech::device_gain_units(value.round() as i32))
                    ))
                    .size(12.0)
                    .strong()
                    .color(pal().accent_dark),
                );
                let w = (ui.available_width() - 8.0).max(120.0);
                let resp = slider(ui, value, 0.0, GAIN_MAX, w, pal().accent);
                resp.on_hover_text(format!(
                    "Wheel value {} (0..10000); the original ticks every {} %.",
                    mech::device_gain_units(value.round() as i32),
                    mech::GAIN_TICK_PCT
                ));
            });
        });
        if (*value - before).abs() > f32::EPSILON {
            // The wheel stores percent * 100 and the panel shows integer %.
            *value = mech::clamp_gain(value.round() as i32) as f32;
            self.dirty = true;
        }
        if which == 0 {
            let overall = self.overall;
            self.ffb.set_gain(overall);
        }
    }

    /// Firmware and support.  There is deliberately no "check for updates"
    /// here: this panel has no update server, and pretending to contact one
    /// was misleading.  What it shows is what can actually be read.
    fn update_dialog(&mut self, ctx: &egui::Context) {
        let detected = self.dev_info.as_ref().and_then(device::firmware_version);
        egui::Window::new("Firmware & support")
            .id(Id::new("update_dialog"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .default_width(460.0)
            .show(ctx, |ui| {
                ui.set_min_width(440.0);
                ui.add_space(6.0);
                match detected {
                    Some(firmware) if firmware == mech::TMX_LATEST_FIRMWARE => {
                        ui.label(
                            RichText::new(format!("Firmware {firmware} - up to date"))
                                .size(14.0)
                                .strong()
                                .color(pal().ok_green),
                        );
                    }
                    Some(firmware) => {
                        ui.label(
                            RichText::new(format!(
                                "Firmware {firmware} - the TMX support page lists {}",
                                mech::TMX_LATEST_FIRMWARE
                            ))
                            .size(14.0)
                            .strong()
                            .color(pal().warn),
                        );
                    }
                    None => {
                        ui.label(
                            RichText::new("No wheel connected - firmware unknown")
                                .size(14.0)
                                .strong()
                                .color(pal().muted),
                        );
                    }
                }
                ui.add_space(8.0);
                small(
                    ui,
                    concat!(
                        "TMX/T150 Control Panel ",
                        env!("CARGO_PKG_VERSION"),
                        " - independent reimplementation, not affiliated with Thrustmaster."
                    ),
                );
                ui.add_space(6.0);
                small(
                    ui,
                    "This panel does not contact an update server: the wheel firmware and the \
                     Windows driver package are updated from Thrustmaster's site, and the kernel \
                     side comes from your distribution.",
                );
                ui.add_space(8.0);
                ui.hyperlink_to(
                    RichText::new("Official TMX support page (firmware, drivers, manual)")
                        .size(11.0),
                    mech::TMX_SUPPORT_URL,
                );
                ui.add_space(10.0);
                if button(ui, "Close", true, true).clicked() {
                    self.show_update = false;
                }
            });
    }
}

fn pedal_bar(ui: &mut Ui, value: &mut f32, label: &str, color: Color32) -> Response {
    ui.vertical(|ui| {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(44.0, 58.0), Sense::click_and_drag());
        if resp.dragged() || resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                *value = (1.0 - (pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
            }
        }
        let p = ui.painter();
        p.rect_filled(rect, Rounding::same(4.0), Color32::from_rgb(239, 242, 246));
        p.rect_stroke(rect, Rounding::same(4.0), Stroke::new(1.0, pal().line));
        let h = rect.height() * value.clamp(0.0, 1.0);
        let fill = Rect::from_min_max(
            Pos2::new(rect.left(), rect.bottom() - h),
            Pos2::new(rect.right(), rect.bottom()),
        );
        p.rect_filled(fill, Rounding::same(4.0), color);
        p.text(
            Pos2::new(rect.center().x, rect.top() + 9.0),
            Align2::CENTER_CENTER,
            format!("{:.0}", *value * 100.0),
            FontId::proportional(11.0),
            Color32::from_rgb(60, 70, 84),
        );
        ui.add_space(2.0);
        ui.label(RichText::new(label).size(10.5).color(pal().muted))
    })
    .inner
}

#[cfg(test)]
mod panel_tests {
    use super::*;

    #[test]
    fn cancel_restores_the_opening_gain_block() {
        let mut p = Panel::new();
        let opened = p.snapshot();
        p.overall = 12.0;
        p.constant = 34.0;
        p.autocenter_game = false;
        p.boost = true;
        p.dirty = true;

        p.restore_snapshot();

        assert_eq!(p.overall, opened.overall);
        assert_eq!(p.constant, opened.constant);
        assert!(p.autocenter_game);
        assert!(!p.boost);
        assert!(!p.dirty);
    }

    #[test]
    fn repeated_vendor_word_is_shown_once() {
        assert_eq!(
            dedup_words("Thrustmaster Thrustmaster TMX Racing Wheel"),
            "Thrustmaster TMX Racing Wheel"
        );
        assert_eq!(dedup_words("Generic X-Box pad"), "Generic X-Box pad");
    }

    #[test]
    fn reset_defaults_matches_the_default_handler() {
        let mut p = Panel::new();
        p.overall = 1.0;
        p.constant = 1.0;
        p.autocenter_game = false;
        p.boost = true;
        p.reset_defaults();

        // The 900-degree branch of sub_180005f60.
        assert_eq!(p.overall, mech::DEFAULT_OVERALL_LIMITED_PCT as f32);
        assert_eq!(p.constant, mech::DEFAULT_DETAIL_GAIN_PCT as f32);
        assert_eq!(
            p.autocenter_strength,
            mech::DEFAULT_AUTOCENTER_STRENGTH_PCT as f32
        );
        assert!(p.autocenter_game);
        assert!(!p.boost);
    }
}
