//! Virtual Xbox 360 pad bridge.
//!
//! Frostbite games (Need for Speed Heat, The Crew 2) do not recognise the
//! wheel as a wheel under Wine/Proton: their DirectInput probe only accepts
//! devices that a Windows driver has registered, so the game falls back to
//! keyboard-only.  The workaround the panel offers is the one that works in
//! practice: keep the wheel away from the game (`SDL_GAMECONTROLLER_IGNORE_
//! DEVICES`) and present the wheel to it as an ordinary Xbox 360 pad through
//! `/dev/uinput`.
//!
//! The bridge reads the wheel's `eventN` node directly, so no SDL, Python or
//! extra daemon is involved, and it re-reads its tuning live: the sliders in
//! the Game Bridge tab take effect on the next event.

use crate::device::DeviceInfo;
use crate::ffb;
use eframe::egui::Color32;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

// -- ioctl numbers ---------------------------------------------------------
const UI_DEV_CREATE: libc::c_ulong = 0x5501;
const UI_DEV_SETUP: libc::c_ulong = 0x405c_5503;
const UI_ABS_SETUP: libc::c_ulong = 0x401c_5504;
const UI_SET_EVBIT: libc::c_ulong = 0x4004_5564;
const UI_SET_KEYBIT: libc::c_ulong = 0x4004_5565;
const UI_SET_ABSBIT: libc::c_ulong = 0x4004_5567;

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;

const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;
const ABS_Z: u16 = 0x02;
const ABS_RX: u16 = 0x03;
const ABS_RY: u16 = 0x04;
const ABS_RZ: u16 = 0x05;
const ABS_THROTTLE: u16 = 0x06;
const ABS_HAT0X: u16 = 0x10;
const ABS_HAT0Y: u16 = 0x11;

const BTN_SOUTH: u16 = 0x130;
const BTN_EAST: u16 = 0x131;
const BTN_NORTH: u16 = 0x133;
const BTN_WEST: u16 = 0x134;
const BTN_TL: u16 = 0x136;
const BTN_TR: u16 = 0x137;
const BTN_SELECT: u16 = 0x13a;
const BTN_START: u16 = 0x13b;
const BTN_MODE: u16 = 0x13c;
const BTN_THUMBL: u16 = 0x13d;
const BTN_THUMBR: u16 = 0x13e;

/// Wheel button codes, in the order the TMX reports them (`0x120`..`0x12c`).
pub const WHEEL_BUTTONS: [u16; 13] = [
    0x120, 0x121, 0x122, 0x123, 0x124, 0x125, 0x126, 0x127, 0x128, 0x129, 0x12a, 0x12b, 0x12c,
];

/// Launch option for the connected wheel.  The line is per-device, not per
/// game: it only hides the wheel so its pedals cannot be read as a stick.
pub fn launch_option(vendor: u16, product: u16) -> String {
    format!("SDL_GAMECONTROLLER_IGNORE_DEVICES=0x{vendor:04x}/0x{product:04x} %command%")
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; 80],
    ff_effects_max: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct AbsInfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

#[repr(C)]
struct UinputAbsSetup {
    code: u16,
    absinfo: AbsInfo,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct InputEvent {
    sec: i64,
    usec: i64,
    kind: u16,
    code: u16,
    value: i32,
}

/// `EVIOCGABS(abs)` from `<linux/input.h>`.
fn eviocgabs(abs: u16) -> libc::c_ulong {
    0x8018_4540 + abs as libc::c_ulong
}

/// One pad button a wheel button can be mapped to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PadButton {
    A,
    B,
    X,
    Y,
    Lb,
    Rb,
    Back,
    Start,
    Ls,
    Rs,
    Guide,
    Lt,
    Rt,
}

