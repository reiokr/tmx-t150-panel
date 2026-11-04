//! Panel state that is not written to the wheel.
//!
//! The Settings page mirrors the Windows panel, and several controls there go
//! through the vendor's COM property API (`tm_api_set_properties`), which has no
//! Linux equivalent: the drive mode, the HexaLed pattern, BOOST, LED intensity,
//! steering linearity and the rim choice.  `hid-t150` only exposes rotation,
//! gain and auto-centre.
//!
//! Those controls therefore cannot change the hardware on Linux, but the panel
//! should at least remember what was chosen instead of resetting to Sport on
//! every start.  This file stores them.

use std::path::PathBuf;

#[derive(Clone, PartialEq)]
pub struct PanelState {
    pub mode: usize,
    pub hexaled: bool,
    pub boost: bool,
    pub advanced: bool,
    pub led_intensity: f32,
    pub linearity: f32,
    pub rim: usize,
    pub pedals_separate: bool,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            // `sub_180005f60` selects index 1 (Sport) for the LED Default branch.
            mode: 1,
            hexaled: false,
            boost: false,
            advanced: true,
            led_intensity: 75.0,
            linearity: 100.0,
            rim: 0,
            pedals_separate: true,
        }
    }
}

impl PanelState {
    fn to_text(&self) -> String {
        format!(
            "mode={}\nhexaled={}\nboost={}\nadvanced={}\nled_intensity={:.1}\nlinearity={:.1}\nrim={}\npedals_separate={}\n",
            self.mode,
            self.hexaled as u8,
            self.boost as u8,
            self.advanced as u8,
            self.led_intensity,
            self.linearity,
            self.rim,
            self.pedals_separate as u8
        )
    }

    fn from_text(text: &str) -> Self {
        let mut state = PanelState::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "mode" => state.mode = value.parse().unwrap_or(state.mode),
                "hexaled" => state.hexaled = value == "1",
                "boost" => state.boost = value == "1",
                "advanced" => state.advanced = value == "1",
                "led_intensity" => {
                    state.led_intensity = value.parse().unwrap_or(state.led_intensity)
                }
                "linearity" => state.linearity = value.parse().unwrap_or(state.linearity),
                "rim" => state.rim = value.parse().unwrap_or(state.rim),
                "pedals_separate" => state.pedals_separate = value == "1",
                _ => {}
            }
        }
        state
    }
}

fn path() -> PathBuf {
    crate::paths::config_dir().join("panel.conf")
}

pub fn load() -> PanelState {
    std::fs::read_to_string(path())
        .map(|text| PanelState::from_text(&text))
        .unwrap_or_default()
}

pub fn save(state: &PanelState) {
    let path = path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, state.to_text());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_original() {
        let state = PanelState::default();
        assert_eq!(state.mode, 1, "Sport is the opening mode");
        assert_eq!(state.led_intensity, 75.0);
        assert_eq!(state.linearity, 100.0);
    }

    #[test]
    fn round_trip() {
        let mut state = PanelState::default();
        state.mode = 2;
        state.hexaled = true;
        state.led_intensity = 42.0;
        state.rim = 3;
        state.pedals_separate = false;
        let again = PanelState::from_text(&state.to_text());
        assert_eq!(again.mode, 2);
        assert!(again.hexaled);
        assert!((again.led_intensity - 42.0).abs() < 1e-3);
        assert_eq!(again.rim, 3);
        assert!(!again.pedals_separate);
    }
}
