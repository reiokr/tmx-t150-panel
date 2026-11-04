//! Real force feedback on Linux.
//!
//! The original panel uploads the effects stored in `CPL_effects/*.ffe` to the
//! wheel through DirectInput.  Those files are RIFF `FORC` containers: each
//! holds one to three `DIEFFECT` blocks with a periodic, constant or damper
//! type, an envelope and its own gain and duration.  On Linux the same thing is
//! done through the kernel's evdev force-feedback interface: `EVIOCSFF`
//! uploads an effect and an `EV_FF` event starts it.
//!
//! The joystick API used for reading input (`/dev/input/js*`) cannot carry
//! effects, so this module finds the matching `/dev/input/event*` node and
//! keeps its own file handle.
//!
//! Everything here is optional: without a wheel, without permission or on a
//! device that does not advertise the needed waveform, `play` reports why
//! instead of pretending to shake the rim.

use crate::device::DeviceInfo;
use std::fs;
use std::io::Write;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

// linux/input.h
const EV_FF: u16 = 0x15;
const FF_RUMBLE: u16 = 0x50;
const FF_PERIODIC: u16 = 0x51;
const FF_CONSTANT: u16 = 0x52;
const FF_SQUARE: u16 = 0x58;
const FF_TRIANGLE: u16 = 0x59;
const FF_SINE: u16 = 0x5a;
const FF_GAIN: u16 = 0x60;
const FF_AUTOCENTER: u16 = 0x61;

// `_IOW('E', 0x80, struct ff_effect)`; the structure is 48 bytes on x86_64.
const EVIOCSFF: libc::c_ulong = 0x4030_4580;
const EVIOCRMFF: libc::c_ulong = 0x4004_4581;
const EVIOCGEFFECTS: libc::c_ulong = 0x8004_4584;

fn eviocgbit(ev: u16, len: usize) -> libc::c_ulong {
    (2u64 << 30) as libc::c_ulong
        | ((len as u64) << 16) as libc::c_ulong
        | ((b'E' as u64) << 8) as libc::c_ulong
        | (0x20 + ev as u64) as libc::c_ulong
}

/// The kernel's `struct input_event` on a 64-bit machine: timeval, type, code,
/// value.  Written straight to the evdev node.
#[repr(C)]
#[derive(Clone, Copy)]
struct InputEvent {
    sec: i64,
    usec: i64,
    kind: u16,
    code: u16,
    value: i32,
}

/// `struct ff_effect`, byte-exact.  The 16-byte header is followed by the
/// 32-byte union; the periodic and constant members are used here.
#[repr(C)]
#[derive(Clone, Copy)]
struct FfEffect([u8; 48]);

impl FfEffect {
    /// Scale a DirectInput level (0..10000) to the kernel's i16 level.
    fn level(value: i32, max: i32) -> i16 {
        (32767.0 * value as f32 / max.max(1) as f32)
            .round()
            .clamp(0.0, 32767.0) as i16
    }

    fn header(kind: u16, direction: u16, length_ms: u16) -> [u8; 48] {
        let mut b = [0u8; 48];
        b[0..2].copy_from_slice(&kind.to_le_bytes());
        b[2..4].copy_from_slice(&(-1i16).to_le_bytes());
        b[4..6].copy_from_slice(&direction.to_le_bytes());
        b[10..12].copy_from_slice(&length_ms.to_le_bytes()); // replay length
        b[12..14].copy_from_slice(&0u16.to_le_bytes()); // replay delay
        b
    }

    fn periodic(
        waveform: u16,
        period_ms: u16,
        magnitude: i16,
        offset: i16,
        phase: u16,
        length_ms: u16,
        envelope: Envelope,
    ) -> Self {
        let mut b = Self::header(FF_PERIODIC, 0, length_ms);
        let u = 16;
        b[u..u + 2].copy_from_slice(&waveform.to_le_bytes());
        b[u + 2..u + 4].copy_from_slice(&period_ms.max(1).to_le_bytes());
        b[u + 4..u + 6].copy_from_slice(&magnitude.to_le_bytes());
        b[u + 6..u + 8].copy_from_slice(&offset.to_le_bytes());
        b[u + 8..u + 10].copy_from_slice(&phase.to_le_bytes());
        // envelope lives at union offset 10..18
        b[u + 10..u + 12].copy_from_slice(&envelope.attack_length_ms.to_le_bytes());
        b[u + 12..u + 14].copy_from_slice(&envelope.attack_level.to_le_bytes());
        b[u + 14..u + 16].copy_from_slice(&envelope.fade_length_ms.to_le_bytes());
        b[u + 16..u + 18].copy_from_slice(&envelope.fade_level.to_le_bytes());
        Self(b)
    }

