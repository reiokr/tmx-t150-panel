//! Real Linux wheel support.
//!
//! The panel talks to whatever racing wheel the kernel exposes as a joystick
//! (`/dev/input/js*`).  That works out of the box with the in-kernel `xpad`
//! driver (Xbox mode) and with the out-of-tree `hid-t150` driver (PC mode).
//!
//! Settings that the wheel firmware owns (rotation range, FFB gain,
//! auto-centre) can only be written through the `hid-t150` sysfs attributes.
//! When those are absent the panel stays read-only instead of pretending.

use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const VENDOR_THRUSTMASTER: u16 = 0x044f;

const JSIOCGAXES: libc::c_ulong = 0x8001_6a11;
const JSIOCGBUTTONS: libc::c_ulong = 0x8001_6a12;
const JSIOCGAXMAP: libc::c_ulong = 0x8040_6a32;

const ABS_X: u8 = 0x00;
const ABS_Y: u8 = 0x01;
const ABS_Z: u8 = 0x02;
const ABS_RZ: u8 = 0x05;
const ABS_THROTTLE: u8 = 0x06;
const ABS_GAS: u8 = 0x09;
const ABS_BRAKE: u8 = 0x0a;
const ABS_HAT0X: u8 = 0x10;
const ABS_HAT0Y: u8 = 0x11;

/// The original's 900° flag `0x1800657bc`: set by the device detection for
/// 900-degree units.  `sub_1800133b0` then caps angle input at 0x384 (900).
pub fn is_900_degree(product: u16) -> bool {
    matches!(product, 0xb677 | 0xb67e | 0xb67f)
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub node: PathBuf,
    pub name: String,
    pub vendor: u16,
    pub product: u16,
    pub driver: String,
}

impl DeviceInfo {
    /// Marketing name for the USB product id, when we know it.
    pub fn product_name(&self) -> String {
        let known = match self.product {
            0xb677 => Some("Thrustmaster T150"),
            0xb67e => Some("Thrustmaster TMX (GIP)"),
            0xb67f => Some("Thrustmaster TMX"),
            0xb65d => Some("Thrustmaster T300"),
            0xb66e => Some("Thrustmaster T300 RS"),
            0xb66f => Some("Thrustmaster TX"),
            _ => None,
        };
        match known {
            Some(n) => format!("{} [{:04x}:{:04x}]", n, self.vendor, self.product),
            None if self.vendor == VENDOR_THRUSTMASTER => {
                format!(
                    "Thrustmaster wheel [{:04x}:{:04x}]",
                    self.vendor, self.product
                )
            }
            None => format!("{} [{:04x}:{:04x}]", self.name, self.vendor, self.product),
        }
    }

    /// The name the kernel exposes for the device.  The in-kernel `xpad`
    /// driver labels many third-party wheels "Generic X-Box pad", so this is
    /// not necessarily the marketing name.
    pub fn kernel_name(&self) -> &str {
        &self.name
    }

    /// True when the kernel name is xpad's generic label instead of a real
    /// product name.  KDE Plasma and Steam display exactly this label, which
    /// makes the wheel look like an anonymous gamepad.
    pub fn has_generic_kernel_name(&self) -> bool {
        let n = self.name.to_ascii_lowercase();
        (n.contains("x-box") || n.contains("xbox")) && n.contains("pad")
    }

    /// Which kernel driver currently owns the wheel.  This decides whether the
    /// control panel can write settings at all.
    pub fn mode_label(&self) -> &'static str {
        match self.driver.as_str() {
            "hid-t150" => "PC mode (hid-t150)",
            "xpad" => "Xbox mode (xpad)",
            _ => "generic input",
        }
    }

    pub fn is_thrustmaster(&self) -> bool {
        self.vendor == VENDOR_THRUSTMASTER
            || self.name.to_ascii_lowercase().contains("thrustmaster")
            || self.name.to_ascii_lowercase().contains("t150")
            || self.name.to_ascii_lowercase().contains("tmx")
    }
}

#[derive(Clone, Default)]
pub struct Snapshot {
    pub connected: bool,
    pub steer: f32,
    pub pedals: [f32; 3],
    pub buttons: [bool; 12],
    pub pov: usize,
    pub events: u64,
}

struct Shared {
    axes: [i16; 64],
    rest: [i16; 64],
    rest_known: [bool; 64],
    buttons: [bool; 32],
    axmap: [u8; 64],
    naxes: usize,
    events: u64,
    alive: bool,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            axes: [0; 64],
            rest: [0; 64],
            rest_known: [false; 64],
            buttons: [false; 32],
            axmap: [0; 64],
            naxes: 0,
            events: 0,
            alive: false,
        }
    }
}

/// Reads the joystick API.  Eight bytes per event:
/// `u32 time, i16 value, u8 type, u8 number`.
fn reader_loop(mut file: File, shared: Arc<Mutex<Shared>>, alive: Arc<AtomicBool>) {
    let mut buf = [0u8; 8];
    loop {
        if file.read_exact(&mut buf).is_err() {
            alive.store(false, Ordering::Relaxed);
            if let Ok(mut s) = shared.lock() {
                s.alive = false;
            }
            return;
        }
        let value = i16::from_le_bytes([buf[4], buf[5]]);
        let kind = buf[6];
        let number = buf[7] as usize;
        let init = kind & 0x80 != 0;
        let kind = kind & 0x7f;

        if let Ok(mut s) = shared.lock() {
            match kind {
                0x01 if number < s.buttons.len() => {
                    s.buttons[number] = value != 0;
                }
                0x02 if number < s.axes.len() => {
                    s.axes[number] = value;
                    if init || !s.rest_known[number] {
                        s.rest[number] = value;
                        s.rest_known[number] = true;
                    }
                }
                _ => {}
            }
            s.events = s.events.wrapping_add(1);
        }
    }
}