impl PadButton {
    pub const ALL: [PadButton; 13] = [
        PadButton::A,
        PadButton::B,
        PadButton::X,
        PadButton::Y,
        PadButton::Lb,
        PadButton::Rb,
        PadButton::Back,
        PadButton::Start,
        PadButton::Ls,
        PadButton::Rs,
        PadButton::Guide,
        PadButton::Lt,
        PadButton::Rt,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PadButton::A => "A",
            PadButton::B => "B",
            PadButton::X => "X",
            PadButton::Y => "Y",
            PadButton::Lb => "LB",
            PadButton::Rb => "RB",
            PadButton::Back => "Back",
            PadButton::Start => "Start",
            PadButton::Ls => "LS",
            PadButton::Rs => "RS",
            PadButton::Guide => "Guide",
            PadButton::Lt => "LT",
            PadButton::Rt => "RT",
        }
    }

    fn key_code(self) -> Option<u16> {
        Some(match self {
            PadButton::A => BTN_SOUTH,
            PadButton::B => BTN_EAST,
            PadButton::X => BTN_WEST,
            PadButton::Y => BTN_NORTH,
            PadButton::Lb => BTN_TL,
            PadButton::Rb => BTN_TR,
            PadButton::Back => BTN_SELECT,
            PadButton::Start => BTN_START,
            PadButton::Ls => BTN_THUMBL,
            PadButton::Rs => BTN_THUMBR,
            PadButton::Guide => BTN_MODE,
            PadButton::Lt | PadButton::Rt => return None,
        })
    }

    fn trigger(self) -> Option<u16> {
        match self {
            PadButton::Lt => Some(ABS_Z),
            PadButton::Rt => Some(ABS_RZ),
            _ => None,
        }
    }
}

/// Tuning, shared with the running bridge thread so the sliders apply live.
#[derive(Clone)]
pub struct Settings {
    pub gain: f32,
    pub curve: f32,
    pub deadzone: f32,
    pub swap_pedals: bool,
    pub map: [PadButton; 13],
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            gain: 1.3,
            curve: 1.0,
            deadzone: 0.0,
            swap_pedals: false,
            map: [
                PadButton::A,
                PadButton::B,
                PadButton::X,
                PadButton::Y,
                PadButton::Lb,
                PadButton::Rb,
                PadButton::Back,
                PadButton::Start,
                PadButton::Ls,
                PadButton::Rs,
                PadButton::Guide,
                PadButton::A,
                PadButton::B,
            ],
        }
    }
}

impl Settings {
    fn to_text(&self) -> String {
        let map: Vec<String> = self.map.iter().map(|b| b.name().to_string()).collect();
        format!(
            "gain={:.3}\ncurve={:.3}\ndeadzone={:.3}\nswap_pedals={}\nmap={}\n",
            self.gain,
            self.curve,
            self.deadzone,
            if self.swap_pedals { 1 } else { 0 },
            map.join(",")
        )
    }

    fn from_text(text: &str) -> Self {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "gain" => s.gain = value.parse().unwrap_or(s.gain),
                "curve" => s.curve = value.parse().unwrap_or(s.curve),
                "deadzone" => s.deadzone = value.parse().unwrap_or(s.deadzone),
                "swap_pedals" => s.swap_pedals = value == "1",
                "map" => {
                    for (slot, name) in s.map.iter_mut().zip(value.split(',')) {
                        if let Some(found) = PadButton::ALL
                            .iter()
                            .find(|b| b.name().eq_ignore_ascii_case(name.trim()))
                        {
                            *slot = *found;
                        }
                    }
                }
                _ => {}
            }
        }
        s
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| String::from("/tmp")))
                .join(".config")
        });
    base.join("ttrs-wheel").join("bridge.conf")
}

// -- profiles -------------------------------------------------------------

/// One game's tuning.  The bridge always runs with the active profile.
#[derive(Clone)]
pub struct Profile {
    pub name: String,
    pub appid: Option<u32>,
    /// Virtual-pad tuning.
    pub settings: Settings,
    /// Wheel-side settings, written when the profile becomes active.
    pub rotation: f32,
    pub overall: f32,
    pub autocenter_game: bool,
    pub autocenter_strength: f32,
    /// The game only sees a pad under Wine, so the bridge must run with it.
    pub needs_bridge: bool,
}

impl Profile {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            appid: None,
            settings: Settings::default(),
            rotation: crate::mechanics::opening_angle_deg(true) as f32,
            overall: crate::mechanics::default_overall_pct(
                crate::mechanics::WheelClass::Limited900,
            ) as f32,
            autocenter_game: true,
            autocenter_strength: crate::mechanics::DEFAULT_AUTOCENTER_STRENGTH_PCT as f32,
            // Old profiles have no `bridge=` key: derive the default from the
            // name so NFS Heat keeps its bridge after the format grew a field.
            needs_bridge: crate::steam::suggest(name).bridge,
        }
    }
}

#[derive(Clone)]
pub struct Profiles {
    pub list: Vec<Profile>,
    pub active: usize,
    /// Switch to a profile automatically when its Steam app starts.
    pub follow: bool,
}

