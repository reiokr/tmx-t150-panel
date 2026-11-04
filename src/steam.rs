//! Steam library scanner.
//!
//! Finds installed racing games so the panel can offer a profile per game
//! instead of asking the person to type names and AppIDs by hand.  It only
//! reads Steam's own files: `libraryfolders.vdf` lists the library roots and
//! each `appmanifest_<appid>.acf` carries the game's name.
//!
//! A library that cannot be read is reported rather than hidden: this machine
//! keeps a Steam library on a vfat partition whose mount options can make it
//! root-only, and a silent "no games found" would be misleading.

use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct SteamGame {
    pub appid: u32,
    pub name: String,
}

/// One game's tuning, taken from Brian Koponen's TMX/T150 pages.
pub struct Reference {
    /// Normalised substring used to match a Steam title.
    pub needle: &'static str,
    pub name: &'static str,
    pub appid: Option<u32>,
    /// Frostbite titles need the virtual pad under Wine.
    pub bridge: bool,
    pub rotation: f32,
    pub overall: f32,
    /// The in-game values, which the panel cannot write for the game.
    pub ingame: &'static str,
    pub url: &'static str,
    pub caveat: &'static str,
}

pub const REFERENCES: [Reference; 7] = [
    Reference {
        needle: "need for speed heat",
        name: "Need for Speed Heat",
        appid: Some(1222680),
        bridge: true,
        rotation: 480.0,
        overall: 100.0,
        ingame: "Wheel Deadzone 0.0%, Wheel Force Feedback 55%, Tire Force 50%, Spring Force 5%, \
                  Damper Force 0%, Surface Force 30%, Collision Force 45%.",
        url: "https://www.briankoponen.com/need-for-speed-heat-thrustmaster-tmx-t150-settings/",
        caveat: "Tested on this machine under Proton: the bridge is required.",
    },
    Reference {
        needle: "the crew 2",
        name: "The Crew 2",
        appid: Some(646910),
        bridge: true,
        rotation: 900.0,
        overall: 100.0,
        ingame: "Wheel Range 240°, Steering/Throttle/Brakes Preset Precise, Gearbox Sequential, \
                  Force Feedback two clicks below centre, Cockpit Camera Animation 0%, assists off. \
                  The game changes the wheel's rotation itself and does not restore it.",
        url: "https://www.briankoponen.com/the-crew-2-thrustmaster-tmx-t150-settings/",
        caveat: "Same Frostbite engine as NFS Heat; the reference assumes native wheel support, \
                 which Proton does not provide.",
    },
    Reference {
        needle: "project cars 2",
        name: "Project CARS 2",
        appid: Some(378860),
        bridge: false,
        rotation: 900.0,
        overall: 100.0,
        ingame: "Calibrate the wheel first; Steering Deadzone 0, Sensitivity 50, Speed Sensitivity 0; \
                  Force Feedback Custom: Gain 100, Volume 50, Tone 50, FX 15.",
        url: "https://www.briankoponen.com/project-cars-2-thrustmaster-tmx-t150-settings/",
        caveat: "Wheel works natively under Proton.",
    },
    Reference {
        needle: "assetto corsa",
        name: "Assetto Corsa",
        appid: Some(244210),
        bridge: false,
        rotation: 900.0,
        overall: 100.0,
        ingame: "Steering Gamma 1.00, Filter 0.00, Speed Sensitivity 0.00; FFB Gain 100%, Filter 0%, \
                  Minimum Force 0%, kerb/road/slip/ABS 0-10%. A WheelCheck LUT plus the FFBClip app \
                  sharpens the force feedback noticeably.",
        url: "https://www.briankoponen.com/assetto-corsa-thrustmaster-tmx-t150-settings/",
        caveat: "Wheel works natively under Proton.",
    },
    Reference {
        needle: "dirt 3",
        name: "DiRT 3 Complete Edition",
        appid: Some(321040),
        bridge: false,
        rotation: 540.0,
        overall: 100.0,
        ingame: "Add actionmap/tm_tmx.xml, an actionMapPaths.xml line and a devicesetup.xml FFBDevice \
                  line; then Steering/Throttle/Brake Deadzone 0%, Saturation 100%, Vibration Strength \
                  30%, Wheel Strength 50%, Wheel Weight 10%.",
        url: "https://www.briankoponen.com/dirt-3-thrustmaster-tmx-t150-settings/",
        caveat: "Older game: needs the action-map files before the wheel drives the menus.",
    },
    Reference {
        needle: "euro truck simulator 2",
        name: "Euro Truck Simulator 2",
        appid: Some(227300),
        bridge: false,
        rotation: 900.0,
        overall: 100.0,
        ingame: "Controller Subtype Wheel, Steering Sensitivity 0.40, Non-linearity 0.00, Animation \
                  Range 900°. FFB: Overall Gain 160%, Centering High 100% / Low 40%, Friction 50%, \
                  Engine 10%, Terrain 15%, Bumps 90%, Collisions 100%, Grind 32%, Understeer 20%. \
                  Uneven Surface Simulation 40%.",
        url: "https://www.briankoponen.com/euro-truck-simulator-2-thrustmaster-tmx-t150-settings/",
        caveat: "Wheel works natively under Proton.",
    },
    Reference {
        needle: "american truck simulator",
        name: "American Truck Simulator",
        appid: Some(270880),
        bridge: false,
        rotation: 900.0,
        overall: 100.0,
        ingame: "Same as Euro Truck Simulator 2: Wheel subtype, Sensitivity 0.40, Non-linearity \
                  0.00, Animation Range 900°, FFB Overall Gain 160%, Uneven Surface 40%.",
        url: "https://www.briankoponen.com/american-truck-simulator-thrustmaster-tmx-t150-settings/",
        caveat: "Wheel works natively under Proton.",
    },
];