fn read_trim(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn parse_hex(s: &str) -> Option<u16> {
    u16::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok()
}

fn driver_of(input_dir: &Path) -> String {
    // A joystick input node sits under a HID/USB interface.  The driver that
    // actually owns the wheel is therefore one or two levels further down the
    // sysfs chain (input10 -> 10-9:1.0 -> xpad), not directly under the node.
    let candidates = [
        input_dir.join("device/device/driver"),
        input_dir.join("device/driver"),
        input_dir.join("driver"),
    ];
    for candidate in &candidates {
        if let Ok(link) = fs::read_link(candidate) {
            if let Some(name) = link.file_name().and_then(|n| n.to_str()) {
                return name.to_string();
            }
        }
    }
    for candidate in &candidates {
        if let Some(name) = read_trim(candidate.join("module")) {
            return name;
        }
    }
    String::from("unknown")
}

fn discover_one(js_name: &str) -> Option<DeviceInfo> {
    let sys = PathBuf::from("/sys/class/input").join(js_name);
    let input_dir = fs::canonicalize(sys.join("device")).ok()?;
    let vendor = read_trim(input_dir.join("id/vendor")).and_then(|v| parse_hex(&v))?;
    let product = read_trim(input_dir.join("id/product")).and_then(|v| parse_hex(&v))?;
    let name = read_trim(input_dir.join("name")).unwrap_or_else(|| String::from("joystick"));
    Some(DeviceInfo {
        node: PathBuf::from("/dev/input").join(js_name),
        name,
        vendor,
        product,
        driver: driver_of(&input_dir),
    })
}

fn is_wheel_like(info: &DeviceInfo) -> bool {
    let n = info.name.to_ascii_lowercase();
    info.is_thrustmaster()
        || info.vendor == 0x046d
        || n.contains("wheel")
        || n.contains("racing")
        || n.contains("steering")
}

/// Pick the wheel, preferring a Thrustmaster device over any other joystick.
pub fn discover() -> Option<DeviceInfo> {
    let mut fallback: Option<DeviceInfo> = None;
    let entries = fs::read_dir("/sys/class/input").ok()?;
    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.starts_with("js").then_some(n)
        })
        .collect();
    names.sort();
    for name in names {
        if let Some(info) = discover_one(&name) {
            if info.is_thrustmaster() {
                return Some(info);
            }
            if fallback.is_none() && is_wheel_like(&info) {
                fallback = Some(info);
            }
        }
    }
    fallback
}

fn ioctl_u8(fd: libc::c_int, req: libc::c_ulong) -> u8 {
    let mut value: u8 = 0;
    let rc = unsafe { libc::ioctl(fd, req, &mut value as *mut u8) };
    if rc < 0 {
        0
    } else {
        value
    }
}

/// Which axes carry the pedals.
///
/// Thrustmaster's two-pedal sets report the left pedal on `ABS_Y` and the
/// right one on `ABS_RZ`; a third pedal (T3PA) shows up as `ABS_THROTTLE`.
/// The old order treated `ABS_Y` as the clutch, so the left pedal lit the
/// clutch bar and the brake stayed at zero.
///
/// `parked[i]` is true when axis `i` rests at an extreme, which is how a pedal
/// behaves; a centred axis is a stick and must not be taken for a clutch.
fn pedal_axes(axmap: &[u8], parked: &[bool]) -> (Option<usize>, Option<usize>, Option<usize>) {
    let find = |code: u8| axmap.iter().position(|c| *c == code);
    let pedal_like = |i: usize| parked.get(i).copied().unwrap_or(false);

    let gas = find(ABS_GAS).or_else(|| find(ABS_RZ)).or_else(|| find(ABS_Y));
    let brake = find(ABS_BRAKE)
        .or_else(|| find(ABS_Z))
        .or_else(|| find(ABS_Y).filter(|i| Some(*i) != gas && pedal_like(*i)));
    let clutch = find(ABS_THROTTLE)
        .filter(|i| Some(*i) != gas && Some(*i) != brake && pedal_like(*i))
        .or_else(|| {
            find(ABS_Y).filter(|i| Some(*i) != gas && Some(*i) != brake && pedal_like(*i))
        });
    (clutch, brake, gas)
}

pub struct DeviceMonitor {
    shared: Arc<Mutex<Shared>>,
    alive: Arc<AtomicBool>,
    pub info: Option<DeviceInfo>,
    pub snapshot: Snapshot,
    last_scan: Instant,
    pub last_error: Option<String>,
}

impl Default for DeviceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl DeviceMonitor {
    pub fn new() -> Self {
        let mut me = Self {
            shared: Arc::new(Mutex::new(Shared::default())),
            alive: Arc::new(AtomicBool::new(false)),
            info: None,
            snapshot: Snapshot::default(),
            last_scan: Instant::now() - Duration::from_secs(10),
            last_error: None,
        };
        me.scan();
        me.refresh();
        me
    }

    /// Force a rescan, e.g. after the user reconnects the wheel.
    pub fn rescan(&mut self) {
        self.info = None;
        self.snapshot.connected = false;
        self.scan();
        self.refresh();
    }

    fn scan(&mut self) {
        self.last_scan = Instant::now();
        let Some(info) = discover() else {
            self.info = None;
            self.last_error = Some(String::from("No wheel found under /dev/input/js*."));
            return;
        };

        let file = match OpenOptions::new().read(true).open(&info.node) {
            Ok(f) => f,
            Err(e) => {
                self.last_error = Some(format!(
                    "Found {} but cannot read it: {e}. Add yourself to the input group.",
                    info.node.display()
                ));
                self.info = None;
                return;
            }
        };

        let fd = file.as_raw_fd();
        let naxes = ioctl_u8(fd, JSIOCGAXES) as usize;
        let _nbuttons = ioctl_u8(fd, JSIOCGBUTTONS) as usize;
        let mut axmap = [0u8; 64];
        unsafe {
            libc::ioctl(fd, JSIOCGAXMAP, axmap.as_mut_ptr());
        }

        let mut shared = Shared {
            axmap,
            naxes: naxes.min(64),
            ..Default::default()
        };
        shared.alive = true;

        let shared = Arc::new(Mutex::new(shared));
        let alive = Arc::new(AtomicBool::new(true));
        {
            let shared = Arc::clone(&shared);
            let alive = Arc::clone(&alive);
            thread::spawn(move || reader_loop(file, shared, alive));
        }

        self.shared = shared;
        self.alive = alive;
        self.info = Some(info);
        self.last_error = None;
        // Give the kernel a moment to push the init events so the resting
        // positions are known before the first frame is drawn.
        thread::sleep(Duration::from_millis(12));
    }