impl Default for Profiles {
    fn default() -> Self {
        Self {
            list: vec![Profile::new("Default")],
            active: 0,
            follow: true,
        }
    }
}

impl Profile {
    /// Build a profile from a reference entry: the bridge tuning is the only
    /// part the table does not carry, so it follows the "needs bridge" flag.
    pub fn from_reference(reference: &crate::steam::Reference) -> Self {
        let mut profile = Profile::new(reference.name);
        profile.appid = reference.appid;
        profile.rotation = reference.rotation;
        profile.overall = reference.overall;
        profile.needs_bridge = reference.bridge;
        profile.settings.gain = if reference.bridge { 1.3 } else { 1.0 };
        profile
    }
}

impl Profiles {
    pub fn current(&self) -> &Profile {
        &self.list[self.active.min(self.list.len() - 1)]
    }

    pub fn current_mut(&mut self) -> &mut Profile {
        let index = self.active.min(self.list.len() - 1);
        &mut self.list[index]
    }

    pub fn add(&mut self, name: &str) {
        self.list.push(Profile::new(name));
        self.active = self.list.len() - 1;
    }

    pub fn duplicate(&mut self) {
        let mut copy = self.current().clone();
        copy.name = format!("{} copy", copy.name);
        copy.appid = None;
        self.list.push(copy);
        self.active = self.list.len() - 1;
    }

    pub fn remove(&mut self) {
        if self.list.len() <= 1 {
            self.list.truncate(1);
            self.list[0] = Profile::new("Default");
            self.active = 0;
            return;
        }
        self.list.remove(self.active);
        self.active = self.active.min(self.list.len() - 1);
    }

    pub fn index_of_appid(&self, appid: u32) -> Option<usize> {
        self.list.iter().position(|p| p.appid == Some(appid))
    }

    pub fn to_text(&self) -> String {
        let mut out = format!("follow={}\nactive={}\n", self.follow as u8, self.active);
        for profile in &self.list {
            out.push_str(&format!("\n[{}]\n", profile.name));
            match profile.appid {
                Some(id) => out.push_str(&format!("appid={id}\n")),
                None => out.push_str("appid=\n"),
            }
            out.push_str(&format!(
                "bridge={}\nrotation={:.1}\noverall={:.1}\nautocenter_game={}\nautocenter_strength={:.1}\n",
                profile.needs_bridge as u8,
                profile.rotation,
                profile.overall,
                profile.autocenter_game as u8,
                profile.autocenter_strength
            ));
            out.push_str(&profile.settings.to_text());
        }
        out
    }

    pub fn from_text(text: &str) -> Self {
        let mut out = Profiles {
            list: Vec::new(),
            active: 0,
            follow: true,
        };
        let mut current: Option<Profile> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                if let Some(done) = current.take() {
                    out.list.push(done);
                }
                current = Some(Profile::new(name));
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match &mut current {
                None => match key {
                    "follow" => out.follow = value == "1",
                    "active" => out.active = value.parse().unwrap_or(0),
                    _ => {}
                },
                Some(profile) => match key {
                    "appid" => profile.appid = value.parse().ok(),
                    "bridge" => profile.needs_bridge = value == "1",
                    "rotation" => profile.rotation = value.parse().unwrap_or(profile.rotation),
                    "overall" => profile.overall = value.parse().unwrap_or(profile.overall),
                    "autocenter_game" => profile.autocenter_game = value == "1",
                    "autocenter_strength" => {
                        profile.autocenter_strength =
                            value.parse().unwrap_or(profile.autocenter_strength)
                    }
                    "gain" => profile.settings.gain = value.parse().unwrap_or(1.3),
                    "curve" => profile.settings.curve = value.parse().unwrap_or(1.0),
                    "deadzone" => profile.settings.deadzone = value.parse().unwrap_or(0.0),
                    "swap_pedals" => profile.settings.swap_pedals = value == "1",
                    "map" => {
                        for (slot, name) in profile.settings.map.iter_mut().zip(value.split(',')) {
                            if let Some(found) = PadButton::ALL
                                .iter()
                                .find(|b| b.name().eq_ignore_ascii_case(name.trim()))
                            {
                                *slot = *found;
                            }
                        }
                    }
                    _ => {}
                },
            }
        }
        if let Some(done) = current {
            out.list.push(done);
        }
        if out.list.is_empty() {
            out.list.push(Profile::new("Default"));
        }
        out.active = out.active.min(out.list.len() - 1);
        out
    }
}