    fn constant(level: i16, length_ms: u16, envelope: Envelope) -> Self {
        // `hid-t150` scales a constant effect by sin(direction):
        // `level * fixp_sin16(direction / (0xFFFF / 360)) >> 15`
        // (forcefeedback.c, t150_ff_prepare_update).  Direction 0 therefore
        // zeroes the force, which is why Blown Tire's -10000 constant part
        // arrived silent.  A quarter turn (0x4000 = 90°) keeps the level
        // unchanged; the sign still comes from `level` itself.
        let mut b = Self::header(FF_CONSTANT, 0x4000, length_ms);
        let u = 16;
        b[u..u + 2].copy_from_slice(&level.to_le_bytes());
        b[u + 2..u + 4].copy_from_slice(&envelope.attack_length_ms.to_le_bytes());
        b[u + 4..u + 6].copy_from_slice(&envelope.attack_level.to_le_bytes());
        b[u + 6..u + 8].copy_from_slice(&envelope.fade_length_ms.to_le_bytes());
        b[u + 8..u + 10].copy_from_slice(&envelope.fade_level.to_le_bytes());
        Self(b)
    }

    fn id(&self) -> i16 {
        i16::from_le_bytes([self.0[2], self.0[3]])
    }
}

#[derive(Clone, Copy, Default)]
struct Envelope {
    attack_length_ms: u16,
    attack_level: i16,
    fade_length_ms: u16,
    fade_level: i16,
}

/// One `DIEFFECT` from a preset.  Values keep the file's units; `play`
/// converts them to the kernel's scale.
#[derive(Clone, Copy)]
pub struct Pulse {
    /// Preset name, shown when something fails.
    pub name: &'static str,
    /// `FF_SINE` / `FF_SQUARE` / `FF_TRIANGLE` / `FF_CONSTANT`.
    pub waveform: u16,
    /// `DIPERIODIC.dwPeriod`, microseconds.
    pub period_us: u32,
    /// `DIPERIODIC.dwMagnitude`, 0..10000.
    pub magnitude: i32,
    /// `DIPERIODIC.lOffset`, -10000..10000.
    pub offset: i32,
    /// `DIPERIODIC.dwPhase`, hundredths of a degree.
    pub phase: u16,
    /// Envelope: attack level (0..10000), length in µs, fade level, length.
    pub attack_level: i32,
    pub attack_us: u32,
    pub fade_level: i32,
    pub fade_us: u32,
    /// `DIEFFECT.dwDuration`, seconds (already limited to the preset's run).
    pub seconds: f32,
    /// `DIEFFECT.dwGain`, 0..10000.
    pub gain: u32,
    /// Overall Strength slider, 0..1.
    pub overall: f32,
}

pub struct ForceFeedback {
    file: Option<fs::File>,
    node: Option<PathBuf>,
    effect: Vec<i16>,
    pub supported: bool,
    pub caps: u128,
    pub effects_max: i32,
    pub error: Option<String>,
    last_played: Option<&'static str>,
    last_count: usize,
    last_total: usize,
    last_gain: Option<i32>,
}

impl Default for ForceFeedback {
    fn default() -> Self {
        Self::new()
    }
}

impl ForceFeedback {
    pub fn new() -> Self {
        Self {
            file: None,
            node: None,
            effect: Vec::new(),
            supported: false,
            caps: 0,
            effects_max: 0,
            error: None,
            last_played: None,
            last_count: 0,
            last_total: 0,
            last_gain: None,
        }
    }

    /// Is a handle open and able to take effects?
    pub fn is_ready(&self) -> bool {
        self.file.is_some() && self.supported
    }

    pub fn node(&self) -> Option<&PathBuf> {
        self.node.as_ref()
    }