    fn refresh(&mut self) {
        if self.info.is_none() {
            self.snapshot = Snapshot::default();
            return;
        }
        let Ok(shared) = self.shared.lock() else {
            return;
        };
        if !self.alive.load(Ordering::Relaxed) {
            self.snapshot.connected = false;
            return;
        }

        let axmap = shared.axmap;
        let axes = shared.axes;
        let rest = shared.rest;
        let known = shared.rest_known;

        let find = |code: u8| -> Option<usize> { (0..shared.naxes).find(|i| axmap[*i] == code) };

        let steer_axis = find(ABS_X);
        let parked: Vec<bool> = (0..shared.naxes)
            .map(|i| known.get(i).copied().unwrap_or(false) && rest[i].abs() > 16000)
            .collect();
        let (clutch_axis, brake_axis, gas_axis) = pedal_axes(&axmap[..shared.naxes], &parked);

        let centred = |i: usize| -> f32 {
            let r = if known.get(i).copied().unwrap_or(false) {
                rest[i]
            } else {
                0
            };
            let span = (32767 - r.abs()).max(1) as f32;
            ((axes[i] as f32 - r as f32) / span).clamp(-1.0, 1.0)
        };

        let pedal = |i: Option<usize>| -> f32 {
            let Some(i) = i else { return 0.0 };
            if !known.get(i).copied().unwrap_or(false) {
                return 0.0;
            }
            let r = rest[i];
            let v = axes[i] as f32;
            if r < -16000 {
                ((v - r as f32) / (32767.0 - r as f32)).clamp(0.0, 1.0)
            } else if r > 16000 {
                ((r as f32 - v) / (r as f32 + 32767.0)).clamp(0.0, 1.0)
            } else {
                ((v + 32767.0) / 65534.0).clamp(0.0, 1.0)
            }
        };

        let hat_x = find(ABS_HAT0X).map(|i| axes[i]).unwrap_or(0);
        let hat_y = find(ABS_HAT0Y).map(|i| axes[i]).unwrap_or(0);
        let pov = match (hat_x.signum(), hat_y.signum()) {
            (0, -1) => 0,
            (1, -1) => 1,
            (1, 0) => 2,
            (1, 1) => 3,
            (0, 1) => 4,
            (-1, 1) => 5,
            (-1, 0) => 6,
            (-1, -1) => 7,
            _ => 8,
        };

        let mut buttons = [false; 12];
        for (i, b) in buttons.iter_mut().enumerate() {
            *b = shared.buttons.get(i).copied().unwrap_or(false);
        }

        self.snapshot = Snapshot {
            connected: true,
            steer: steer_axis.map(centred).unwrap_or(0.0),
            pedals: [pedal(clutch_axis), pedal(brake_axis), pedal(gas_axis)],
            buttons,
            pov,
            events: shared.events,
        };
    }