fn profiles_path() -> PathBuf {
    config_path().with_file_name("profiles.conf")
}

pub fn load_profiles() -> Profiles {
    fs::read_to_string(profiles_path())
        .map(|t| Profiles::from_text(&t))
        .unwrap_or_default()
}

pub fn save_profiles(profiles: &Profiles) {
    let path = profiles_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(mut file) = File::create(path) {
        let _ = file.write_all(profiles.to_text().as_bytes());
    }
}

/// Which Steam app is running, read from Steam's own compat log.  Steam keeps
/// appending `StartSession` / `release session(s)` lines, so the last unmatched
/// start is the live game.
pub fn running_appid() -> Option<u32> {
    let base = std::env::var("HOME").ok()?;
    let path = PathBuf::from(base).join(".local/share/Steam/logs/compat_log.txt");
    let text = fs::read_to_string(path).ok()?;
    let mut running: Option<u32> = None;
    for line in text.lines() {
        if let Some(rest) = line.split("StartSession: appID ").nth(1) {
            if let Some(id) = rest.split_whitespace().next().and_then(|v| v.parse::<u32>().ok()) {
                running = Some(id);
            }
        } else if let Some(rest) = line.split("release session(s) for appID ").nth(1) {
            if let Some(id) = rest.split_whitespace().next().and_then(|v| v.parse::<u32>().ok()) {
                if running == Some(id) {
                    running = None;
                }
            }
        }
    }
    running
}

pub fn load_settings() -> Settings {
    fs::read_to_string(config_path())
        .map(|t| Settings::from_text(&t))
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) {
    let path = config_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    if let Ok(mut file) = File::create(path) {
        let _ = file.write_all(settings.to_text().as_bytes());
    }
}

/// What the UI shows and what the thread publishes.
#[derive(Clone, Default)]
pub struct Live {
    pub running: bool,
    pub message: String,
    pub steer: f32,
    pub gas: f32,
    pub brake: f32,
    pub pressed: u32,
}

impl Live {
    pub fn dot(&self) -> Color32 {
        if self.running {
            crate::theme::pal().ok_green
        } else if self.message.is_empty() {
            crate::theme::pal().muted
        } else {
            crate::theme::pal().warn
        }
    }
}

pub struct Bridge {
    pub settings: Settings,
    pub live: Live,
    shared_settings: Arc<Mutex<Settings>>,
    shared_live: Arc<Mutex<Live>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// Node the running thread is attached to.
    node: Option<PathBuf>,
}

impl Default for Bridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Bridge {
    pub fn new() -> Self {
        Self {
            settings: load_settings(),
            live: Live::default(),
            shared_settings: Arc::new(Mutex::new(Settings::default())),
            shared_live: Arc::new(Mutex::new(Live::default())),
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
            node: None,
        }
    }

    /// Push the UI's tuning into the running thread.
    pub fn push_settings(&mut self) {
        if let Ok(mut shared) = self.shared_settings.lock() {
            *shared = self.settings.clone();
        }
        save_settings(&self.settings);
    }

    pub fn start(&mut self, info: &DeviceInfo) {
        self.stop();
        let Some(node) = ffb::event_node_for(info) else {
            self.live.message = String::from("No event node for the wheel.");
            return;
        };
        if let Ok(mut shared) = self.shared_settings.lock() {
            *shared = self.settings.clone();
        }
        self.stop.store(false, Ordering::SeqCst);
        if let Ok(mut live) = self.shared_live.lock() {
            *live = Live {
                message: String::from("Starting..."),
                ..Live::default()
            };
        }
        let settings = Arc::clone(&self.shared_settings);
        let live = Arc::clone(&self.shared_live);
        let stop = Arc::clone(&self.stop);
        let path = node.clone();
        self.node = Some(node);
        self.thread = Some(std::thread::spawn(move || {
            run(path, settings, live, stop);
        }));
    }

    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        self.node = None;
        if let Ok(mut live) = self.shared_live.lock() {
            live.running = false;
            live.steer = 0.0;
            live.gas = 0.0;
            live.brake = 0.0;
            live.pressed = 0;
        }
    }

    /// Copy the thread's state into `self.live` once per frame.
    pub fn poll(&mut self) {
        if let Ok(live) = self.shared_live.lock() {
            self.live = live.clone();
        }
        // A dropped wheel should not leave a stale "running" flag behind.
        if self.live.running && self.thread.is_none() {
            self.live.running = false;
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop();
    }
}