    pub fn last_played(&self) -> Option<&'static str> {
        self.last_played
    }

    pub fn last_count(&self) -> usize {
        self.last_count
    }

    pub fn last_total(&self) -> usize {
        self.last_total
    }

    /// Follow the joystick the panel is reading.  Called when the device list
    /// changes; opening is cheap and idempotent.
    pub fn attach(&mut self, info: Option<&DeviceInfo>) {
        let node = info.and_then(event_node_for);
        if node == self.node {
            return;
        }
        self.close();
        let Some(node) = node else {
            self.error = Some(String::from("The wheel exposes no /dev/input/event node."));
            return;
        };
        match fs::OpenOptions::new().read(true).write(true).open(&node) {
            Ok(file) => {
                let mut bits = [0u8; 16];
                let rc = unsafe {
                    libc::ioctl(
                        file.as_raw_fd(),
                        eviocgbit(EV_FF, bits.len()),
                        bits.as_mut_ptr(),
                    )
                };
                let ff_bits = if rc >= 0 {
                    u128::from_le_bytes(bits)
                } else {
                    0
                };
                let periodic = ff_bits & (1u128 << FF_PERIODIC) != 0;
                let mut max: libc::c_int = 0;
                let _ = unsafe { libc::ioctl(file.as_raw_fd(), EVIOCGEFFECTS, &mut max) };
                self.file = Some(file);
                self.node = Some(node.clone());
                self.supported = periodic;
                self.caps = ff_bits;
                self.effects_max = max;
                self.last_gain = None;
                self.error = if periodic {
                    None
                } else {
                    Some(format!(
                        "{} does not advertise periodic force feedback; test effects can only be shown on screen.",
                        node.display()
                    ))
                };
            }
            Err(err) => {
                self.node = Some(node.clone());
                self.error = Some(format!(
                    "Cannot write forces to {} ({}). Add your user to the input group and log in again.",
                    node.display(),
                    err
                ));
            }
        }
    }

    fn write_event(&mut self, kind: u16, code: u16, value: i32) -> std::io::Result<()> {
        let Some(file) = &mut self.file else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "no force feedback device",
            ));
        };
        let ev = InputEvent {
            sec: 0,
            usec: 0,
            kind,
            code,
            value,
        };
        let bytes = unsafe {
            std::slice::from_raw_parts(
                &ev as *const InputEvent as *const u8,
                std::mem::size_of::<InputEvent>(),
            )
        };
        file.write_all(bytes)?;
        file.flush()
    }

    /// Stop every effect this panel started.
    pub fn stop(&mut self) {
        let ids = std::mem::take(&mut self.effect);
        for id in ids {
            let _ = self.write_event(EV_FF, id as u16, 0);
            if let Some(file) = &self.file {
                let id32 = id as libc::c_int;
                let _ = unsafe { libc::ioctl(file.as_raw_fd(), EVIOCRMFF, &id32) };
            }
        }
    }

    /// Upload and start every part of a preset.  Parts whose waveform the wheel
    /// does not support are skipped, so a missing constant/damper part never
    /// hides the periodic ones.
    pub fn play(&mut self, pulses: &[Pulse]) -> Result<String, String> {
        if self.file.is_none() {
            return Err(String::from("No wheel to send forces to."));
        }
        if !self.supported {
            return Err(self
                .error
                .clone()
                .unwrap_or_else(|| String::from("This wheel has no periodic force feedback.")));
        }
        self.stop();
        let mut skipped = 0usize;
        let mut mapped = 0usize;
        for pulse in pulses {
            let Some(waveform) = self.usable_waveform(pulse.waveform) else {
                skipped += 1;
                continue;
            };
            if waveform != pulse.waveform {
                mapped += 1;
            }
            let length_ms = (pulse.seconds * 1000.0).round().clamp(20.0, 65000.0) as u16;
            let gain = pulse.gain.clamp(0, 10_000) as f32 / 10_000.0;
            let overall = pulse.overall.clamp(0.0, 1.0);
            let envelope = Envelope {
                attack_length_ms: (pulse.attack_us / 1000).min(20_000) as u16,
                attack_level: FfEffect::level(pulse.attack_level, 10_000),
                fade_length_ms: (pulse.fade_us / 1000).min(20_000) as u16,
                fade_level: FfEffect::level(pulse.fade_level, 10_000),
            };
            let mut fx = if waveform == FF_CONSTANT {
                let level = (32767.0 * (pulse.magnitude as f32 / 10_000.0) * gain * overall)
                    .round()
                    .clamp(-32767.0, 32767.0) as i16;
                FfEffect::constant(level, length_ms, envelope)
            } else {
                let magnitude = (32767.0 * (pulse.magnitude as f32 / 10_000.0) * gain * overall)
                    .round()
                    .clamp(0.0, 32767.0) as i16;
                let offset = (32767.0 * (pulse.offset as f32 / 10_000.0) * gain * overall)
                    .round()
                    .clamp(-32767.0, 32767.0) as i16;
                FfEffect::periodic(
                    waveform,
                    (pulse.period_us / 1000).max(1).min(20_000) as u16,
                    magnitude,
                    offset,
                    pulse.phase,
                    length_ms,
                    envelope,
                )
            };
            let Some(file) = &self.file else {
                break;
            };
            let rc = unsafe { libc::ioctl(file.as_raw_fd(), EVIOCSFF, fx.0.as_mut_ptr()) };
            if rc < 0 {
                skipped += 1;
                continue;
            }
            let id = fx.id();
            if self.write_event(EV_FF, id as u16, 1).is_ok() {
                self.effect.push(id);
            } else if let Some(file) = &self.file {
                let id32 = id as libc::c_int;
                let _ = unsafe { libc::ioctl(file.as_raw_fd(), EVIOCRMFF, &id32) };
            }
        }
        if self.effect.is_empty() {
            self.error = Some(String::from(
                "The driver accepted none of this preset's effects.",
            ));
            return Err(self.error.clone().unwrap_or_default());
        }
        self.last_played = Some(pulses[0].name);
        self.last_count = self.effect.len();
        self.last_total = pulses.len();
        let mut msg = format!(
            "Playing {} on the wheel ({} of {} effect parts).",
            pulses[0].name,
            self.effect.len(),
            pulses.len()
        );
        if skipped > 0 {
            msg.push_str(&format!(
                " {skipped} part(s) use a force this wheel does not support."
            ));
        }
        if mapped > 0 {
            msg.push_str(&format!(
                " {mapped} part(s) play as a sine wave on this driver."
            ));
        }
        self.error = None;
        Ok(msg)
    }

    /// The waveform to send, or `None` when the wheel cannot render it.
    ///
    /// hid-t150 advertises sine/saw, constant, spring and damper, but its
    /// upload treats every other periodic waveform as sine.  Car Crash and
    /// Force Field are made only of square/triangle parts, so without this
    /// fallback every part was skipped and the wheel stayed still even though
    /// the wheel can play them as a sine.
    fn usable_waveform(&self, waveform: u16) -> Option<u16> {
        if self.waveform_supported(waveform) {
            return Some(waveform);
        }
        match waveform {
            FF_SQUARE | FF_TRIANGLE if self.waveform_supported(FF_SINE) => Some(FF_SINE),
            _ => None,
        }
    }

    fn waveform_supported(&self, waveform: u16) -> bool {
        match waveform {
            FF_CONSTANT => self.caps & (1u128 << FF_CONSTANT) != 0,
            _ => self.caps & (1u128 << waveform) != 0,
        }
    }

    /// FF_GAIN is a global multiplier the driver applies to every effect, so the
    /// Overall Strength slider keeps working even in Xbox mode.
    pub fn set_gain(&mut self, pct: f32) {
        let value = (pct.clamp(0.0, 100.0) * 655.35).round() as i32;
        if self.last_gain == Some(value) {
            return;
        }
        if self.file.is_none() {
            return;
        }
        if self.write_event(EV_FF, FF_GAIN, value).is_ok() {
            self.last_gain = Some(value);
        }
    }

    /// FF_AUTOCENTER is separate from the effect gain; not every wheel has it.
    pub fn set_autocenter(&mut self, pct: f32) {
        if self.file.is_none() {
            return;
        }
        let value = (pct.clamp(0.0, 100.0) * 655.35).round() as i32;
        let _ = self.write_event(EV_FF, FF_AUTOCENTER, value);
    }

    fn close(&mut self) {
        self.stop();
        self.file = None;
        self.node = None;
        self.supported = false;
        self.caps = 0;
        self.effects_max = 0;
    }
}