    /// Called every frame.
    pub fn poll(&mut self, _dt: f32) -> Snapshot {
        let lost = self
            .info
            .as_ref()
            .map(|_| !self.alive.load(Ordering::Relaxed))
            .unwrap_or(true);
        if lost && self.last_scan.elapsed() > Duration::from_millis(1500) {
            self.scan();
        }
        self.refresh();
        self.snapshot.clone()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WheelSettings {
    pub rotation: f32,
    pub gain: f32,
    pub autocenter: bool,
    /// 0..100 %, control 1073 in the original (default 50 %).
    pub autocenter_strength: f32,
}

/// Find the `hid-t150` sysfs directory for this wheel, if the module is loaded.
pub fn driver_interface(info: &DeviceInfo) -> Option<PathBuf> {
    let dir = fs::read_dir("/sys/bus/hid/devices").ok()?;
    for entry in dir.flatten() {
        let path = entry.path();
        let uevent = read_trim(path.join("uevent")).unwrap_or_default();
        let id_line = uevent
            .lines()
            .find(|l| l.starts_with("HID_ID="))
            .unwrap_or("");
        // HID_ID=0003:0000044F:0000B677
        let mut parts = id_line.split(':');
        let _bus = parts.next();
        let vendor = parts.next().and_then(|v| parse_hex(v));
        let product = parts.next().and_then(|v| parse_hex(v));
        if vendor == Some(info.vendor) && product == Some(info.product) {
            // Only the out-of-tree driver exposes these attributes.
            if path.join("range").exists() {
                return Some(path);
            }
        }
    }
    None
}

/// Write the panel's settings to the wheel firmware.  Everything is reported
/// per attribute so a half-working driver is visible instead of silent.
pub fn apply_settings(info: &DeviceInfo, settings: &WheelSettings) -> Result<String, String> {
    let Some(dir) = driver_interface(info) else {
        let next = if is_gip_wheel(info.product) {
            "Press Switch to PC mode in the panel: the wheel is in Xbox firmware (044f:b67e), which hid-t150 does not handle."
        } else {
            "Switch the wheel to PC mode and load hid-t150 to write them."
        };
        return Err(format!(
            "The wheel is in {} and no hid-t150 attributes are present. \
             Settings are live-read only; {}",
            info.mode_label(),
            next
        ));
    };

    let mut written = Vec::new();
    let mut failed = Vec::new();

    // T150 / TMX are 900° units (the original's 0x1800657bc flag).
    let limited = is_900_degree(info.product);
    let range = format!(
        "{}\n",
        crate::mechanics::clamp_angle(settings.rotation.round() as i32, limited)
    );
    match fs::write(dir.join("range"), range) {
        Ok(_) => written.push(format!("range {:.0} deg", settings.rotation)),
        Err(e) => failed.push(format!("range: {e}")),
    }

    let gain =
        (crate::mechanics::clamp_gain(settings.gain.round() as i32) as f32 * 655.35).round() as u32;
    match fs::write(dir.join("gain"), format!("{gain}\n")) {
        Ok(_) => written.push(format!("gain {:.0} %", settings.gain)),
        Err(e) => failed.push(format!("gain: {e}")),
    }

    let strength =
        (crate::mechanics::clamp_gain(settings.autocenter_strength.round() as i32) as f32 * 655.35)
            .round() as u32;
    match fs::write(dir.join("autocenter"), format!("{strength}\n")) {
        Ok(_) => written.push(format!(
            "auto-centre strength {:.0} %",
            settings.autocenter_strength
        )),
        Err(e) => failed.push(format!("auto-centre strength: {e}")),
    }

    let auto = if settings.autocenter { "1\n" } else { "0\n" };
    match fs::write(dir.join("enable_autocenter"), auto) {
        Ok(_) => written.push(format!(
            "auto-centre {}",
            if settings.autocenter { "on" } else { "off" }
        )),
        Err(e) => failed.push(format!("auto-centre: {e}")),
    }

    if failed.is_empty() {
        Ok(format!("Written to the wheel: {}.", written.join(", ")))
    } else if written.is_empty() {
        Err(format!(
            "The driver rejected every setting: {}.",
            failed.join("; ")
        ))
    } else {
        Err(format!(
            "Partly applied ({}); failed: {}.",
            written.join(", "),
            failed.join("; ")
        ))
    }
}

/// Firmware byte exposed by hid-t150, if the module is loaded.
pub fn firmware_version(info: &DeviceInfo) -> Option<u8> {
    let dir = driver_interface(info)?;
    read_trim(dir.join("firmware_version"))?.parse().ok()
}

/// The settings the wheel holds right now, read back from hid-t150.
///
/// The original fills its sliders from device getters when the page opens
/// (`sub_18000606d` calls `sub_180001f00` for the overall gain,
/// `sub_180001bb0` for constant, `sub_1800020a0` for periodic and
/// `sub_180001b20` for auto-center strength) instead of starting from the
/// Default button's numbers.  On Linux the same values live in the driver's
/// `range`, `gain`, `autocenter` and `enable_autocenter` attributes.  Only
/// the four settings hid-t150 exposes can be mirrored; constant, periodic and
/// spring/damper stay at their defaults because the module has no attributes
/// for them.
pub fn read_settings(info: &DeviceInfo) -> Option<WheelSettings> {
    let dir = driver_interface(info)?;
    let percent = |path: PathBuf| -> Option<f32> {
        let raw: f32 = read_trim(path)?.parse().ok()?;
        Some((raw * 100.0 / 65535.0).round().clamp(0.0, 100.0))
    };
    let rotation = read_trim(dir.join("range"))?
        .parse::<f32>()
        .ok()?
        .clamp(40.0, 1080.0);
    let gain = percent(dir.join("gain")).unwrap_or(100.0);
    let autocenter_strength = percent(dir.join("autocenter")).unwrap_or(50.0);
    let autocenter = read_trim(dir.join("enable_autocenter"))
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "y" | "yes" | "on"))
        .unwrap_or(false);
    Some(WheelSettings {
        rotation,
        gain,
        autocenter,
        autocenter_strength,
    })
}

// ---------------------------------------------------------------------------
// Xbox/GIP units: switching the wheel to PC mode
//
// A Thrustmaster wheel sold for Xbox (the TMX) boots with Xbox/GIP firmware and
// reports 044f:b67e.  That is not the firmware hid-t150 knows: the module only
// matches 044f:b677 (T150) and 044f:b67f (TMX in PC mode).  No amount of
// `modprobe` or rebooting can change the id - the wheel has to be told to
// change mode over USB after it is plugged in, exactly what the Windows driver
// and the `tmdrv` tool do with two vendor control requests:
//
//   044f:b67e  --wValue 1-->  044f:b65d  --wValue 7-->  044f:b67f
//
// The panel sends those itself so a person does not have to install anything
// else; the command is also exposed as `ttrs-wheel --tmx-switch` for a udev
// rule or a terminal.
// ---------------------------------------------------------------------------

pub const TMX_GIP_PRODUCT: u16 = 0xb67e;
const TMX_LOADER_PRODUCT: u16 = 0xb65d;
pub const TMX_PC_PRODUCT: u16 = 0xb67f;

/// Product ids whose firmware is Xbox/GIP and therefore invisible to hid-t150.
pub fn is_gip_wheel(product: u16) -> bool {
    product == TMX_GIP_PRODUCT
}

mod usb {
    //! The two libusb calls this needs, loaded at run time so the panel still
    //! starts on a machine without libusb installed.
    use std::ffi::{c_void, CString};

    pub type Ctx = *mut c_void;
    pub type Handle = *mut c_void;

    pub struct Usb {
        pub init: unsafe extern "C" fn(*mut Ctx) -> libc::c_int,
        pub exit: unsafe extern "C" fn(Ctx),
        pub open: unsafe extern "C" fn(Ctx, u16, u16) -> Handle,
        pub close: unsafe extern "C" fn(Handle),
        pub detach: unsafe extern "C" fn(Handle, libc::c_int) -> libc::c_int,
        pub claim: unsafe extern "C" fn(Handle, libc::c_int) -> libc::c_int,
        pub release: unsafe extern "C" fn(Handle, libc::c_int) -> libc::c_int,
        pub control:
            unsafe extern "C" fn(Handle, u8, u8, u16, u16, *mut u8, u16, u32) -> libc::c_int,
    }

    unsafe impl Send for Usb {}
    unsafe impl Sync for Usb {}

    static LIB: std::sync::OnceLock<Result<Usb, String>> = std::sync::OnceLock::new();