/// Everything the panel needs to write a profile for a game.
pub struct Suggest {
    pub bridge: bool,
    pub rotation: f32,
    pub overall: f32,
    pub gain: f32,
    pub curve: f32,
    pub deadzone: f32,
    pub source: &'static str,
}

pub fn reference_for(name: &str) -> Option<&'static Reference> {
    let norm = normalize(name);
    REFERENCES.iter().find(|r| norm.contains(r.needle))
}

/// Substrings that mark a racing or rally title.  Deliberately conservative:
/// a false positive only costs one profile the person can delete.
const RACING: [&str; 24] = [
    "rally",
    "dirt",
    "assetto",
    "project cars",
    "forza",
    "f1 ",
    "f1 2",
    "wrc",
    "grid",
    "nascar",
    "need for speed",
    "the crew",
    "truck simulator",
    "beamng",
    "wreckfest",
    "automobilista",
    "rfactor",
    "raceroom",
    "motogp",
    "snowrunner",
    "dakar",
    "gravel",
    "le mans",
    "iracing",
];

/// Lowercase, with every run of punctuation (TM, (R), dashes) turned into one
/// space: "Need for Speed™ Heat" has to match "need for speed heat".
fn normalize(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut space = true;
    for ch in name.chars() {
        if ch.is_alphanumeric() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    out.trim().to_string()
}

pub fn is_racing(name: &str) -> bool {
    let lower = normalize(name);
    RACING.iter().any(|needle| lower.contains(needle))
}

/// Suggested tuning for a game: the reference table first, then the Frostbite
/// rule, then a conservative native default.
pub fn suggest(name: &str) -> Suggest {
    if let Some(reference) = reference_for(name) {
        return Suggest {
            bridge: reference.bridge,
            rotation: reference.rotation,
            overall: reference.overall,
            gain: if reference.bridge { 1.3 } else { 1.0 },
            curve: 1.0,
            deadzone: 0.0,
            source: "briankoponen.com, TMX/T150",
        };
    }
    let norm = normalize(name);
    if norm.contains("need for speed") || norm.contains("the crew") {
        return Suggest {
            bridge: true,
            rotation: 480.0,
            overall: 100.0,
            gain: 1.3,
            curve: 1.0,
            deadzone: 0.0,
            source: "Frostbite engine rule (bridge needed)",
        };
    }
    Suggest {
        bridge: false,
        rotation: 900.0,
        overall: 100.0,
        gain: 1.0,
        curve: 1.0,
        deadzone: 0.0,
        source: "default: wheel works natively",
    }
}

/// Every quote-delimited value on one VDF/ACF line.
fn quoted(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('"') else {
            break;
        };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out
}

/// Library roots, from `libraryfolders.vdf` plus the default install.
pub fn libraries() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/tmp"));
    let mut out: Vec<PathBuf> = Vec::new();
    for root in [
        PathBuf::from(&home).join(".local/share/Steam"),
        PathBuf::from(&home).join(".steam/steam"),
    ] {
        let vdf = root.join("steamapps/libraryfolders.vdf");
        if let Ok(text) = fs::read_to_string(&vdf) {
            for line in text.lines() {
                let values = quoted(line);
                if values.len() >= 2 && values[0].eq_ignore_ascii_case("path") {
                    out.push(PathBuf::from(&values[1]));
                }
            }
        }
        if root.join("steamapps").is_dir() {
            out.push(root);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn acf_name(text: &str) -> Option<String> {
    for line in text.lines() {
        let values = quoted(line);
        if values.len() >= 2 && values[0].eq_ignore_ascii_case("name") {
            return Some(values[1].trim().to_string());
        }
    }
    None
}

/// All installed games, plus one line per library that could not be read.
pub fn scan() -> (Vec<SteamGame>, Vec<String>) {
    let mut games: Vec<SteamGame> = Vec::new();
    let mut problems: Vec<String> = Vec::new();

    for library in libraries() {
        let steamapps = library.join("steamapps");
        let entries = match fs::read_dir(&steamapps) {
            Ok(entries) => entries,
            Err(error) => {
                problems.push(format!("{}: {error}", steamapps.display()));
                continue;
            }
        };
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().to_string();
            let Some(rest) = file.strip_prefix("appmanifest_") else {
                continue;
            };
            let Some(id) = rest.strip_suffix(".acf") else {
                continue;
            };
            let Ok(appid) = id.parse::<u32>() else {
                continue;
            };
            match fs::read_to_string(entry.path()) {
                Ok(text) => {
                    if let Some(name) = acf_name(&text) {
                        if !name.is_empty() {
                            games.push(SteamGame { appid, name });
                        }
                    }
                }
                Err(error) => problems.push(format!("{}: {error}", entry.path().display())),
            }
        }
    }

    games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    games.dedup_by_key(|g| g.appid);
    (games, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_vdf_paths() {
        let line = "\t\t\t\t\"path\"\t\t\"/mnt/nvme0n1p1/SteamLibrary\"";
        let values = quoted(line);
        assert_eq!(values, vec!["path", "/mnt/nvme0n1p1/SteamLibrary"]);
    }

    #[test]
    fn reads_acf_name() {
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"1222680\"\n\t\"name\"\t\t\"Need for Speed™ Heat \"\n\t\"installdir\"\t\t\"Need for Speed Heat\"\n}";
        assert_eq!(acf_name(acf).as_deref(), Some("Need for Speed™ Heat"));
    }

    #[test]
    fn spots_racing_titles() {
        for name in [
            "DiRT 3 Complete Edition",
            "Project CARS 2",
            "Assetto Corsa",
            "Euro Truck Simulator 2",
            "Need for Speed™ Heat",
            "The Crew 2",
            "EA Sports WRC",
            "F1 23",
        ] {
            assert!(is_racing(name), "{name} should be a racing title");
        }
        for name in ["Stellaris", "Cities: Skylines", "Transport Fever 2", "Chess"] {
            assert!(!is_racing(name), "{name} should not be a racing title");
        }
    }

    #[test]
    fn heat_gets_the_reference_values() {
        let s = suggest("Need for Speed™ Heat");
        assert!(s.bridge);
        assert_eq!(s.rotation, 480.0);
        assert_eq!(s.overall, 100.0);
        assert_eq!(s.gain, 1.3);
        assert!(s.source.contains("briankoponen"));
    }

    #[test]
    fn trademark_does_not_break_the_match() {
        let s = suggest("Need for Speed™ Heat ");
        assert!(s.source.contains("briankoponen"), "{}", s.source);
        assert!(is_racing("Need for Speed™ Heat "));
    }

    #[test]
    fn other_sims_stay_native() {
        let s = suggest("Assetto Corsa");
        assert!(!s.bridge);
        assert_eq!(s.rotation, 900.0);
    }
}