impl Drop for ForceFeedback {
    fn drop(&mut self) {
        self.close();
    }
}

/// Find `/dev/input/eventN` for the joystick node.  Both nodes hang off the
/// same `inputN` device in sysfs, so compare the canonical device paths.
pub(crate) fn event_node_for(info: &DeviceInfo) -> Option<PathBuf> {
    let js_name = info.node.file_name()?.to_string_lossy().to_string();
    let js_device = fs::canonicalize(
        PathBuf::from("/sys/class/input")
            .join(&js_name)
            .join("device"),
    )
    .ok()?;
    let entries = fs::read_dir("/sys/class/input").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("event") {
            continue;
        }
        let device = fs::canonicalize(entry.path().join("device")).ok();
        if device.as_ref() == Some(&js_device) {
            return Some(PathBuf::from("/dev/input").join(name));
        }
    }
    None
}

/// Human-readable capabilities, used by the wheel-output and diagnostics cards.
pub fn capability_summary(ff_bits: u128) -> String {
    let mut names = Vec::new();
    for (bit, name) in [
        (FF_RUMBLE, "rumble"),
        (FF_PERIODIC, "periodic"),
        (FF_CONSTANT, "constant"),
        (FF_SQUARE, "square"),
        (FF_TRIANGLE, "triangle"),
        (FF_SINE, "sine"),
        (FF_GAIN, "gain"),
        (FF_AUTOCENTER, "auto-centre"),
    ] {
        if ff_bits & (1u128 << bit) != 0 {
            names.push(name);
        }
    }
    if names.is_empty() {
        String::from("none")
    } else {
        names.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel_with(waveforms: &[u16]) -> ForceFeedback {
        let mut wheel = ForceFeedback::new();
        wheel.caps = waveforms
            .iter()
            .fold(0u128, |caps, &bit| caps | (1u128 << bit));
        wheel
    }

    #[test]
    fn square_and_triangle_fall_back_to_sine() {
        let wheel = wheel_with(&[FF_PERIODIC, FF_SINE]);
        assert_eq!(wheel.usable_waveform(FF_SQUARE), Some(FF_SINE));
        assert_eq!(wheel.usable_waveform(FF_TRIANGLE), Some(FF_SINE));
        assert_eq!(wheel.usable_waveform(FF_SINE), Some(FF_SINE));
    }

    #[test]
    fn advertised_waveform_is_sent_unchanged() {
        let wheel = wheel_with(&[FF_PERIODIC, FF_SINE, FF_SQUARE, FF_TRIANGLE]);
        assert_eq!(wheel.usable_waveform(FF_SQUARE), Some(FF_SQUARE));
        assert_eq!(wheel.usable_waveform(FF_TRIANGLE), Some(FF_TRIANGLE));
    }

    #[test]
    fn constant_survives_when_the_wheel_has_it() {
        let wheel = wheel_with(&[FF_CONSTANT, FF_PERIODIC, FF_SINE]);
        assert_eq!(wheel.usable_waveform(FF_CONSTANT), Some(FF_CONSTANT));
    }

    #[test]
    fn unsupported_constant_is_not_replaced_by_sine() {
        let wheel = wheel_with(&[FF_PERIODIC, FF_SINE]);
        assert_eq!(wheel.usable_waveform(FF_CONSTANT), None);
    }

    /// `hid-t150` multiplies a constant level by sin(direction).  Sending
    /// direction 0 made Blown Tire's constant part exactly zero, so the only
    /// part left was its very small 8.5 % square (sent as sine).  A quarter
    /// turn keeps the level while periodic effects still work with 0.
    #[test]
    fn constant_uses_a_quarter_turn_direction() {
        let env = Envelope::default();
        let constant = FfEffect::constant(-32767, 900, env);
        assert_eq!(u16::from_le_bytes([constant.0[4], constant.0[5]]), 0x4000);
        let periodic = FfEffect::periodic(FF_SINE, 50, 1000, 0, 0, 900, env);
        assert_eq!(u16::from_le_bytes([periodic.0[4], periodic.0[5]]), 0);
    }

    /// hid-t150 advertises sine and constant but not square/triangle.  Before
    /// the sine fallback, Car Crash and Force Field had no playable part at all.
    #[test]
    fn every_preset_has_a_part_the_t150_can_play() {
        let wheel = wheel_with(&[FF_PERIODIC, FF_SINE, FF_CONSTANT]);
        for (i, parts) in crate::panel::PARTS.iter().enumerate() {
            assert!(
                parts
                    .iter()
                    .any(|part| wheel.usable_waveform(part.waveform).is_some()),
                "preset {} ({}) has no part the t150 can play",
                i + 1,
                crate::panel::EFFECTS[i].name
            );
        }
    }
}