    pub fn load() -> Result<&'static Usb, String> {
        LIB.get_or_init(|| unsafe { load_now() })
            .as_ref()
            .map_err(|e| e.clone())
    }

    unsafe fn load_now() -> Result<Usb, String> {
        let name = CString::new("libusb-1.0.so.0").unwrap();
        let lib = libc::dlopen(name.as_ptr(), libc::RTLD_NOW);
        if lib.is_null() {
            return Err(String::from(
                "libusb-1.0 is not installed, so the panel cannot talk to the wheel's USB port.",
            ));
        }
        macro_rules! sym {
            ($name:literal, $ty:ty) => {{
                let name = CString::new($name).unwrap();
                let ptr = libc::dlsym(lib, name.as_ptr());
                if ptr.is_null() {
                    return Err(format!("libusb is missing {}.", $name));
                }
                std::mem::transmute::<*mut c_void, $ty>(ptr)
            }};
        }
        Ok(Usb {
            init: sym!("libusb_init", unsafe extern "C" fn(*mut Ctx) -> libc::c_int),
            exit: sym!("libusb_exit", unsafe extern "C" fn(Ctx)),
            open: sym!(
                "libusb_open_device_with_vid_pid",
                unsafe extern "C" fn(Ctx, u16, u16) -> Handle
            ),
            close: sym!("libusb_close", unsafe extern "C" fn(Handle)),
            detach: sym!(
                "libusb_set_auto_detach_kernel_driver",
                unsafe extern "C" fn(Handle, libc::c_int) -> libc::c_int
            ),
            claim: sym!(
                "libusb_claim_interface",
                unsafe extern "C" fn(Handle, libc::c_int) -> libc::c_int
            ),
            release: sym!(
                "libusb_release_interface",
                unsafe extern "C" fn(Handle, libc::c_int) -> libc::c_int
            ),
            control: sym!(
                "libusb_control_transfer",
                unsafe extern "C" fn(Handle, u8, u8, u16, u16, *mut u8, u16, u32) -> libc::c_int
            ),
        })
    }
}

fn usb_ids() -> Vec<(u16, u16)> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/bus/usb/devices") else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let vendor = read_trim(path.join("idVendor")).and_then(|v| parse_hex(&v));
        let product = read_trim(path.join("idProduct")).and_then(|v| parse_hex(&v));
        if let (Some(vendor), Some(product)) = (vendor, product) {
            found.push((vendor, product));
        }
    }
    found
}

fn usb_product_present(product: u16) -> bool {
    usb_ids()
        .iter()
        .any(|(vendor, id)| *vendor == VENDOR_THRUSTMASTER && *id == product)
}

fn usb_node_writable(product: u16) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(entries) = fs::read_dir("/sys/bus/usb/devices") else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if parse_hex(&read_trim(path.join("idVendor")).unwrap_or_default())
            != Some(VENDOR_THRUSTMASTER)
        {
            continue;
        }
        if parse_hex(&read_trim(path.join("idProduct")).unwrap_or_default()) != Some(product) {
            continue;
        }
        let bus = read_trim(path.join("busnum")).and_then(|v| v.parse::<u32>().ok());
        let dev = read_trim(path.join("devnum")).and_then(|v| v.parse::<u32>().ok());
        if let (Some(bus), Some(dev)) = (bus, dev) {
            let node = PathBuf::from(format!("/dev/bus/usb/{bus:03}/{dev:03}"));
            if let Ok(meta) = fs::metadata(&node) {
                let euid = unsafe { libc::geteuid() };
                return meta.uid() == euid || meta.mode() & 0o002 != 0;
            }
        }
    }
    false
}

fn wait_for_product(product: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if usb_product_present(product) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(80));
    }
}

fn open_with_retry(
    lib: &usb::Usb,
    ctx: usb::Ctx,
    product: u16,
    timeout: Duration,
) -> Result<usb::Handle, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let handle = unsafe { (lib.open)(ctx, VENDOR_THRUSTMASTER, product) };
        if !handle.is_null() {
            return Ok(handle);
        }
        // The device nodes under /dev/bus/usb are root-only until a udev rule
        // grants the session access, and retrying cannot change that.
        if unsafe { libc::geteuid() } != 0 && !usb_node_writable(product) {
            let exe = std::env::current_exe()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| String::from("ttrs-wheel"));
            return Err(format!(
                "The wheel is there as 044f:{product:04x}, but its USB port can only be opened by root. Run 'sudo {exe} --tmx-switch', or reinstall the udev rule from app/driver/t150."
            ));
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "The wheel did not answer as 044f:{product:04x}. If it is still in Xbox mode, unplug it and plug it in again, then press the button once more."
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn switch_steps(lib: &usb::Usb, ctx: usb::Ctx) -> Result<String, String> {
    for (product, value) in [
        (TMX_GIP_PRODUCT, 0x0001u16),
        (TMX_LOADER_PRODUCT, 0x0007u16),
    ] {
        let handle = open_with_retry(lib, ctx, product, Duration::from_secs(3))?;
        unsafe {
            (lib.detach)(handle, 1);
            let claimed = (lib.claim)(handle, 0);
            if claimed == -3 {
                (lib.close)(handle);
                return Err(String::from(
                    "The wheel's USB port needs administrator rights.",
                ));
            }
            // bmRequestType 0x41 (vendor, host to device, interface),
            // bRequest 0x53, no data - the same packet tmdrv sends.
            let rc = (lib.control)(handle, 0x41, 0x53, value, 0, std::ptr::null_mut(), 0, 1500);
            (lib.release)(handle, 0);
            (lib.close)(handle);
            // As the wheel changes mode it drops off the bus; IO (-1),
            // NO_DEVICE (-4) and PIPE (-9) are the expected answers.
            if rc < 0 && !matches!(rc, -1 | -4 | -9) {
                return Err(format!(
                    "The wheel refused the PC-mode command (libusb error {rc}). Unplug it and plug it back in, then try again."
                ));
            }
        }
    }

    if wait_for_product(TMX_PC_PRODUCT, Duration::from_secs(5)) {
        Ok(String::from(
            "The wheel switched itself to PC mode (044f:b67f). Loading hid-t150 for it...",
        ))
    } else {
        Err(String::from(
            "The command was sent, but the wheel did not come back as 044f:b67f. Unplug the USB cable, plug it back in and try once more.",
        ))
    }
}