// -- uinput ---------------------------------------------------------------

struct Pad {
    file: File,
}

impl Pad {
    fn create() -> std::io::Result<Self> {
        let file = OpenOptions::new().write(true).open("/dev/uinput")?;
        let fd = file.as_raw_fd();
        unsafe {
            for ev in [EV_SYN, EV_KEY, EV_ABS] {
                libc::ioctl(fd, UI_SET_EVBIT, ev as libc::c_ulong);
            }
            for code in [
                BTN_SOUTH, BTN_EAST, BTN_NORTH, BTN_WEST, BTN_TL, BTN_TR, BTN_SELECT, BTN_START,
                BTN_MODE, BTN_THUMBL, BTN_THUMBR,
            ] {
                libc::ioctl(fd, UI_SET_KEYBIT, code as libc::c_ulong);
            }
            let axes: [(u16, i32, i32, i32, i32); 8] = [
                (ABS_X, -32768, 32767, 16, 128),
                (ABS_Y, -32768, 32767, 16, 128),
                (ABS_RX, -32768, 32767, 16, 128),
                (ABS_RY, -32768, 32767, 16, 128),
                (ABS_Z, 0, 255, 0, 0),
                (ABS_RZ, 0, 255, 0, 0),
                (ABS_HAT0X, -1, 1, 0, 0),
                (ABS_HAT0Y, -1, 1, 0, 0),
            ];
            for (code, min, max, fuzz, flat) in axes {
                libc::ioctl(fd, UI_SET_ABSBIT, code as libc::c_ulong);
                let setup = UinputAbsSetup {
                    code,
                    absinfo: AbsInfo {
                        value: 0,
                        minimum: min,
                        maximum: max,
                        fuzz,
                        flat,
                        resolution: 0,
                    },
                };
                libc::ioctl(fd, UI_ABS_SETUP, &setup as *const UinputAbsSetup);
            }
            let mut name = [0u8; 80];
            let label = b"Microsoft X-Box 360 pad";
            name[..label.len()].copy_from_slice(label);
            let setup = UinputSetup {
                id: InputId {
                    bustype: 0x03, // BUS_USB
                    vendor: 0x045e,
                    product: 0x028e,
                    version: 0x0114,
                },
                name,
                ff_effects_max: 0,
            };
            if libc::ioctl(fd, UI_DEV_SETUP, &setup as *const UinputSetup) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::ioctl(fd, UI_DEV_CREATE) < 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(Self { file })
    }

    fn write(&mut self, kind: u16, code: u16, value: i32) {
        let ev = InputEvent {
            sec: 0,
            usec: 0,
            kind,
            code,
            value,
        };
        let bytes = unsafe {
            std::slice::from_raw_parts(&ev as *const InputEvent as *const u8, std::mem::size_of::<InputEvent>())
        };
        let _ = self.file.write_all(bytes);
    }

    fn syn(&mut self) {
        self.write(EV_SYN, 0, 0);
    }
}

impl Drop for Pad {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.file.as_raw_fd(), 0x5502); // UI_DEV_DESTROY
        }
    }
}

// -- scaling --------------------------------------------------------------

fn steer_out(value: i32, info: &AbsInfo, settings: &Settings) -> i32 {
    let span = (info.maximum - info.minimum).max(1) as f32;
    let mut x = (value - info.minimum) as f32 / span * 2.0 - 1.0;
    let dz = settings.deadzone.clamp(0.0, 0.49);
    if dz > 0.0 {
        if x.abs() <= dz {
            x = 0.0;
        } else {
            x = x.signum() * (x.abs() - dz) / (1.0 - dz);
        }
    }
    if x != 0.0 {
        x = x.signum() * x.abs().powf(settings.curve.clamp(0.2, 2.0)) * settings.gain.max(0.1);
    }
    (x.clamp(-1.0, 1.0) * 32767.0).round() as i32
}

fn pedal_out(value: i32, info: &AbsInfo, inverted: bool) -> i32 {
    let span = (info.maximum - info.minimum).max(1) as f32;
    let mut frac = (value - info.minimum) as f32 / span;
    if inverted {
        frac = 1.0 - frac;
    }
    (frac.clamp(0.0, 1.0) * 255.0).round() as i32
}