/// Send the PC-mode switch to a wheel that is in Xbox/GIP firmware.
pub fn switch_gip_to_pc() -> Result<String, String> {
    if !usb_product_present(TMX_GIP_PRODUCT) && !usb_product_present(TMX_LOADER_PRODUCT) {
        if usb_product_present(TMX_PC_PRODUCT) {
            return Ok(String::from("The wheel is already in PC mode (044f:b67f)."));
        }
        return Err(String::from(
            "No wheel in Xbox mode is on the USB bus. Reconnect the wheel and press the button again.",
        ));
    }

    let lib = usb::load()?;
    unsafe {
        let mut ctx: usb::Ctx = std::ptr::null_mut();
        if (lib.init)(&mut ctx) != 0 {
            return Err(String::from("libusb could not start."));
        }
        let outcome = switch_steps(lib, ctx);
        (lib.exit)(ctx);
        outcome
    }
}

/// Same switch, but asking for the administrator password through polkit when
/// the USB port is not writable by this user (it is root-only by default).
pub fn switch_gip_to_pc_elevated() -> Result<String, String> {
    let root = unsafe { libc::geteuid() } == 0;
    if root || usb_node_writable(TMX_GIP_PRODUCT) {
        return switch_gip_to_pc();
    }
    let exe = std::env::current_exe()
        .map_err(|e| format!("Could not find the panel's own program file: {e}"))?;
    let mut child = Command::new("pkexec")
        .arg(&exe)
        .arg("--tmx-switch")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            format!(
                "Could not ask for the administrator password ({e}). Install polkit, or run 'sudo {} --tmx-switch' in a terminal.",
                exe.display()
            )
        })?;
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(120)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(String::from(
                    "The password prompt was not answered, so the wheel was left as it was.",
                ));
            }
            Err(e) => return Err(format!("The switch did not finish: {e}")),
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("Could not read the switch result: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if out.status.success() && !stdout.is_empty() {
        Ok(stdout)
    } else if !stderr.is_empty() {
        Err(stderr)
    } else {
        Err(String::from("The switch was cancelled."))
    }
}

// ---------------------------------------------------------------------------
// Idle sleep and screen locking
//
// KDE Plasma warns under Settings > Game Controller that the system may go to
// sleep or lock the screen while a controller is in use: the wheel produces no
// pointer or keyboard events, so the idle timer keeps running.  While the panel
// sees wheel input it holds a logind idle/sleep inhibitor, which is exactly the
// manual block that warning suggests.  The inhibitor is released as soon as the
// panel stops seeing input, and always when the panel exits.
// ---------------------------------------------------------------------------

pub struct SleepBlocker {
    child: Option<Child>,
    error: Option<String>,
}

impl SleepBlocker {
    pub fn new() -> Self {
        Self {
            child: None,
            error: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.child.is_some()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Start or stop the inhibitor.  Safe to call every frame.
    pub fn set(&mut self, want: bool) {
        if want == self.child.is_some() {
            return;
        }
        if want {
            match Command::new("systemd-inhibit")
                .args([
                    "--what=idle:sleep",
                    "--mode=block",
                    "--who=TTRS Control Panel",
                    "--why=Race wheel in use",
                    "sleep",
                    "infinity",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    self.child = Some(child);
                    self.error = None;
                }
                Err(err) => {
                    self.error = Some(format!(
                        "Could not start systemd-inhibit ({}). Use KDE Settings > Game Controller to allow the wheel as pointer and keyboard instead.",
                        err
                    ));
                }
            }
        } else if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn stop(&mut self) {
        self.set(false);
    }
}

impl Drop for SleepBlocker {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn desktop_is_kde() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|v| v.to_ascii_uppercase().contains("KDE"))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Connection and driver diagnostics
//
// The control panel can only report "no wheel" on its own.  These checks walk
// the same layers a person would: the USB bus, the input device, permissions,
// the kernel driver and the hid-t150 settings interface.  Every failure comes
// with one plain-language next step.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CheckState {
    Ok,
    Warn,
    Fail,
}

#[derive(Clone, Debug)]
pub struct Check {
    pub title: &'static str,
    pub state: CheckState,
    pub detail: String,
    pub hint: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Diagnostics {
    pub headline_state: CheckState,
    pub summary: String,
    pub checks: Vec<Check>,
}

/// Thrustmaster USB devices currently on the bus: (id, product, port, speed).
fn usb_wheels() -> Vec<(String, String, String, String)> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/bus/usb/devices") else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let vendor = read_trim(path.join("idVendor")).unwrap_or_default();
        if !vendor.eq_ignore_ascii_case("044f") {
            continue;
        }
        let product_id = read_trim(path.join("idProduct")).unwrap_or_default();
        let product =
            read_trim(path.join("product")).unwrap_or_else(|| String::from("Thrustmaster wheel"));
        let port = entry.file_name().to_string_lossy().to_string();
        let speed = read_trim(path.join("speed")).unwrap_or_else(|| String::from("?"));
        found.push((
            format!("{vendor}:{product_id}"),
            product,
            port,
            format!("{speed} Mbps"),
        ));
    }
    found.sort();
    found
}

fn module_loaded(name: &str) -> bool {
    fs::read_to_string("/proc/modules")
        .map(|text| {
            text.lines()
                .any(|line| line.split_whitespace().next() == Some(name))
        })
        .unwrap_or(false)
}

fn kernel_release() -> String {
    fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Is a hid-t150 module built for the running kernel?  A DKMS build that is
/// installed but not loaded is a very different situation from "never built",
/// and the panel should say which one it is.
fn module_installed() -> bool {
    let rel = kernel_release();
    if rel.is_empty() {
        return false;
    }
    let base = PathBuf::from("/lib/modules").join(rel);
    let dirs = ["updates/dkms", "extra", "kernel/drivers/hid"];
    dirs.iter().any(|dir| {
        let path = base.join(dir);
        let Ok(entries) = fs::read_dir(path) else {
            return false;
        };
        entries.flatten().any(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            name.starts_with("hid-t150.ko")
        })
    })
}

fn udev_rule_installed() -> bool {
    [
        "/etc/udev/rules.d/60-hid-t150.rules",
        "/usr/lib/udev/rules.d/60-hid-t150.rules",
        "/run/udev/rules.d/60-hid-t150.rules",
        // The older t150 package installed the rule under this name.
        "/etc/udev/rules.d/10-t150.rules",
        "/usr/lib/udev/rules.d/10-t150.rules",
    ]
    .iter()
    .any(|path| Path::new(path).exists())
}

/// Inspect every layer between the USB socket and the control panel.
pub fn diagnose(keep_awake: bool, blocking: bool) -> Diagnostics {
    let mut checks = Vec::new();
    let usb = usb_wheels();

    if usb.is_empty() {
        checks.push(Check {
            title: "USB connection",
            state: CheckState::Fail,
            detail: String::from("No Thrustmaster device is visible on the USB bus."),
            hint: Some(String::from(
                "Reconnect the wheel with its own cable, use a port directly on the PC (no hub), and check that the power supply is plugged in. Then press Check again.",
            )),
        });
    } else {
        for (id, product, port, speed) in &usb {
            checks.push(Check {
                title: "USB connection",
                state: CheckState::Ok,
                detail: format!("{product} ({id}) is on USB port {port}, {speed} link."),
                hint: None,
            });
        }
    }

    let info = discover();
    match &info {
        Some(info) => checks.push(Check {
            title: "Joystick device",
            state: CheckState::Ok,
            detail: if info.has_generic_kernel_name() {
                format!(
                    "Linux created {} for '{}'. That is the kernel's generic xpad name for {} - KDE Plasma and Steam show the same '{}' label for this wheel, not a separate device.",
                    info.node.display(),
                    info.name,
                    info.product_name(),
                    info.name
                )
            } else {
                format!(
                    "Linux created {} for '{}'.",
                    info.node.display(),
                    info.name
                )
            },
            hint: None,
        }),
        None if usb.is_empty() => checks.push(Check {
            title: "Joystick device",
            state: CheckState::Fail,
            detail: String::from("No /dev/input/js* node exists because no wheel reached the USB bus."),
            hint: None,
        }),
        None => checks.push(Check {
            title: "Joystick device",
            state: CheckState::Fail,
            detail: String::from(
                "The wheel is on the USB bus, but Linux did not create /dev/input/js*.",
            ),
            hint: Some(String::from(
                "Reconnect the wheel and run dmesg | tail -20 to see what the kernel reports. A T150 needs its sliding switch in the PS3 position.",
            )),
        }),
    }

    if let Some(info) = &info {
        if is_gip_wheel(info.product) {
            checks.push(Check {
                title: "Wheel mode",
                state: CheckState::Warn,
                detail: format!(
                    "The wheel reports 044f:{:04x}, the Xbox/GIP firmware of the TMX. hid-t150 only matches 044f:b677 (T150) and 044f:b67f (TMX in PC mode), so loading the module and rebooting cannot make it take over.",
                    info.product
                ),
                hint: Some(String::from(
                    "Press Switch to PC mode below. The panel sends the same two USB commands as the Windows driver and the wheel returns as 044f:b67f. The MODE button on the rim does not do this - it only changes pedal behaviour - and a TMX has no PC/Xbox switch.",
                )),
            });
        }
    }

    if let Some(info) = &info {
        let readable = File::open(&info.node).is_ok();
        checks.push(Check {
            title: "Device permissions",
            state: if readable { CheckState::Ok } else { CheckState::Fail },
            detail: if readable {
                format!("Your user can open {}.", info.node.display())
            } else {
                format!("Your user cannot open {}.", info.node.display())
            },
            hint: if readable {
                None
            } else {
                Some(String::from(
                    "Run sudo usermod -aG input $USER, then log out and back in. The udev rule in app/driver/t150 fixes this too.",
                ))
            },
        });

        let (state, detail, hint) = match info.driver.as_str() {
            "hid-t150" => (
                CheckState::Ok,
                String::from("The hid-t150 driver owns the wheel (PC mode)."),
                None,
            ),
            "xpad" => (
                CheckState::Warn,
                String::from(
                    "The built-in xpad driver owns the wheel (Xbox/GIP mode). Input works, but rotation, gain and auto-centre cannot be written.",
                ),
                Some(if is_gip_wheel(info.product) {
                    String::from(
                        "Press Switch to PC mode below: the wheel returns as 044f:b67f and hid-t150 takes over. Rebooting or reloading the module cannot change the id on its own.",
                    )
                } else {
                    String::from(
                        "Set the sliding switch on the wheel to the PS3 position and reconnect it: the settings then go through hid-t150.",
                    )
                }),
            ),
            other => (
                CheckState::Warn,
                format!("The wheel is handled by '{other}', not by hid-t150."),
                Some(String::from(
                    "Install the bundled hid-t150 driver from app/driver/t150 (sudo ./install.sh) and reconnect the wheel.",
                )),
            ),
        };
        checks.push(Check {
            title: "Kernel driver",
            state,
            detail,
            hint,
        });
    }

    let module = module_loaded("hid_t150");
    let built = module || module_installed();
    checks.push(Check {
        title: "hid-t150 module",
        state: if module { CheckState::Ok } else { CheckState::Warn },
        detail: if module {
            String::from("The hid-t150 kernel module is loaded.")
        } else if built {
            format!(
                "hid-t150 is installed for kernel {} but not loaded: the kernel loads it only for a PC-mode wheel.",
                kernel_release()
            )
        } else {
            format!(
                "hid-t150 is not built for kernel {}.",
                kernel_release()
            )
        },
        hint: if module {
            None
        } else if built {
            Some(String::from(
                "Reconnect the wheel in PC mode. It is also safe to load it now: sudo modprobe hid-t150.",
            ))
        } else {
            Some(String::from(
                "Install it once with sudo ./install.sh in app/driver/t150; it then loads automatically at boot.",
            ))
        },
    });

    let interface = info.as_ref().and_then(driver_interface);
    checks.push(Check {
        title: "Settings interface",
        state: if interface.is_some() {
            CheckState::Ok
        } else {
            CheckState::Warn
        },
        detail: if interface.is_some() {
            String::from("range, gain and auto-centre are exposed and can be written.")
        } else {
            String::from(
                "No range/gain/auto-centre attributes are present, so Apply cannot write to the wheel.",
            )
        },
        hint: if interface.is_some() {
            None
        } else if info.as_ref().map(|i| is_gip_wheel(i.product)).unwrap_or(false) {
            Some(String::from(
                "The wheel is still in Xbox firmware. Press Switch to PC mode below; after it returns as 044f:b67f the range, gain and auto-centre attributes appear.",
            ))
        } else if info.is_some() && built {
            Some(String::from(
                "The hid-t150 module is installed: reconnect the wheel so it takes over from xpad.",
            ))
        } else if info.is_some() {
            Some(String::from(
                "Install the driver from app/driver/t150, then reconnect the wheel.",
            ))
        } else {
            None
        },
    });

    let rule = udev_rule_installed();
    checks.push(Check {
        title: "udev rule",
        state: if rule {
            CheckState::Ok
        } else {
            CheckState::Warn
        },
        detail: if rule {
            String::from("60-hid-t150.rules is installed.")
        } else {
            String::from("60-hid-t150.rules is not installed; device permissions may need root.")
        },
        hint: if rule {
            None
        } else {
            Some(String::from(
                "Running sudo ./install.sh in app/driver/t150 installs the rule as well.",
            ))
        },
    });

    let (state, detail, hint) = if blocking {
        (
            CheckState::Ok,
            String::from("Idle sleep and screen locking are blocked while the wheel is in use."),
            None,
        )
    } else if keep_awake {
        (
            CheckState::Ok,
            String::from("Automatic blocking is on; it starts as soon as the wheel sends input."),
            None,
        )
    } else {
        (
            CheckState::Warn,
            String::from(
                "Automatic blocking is off, so KDE Plasma may sleep or lock the screen while the controller is in use.",
            ),
            Some(String::from(
                "Turn on 'Keep the screen awake while the wheel is in use' in Settings, or in KDE Settings > Game Controller allow the wheel to be used as pointer and keyboard.",
            )),
        )
    };
    checks.push(Check {
        title: "Sleep and screen lock",
        state,
        detail,
        hint,
    });

    if desktop_is_kde() && !blocking {
        checks.push(Check {
            title: "KDE Plasma",
            state: CheckState::Warn,
            detail: String::from(
                "Plasma's Game Controller page warns that the system can sleep or lock while a controller is in use.",
            ),
            hint: Some(String::from(
                "The panel blocks idle sleep on its own while the wheel moves. For games started outside the panel, enable the wheel as pointer and keyboard in KDE Settings > Game Controller.",
            )),
        });
    }

    let (headline_state, summary) = if usb.is_empty() {
        (
            CheckState::Fail,
            String::from(
                "Wheel not found. Check the cable, the USB port and the power supply first.",
            ),
        )
    } else if info.is_none() {
        (
            CheckState::Fail,
            String::from(
                "The wheel is connected to USB but Linux has not created a joystick device.",
            ),
        )
    } else if info
        .as_ref()
        .map(|i| is_gip_wheel(i.product))
        .unwrap_or(false)
    {
        (
            CheckState::Warn,
            String::from(
                "Wheel found, but it is running Xbox firmware (044f:b67e): input works, settings are read-only until it is switched to PC mode.",
            ),
        )
    } else if info.as_ref().map(|i| i.driver.as_str()) == Some("hid-t150") {
        (
            CheckState::Ok,
            String::from("Wheel found in PC mode. Connections and drivers look good."),
        )
    } else {
        (
            CheckState::Warn,
            String::from(
                "Wheel found, but it is in Xbox/GIP mode: input works, settings are read-only.",
            ),
        )
    };

    Diagnostics {
        headline_state,
        summary,
        checks,
    }
}

#[cfg(test)]
mod pedal_tests {
    use super::*;