fn abs_of(fd: i32, code: u16) -> Option<AbsInfo> {
    let mut info = AbsInfo::default();
    let rc = unsafe { libc::ioctl(fd, eviocgabs(code), &mut info as *mut AbsInfo) };
    if rc < 0 {
        None
    } else {
        Some(info)
    }
}

/// The thread body: open the wheel, create the pad, pump events.
fn run(path: PathBuf, settings: Arc<Mutex<Settings>>, live: Arc<Mutex<Live>>, stop: Arc<AtomicBool>) {
    let set_msg = |text: String| {
        if let Ok(mut l) = live.lock() {
            l.running = false;
            l.message = text;
        }
    };

    let wheel = match File::open(&path) {
        Ok(f) => f,
        Err(e) => return set_msg(format!("Cannot open {}: {e}", path.display())),
    };
    let fd = wheel.as_raw_fd();

    let steer_info = match abs_of(fd, ABS_X) {
        Some(i) => i,
        None => return set_msg(String::from("Wheel has no ABS_X axis.")),
    };
    let rest_of = |code: u16| abs_of(fd, code).map(|i| (code, i));
    let mut pedals: Vec<(u16, AbsInfo, bool)> = Vec::new();
    for code in [ABS_RZ, ABS_Y, ABS_THROTTLE] {
        if let Some((code, info)) = rest_of(code) {
            let inverted = info.value > (info.minimum + info.maximum) / 2;
            pedals.push((code, info, inverted));
            if pedals.len() == 2 {
                break;
            }
        }
    }
    let hat_x = abs_of(fd, ABS_HAT0X);
    let hat_y = abs_of(fd, ABS_HAT0Y);

    let mut pad = match Pad::create() {
        Ok(p) => p,
        Err(e) => return set_msg(format!("Cannot create uinput pad: {e}")),
    };

    if let Ok(mut l) = live.lock() {
        l.running = true;
        l.message = String::from("Virtual pad active.");
    }

    let mut trig = [0i32; 2]; // [0] = LT (ABS_Z), [1] = RT (ABS_RZ)
    let mut axis_trigger = [0i32; 2];
    let mut pressed: u32 = 0;

    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let mut pfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let rc = unsafe { libc::poll(&mut pfd, 1, 150) };
        if rc <= 0 {
            continue;
        }
        let mut buf = [0u8; std::mem::size_of::<InputEvent>() * 64];
        let n = unsafe {
            libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
        };
        if n <= 0 {
            continue;
        }
        let size = std::mem::size_of::<InputEvent>();
        let count = n as usize / size;
        let current = match settings.lock() {
            Ok(s) => s.clone(),
            Err(_) => break,
        };

        for i in 0..count {
            let ev = unsafe {
                std::ptr::read_unaligned(buf.as_ptr().add(i * size) as *const InputEvent)
            };
            match ev.kind {
                EV_ABS => {
                    if ev.code == ABS_X {
                        pad.write(EV_ABS, ABS_X, steer_out(ev.value, &steer_info, &current));
                        if let Ok(mut l) = live.lock() {
                            l.steer = (ev.value - steer_info.minimum) as f32
                                / (steer_info.maximum - steer_info.minimum).max(1) as f32
                                * 2.0
                                - 1.0;
                        }
                    } else if ev.code == ABS_HAT0X && hat_x.is_some() {
                        pad.write(EV_ABS, ABS_HAT0X, ev.value);
                    } else if ev.code == ABS_HAT0Y && hat_y.is_some() {
                        pad.write(EV_ABS, ABS_HAT0Y, ev.value);
                    } else if let Some((slot, (_, info, inverted))) = pedals
                        .iter()
                        .enumerate()
                        .find(|(_, (code, _, _))| *code == ev.code)
                    {
                        let (target_slot, target) = if current.swap_pedals {
                            (1 - slot, if slot == 0 { ABS_RZ } else { ABS_Z })
                        } else {
                            (slot, if slot == 0 { ABS_RZ } else { ABS_Z })
                        };
                        let value = pedal_out(ev.value, info, *inverted);
                        axis_trigger[target_slot] = value;
                        trig[target_slot] = axis_trigger[target_slot].max(trig[target_slot]);
                        pad.write(EV_ABS, target, value);
                        if let Ok(mut l) = live.lock() {
                            if target_slot == 0 {
                                l.brake = value as f32 / 255.0;
                            } else {
                                l.gas = value as f32 / 255.0;
                            }
                        }
                    }
                    pad.syn();
                }
                EV_KEY => {
                    let slot = WHEEL_BUTTONS.iter().position(|c| *c == ev.code);
                    let Some(index) = slot else { continue };
                    let target = current.map[index];
                    if let Some(code) = target.key_code() {
                        pad.write(EV_KEY, code, if ev.value != 0 { 1 } else { 0 });
                        let bit = 1u32 << index;
                        if ev.value != 0 {
                            pressed |= bit;
                        } else {
                            pressed &= !bit;
                        }
                    } else if let Some(trigger) = target.trigger() {
                        let s = if trigger == ABS_Z { 0 } else { 1 };
                        if ev.value != 0 {
                            trig[s] = 255;
                        } else {
                            trig[s] = axis_trigger[s];
                        }
                        pad.write(EV_ABS, trigger, trig[s]);
                    }
                    pad.syn();
                }
                _ => {}
            }
        }
        if let Ok(mut l) = live.lock() {
            l.pressed = pressed;
        }
    }

    if let Ok(mut l) = live.lock() {
        l.running = false;
        l.message = String::from("Stopped.");
        l.steer = 0.0;
        l.gas = 0.0;
        l.brake = 0.0;
        l.pressed = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert!(s.gain >= 1.0);
        assert_eq!(s.map.len(), WHEEL_BUTTONS.len());
    }

    #[test]
    fn settings_round_trip() {
        let mut s = Settings::default();
        s.gain = 1.15;
        s.curve = 0.8;
        s.deadzone = 0.05;
        s.swap_pedals = true;
        s.map[0] = PadButton::Rt;
        let again = Settings::from_text(&s.to_text());
        assert!((again.gain - 1.15).abs() < 1e-6);
        assert!((again.curve - 0.8).abs() < 1e-6);
        assert!((again.deadzone - 0.05).abs() < 1e-6);
        assert!(again.swap_pedals);
        assert_eq!(again.map[0], PadButton::Rt);
    }

    #[test]
    fn steering_centre_stays_centre() {
        let info = AbsInfo {
            value: 32768,
            minimum: 0,
            maximum: 65535,
            ..AbsInfo::default()
        };
        let out = steer_out(32767, &info, &Settings::default());
        assert!(out.abs() <= 1);
    }

    #[test]
    fn gain_reaches_full_scale_early() {
        let info = AbsInfo {
            value: 32768,
            minimum: 0,
            maximum: 65535,
            ..AbsInfo::default()
        };
        let s = Settings {
            gain: 1.3,
            ..Settings::default()
        };
        // 77% of the way to the stop is already full lock: |x| * 1.3 >= 1.
        let out = steer_out((65535.0 * 0.90) as i32, &info, &s);
        assert!(out >= 32760, "got {out}");
        let short = steer_out((65535.0 * 0.5) as i32, &info, &s);
        assert!(short.abs() < 1000, "centre must stay calm, got {short}");
    }

    #[test]
    fn profiles_round_trip() {
        let mut profiles = Profiles::default();
        profiles.add("Need for Speed Heat");
        profiles.current_mut().appid = Some(1222680);
        profiles.current_mut().settings.gain = 1.15;
        profiles.current_mut().settings.map[4] = PadButton::Rt;
        profiles.follow = false;

        let again = Profiles::from_text(&profiles.to_text());
        assert_eq!(again.list.len(), 2);
        assert_eq!(again.current().name, "Need for Speed Heat");
        assert_eq!(again.current().appid, Some(1222680));
        assert!((again.current().settings.gain - 1.15).abs() < 1e-6);
        assert_eq!(again.current().settings.map[4], PadButton::Rt);
        assert!(!again.follow);
        assert_eq!(again.index_of_appid(1222680), Some(1));
    }

    #[test]
    fn last_profile_cannot_be_deleted() {
        let mut profiles = Profiles::default();
        profiles.remove();
        assert_eq!(profiles.list.len(), 1);
        assert_eq!(profiles.current().name, "Default");
    }

    #[test]
    fn pedal_inverts_when_resting_at_maximum() {
        let info = AbsInfo {
            minimum: 0,
            maximum: 1023,
            ..AbsInfo::default()
        };
        assert_eq!(pedal_out(1023, &info, true), 0);
        assert_eq!(pedal_out(0, &info, true), 255);
    }
}