    #[test]
    fn tmx_two_pedals_left_is_brake() {
        // What the TMX reports: steering, ABS_Y (left pedal), ABS_RZ (right
        // pedal) and a third axis that never moves because there is no clutch.
        let axmap = [ABS_X, ABS_Y, ABS_RZ, ABS_THROTTLE];
        let parked = [false, true, true, true];
        let (clutch, brake, gas) = pedal_axes(&axmap, &parked);
        assert_eq!(gas, Some(2), "right pedal is the throttle");
        assert_eq!(brake, Some(1), "left pedal is the brake");
        assert_eq!(clutch, Some(3), "the unused axis stays the clutch slot");
    }

    #[test]
    fn three_pedals_use_the_dedicated_axes() {
        let axmap = [ABS_X, ABS_GAS, ABS_BRAKE, ABS_THROTTLE];
        let parked = [false, true, true, true];
        let (clutch, brake, gas) = pedal_axes(&axmap, &parked);
        assert_eq!(gas, Some(1));
        assert_eq!(brake, Some(2));
        assert_eq!(clutch, Some(3));
    }

    #[test]
    fn two_axes_without_a_clutch_leave_it_empty() {
        let axmap = [ABS_X, ABS_Y, ABS_RZ];
        let parked = [false, true, true];
        let (clutch, brake, gas) = pedal_axes(&axmap, &parked);
        assert_eq!(gas, Some(2));
        assert_eq!(brake, Some(1));
        assert_eq!(clutch, None);
    }

    #[test]
    fn a_centred_y_axis_is_not_a_pedal() {
        // A stick parked at zero must not become the brake.
        let axmap = [ABS_X, ABS_Y, ABS_RZ];
        let parked = [false, false, true];
        let (_, brake, gas) = pedal_axes(&axmap, &parked);
        assert_eq!(gas, Some(2));
        assert_eq!(brake, None);
    }
}
