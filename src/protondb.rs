//! ProtonDB verdicts, one per Steam AppID.
//!
//! The panel asks ProtonDB for the summary of the profile's game and shows the
//! tier next to it, so a profile that cannot possibly run (BattlEye titles such
//! as The Crew 2 are "borked") is visible before anyone starts debugging.
//!
//! Requests go through `curl` instead of a TLS crate: the app has no HTTP
//! dependency, and shelling out to curl keeps the build small.  Results are
//! cached on disk for a day so switching profiles does not hammer the API.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    Platinum,
    Gold,
    Silver,
    Bronze,
    Borked,
    Pending,
    Unknown,
}

impl Tier {
    pub fn name(self) -> &'static str {
        match self {
            Tier::Platinum => "Platinum",
            Tier::Gold => "Gold",
            Tier::Silver => "Silver",
            Tier::Bronze => "Bronze",
            Tier::Borked => "Borked",
            Tier::Pending => "Pending",
            Tier::Unknown => "No data",
        }
    }

    fn parse(text: &str) -> Tier {
        match text.trim().to_lowercase().as_str() {
            "platinum" => Tier::Platinum,
            "gold" => Tier::Gold,
            "silver" => Tier::Silver,
            "bronze" => Tier::Bronze,
            "borked" => Tier::Borked,
            "pending" => Tier::Pending,
            _ => Tier::Unknown,
        }
    }

    /// Something is wrong with the game under Proton, not just "less polished".
    pub fn is_bad(self) -> bool {
        matches!(self, Tier::Borked)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Status {
    pub tier: Tier,
    pub trending: Tier,
    pub total: u32,
    /// Unix seconds when this was fetched.
    pub fetched: u64,
}

impl Status {
    fn stale(&self, now: u64) -> bool {
        now.saturating_sub(self.fetched) > 24 * 60 * 60
    }
}

fn cache_path() -> std::path::PathBuf {
    crate::paths::config_dir().join("protondb.cache")
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}


/// The API's JSON, read without a JSON parser: only four fields matter.
pub fn parse(text: &str) -> Option<Status> {
    fn quoted(text: &str, key: &str) -> Option<String> {
        let needle = format!("\"{key}\"");
        let start = text.find(&needle)? + needle.len();
        let rest = &text[start..];
        let open = rest.find('"')? + 1;
        let rest = &rest[open..];
        let close = rest.find('"')?;
        Some(rest[..close].to_string())
    }
    fn number(text: &str, key: &str) -> Option<u32> {
        let needle = format!("\"{key}\"");
        let start = text.find(&needle)? + needle.len();
        let rest = text[start..].trim_start();
        let rest = rest.strip_prefix(':')?.trim_start();
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    }

    let tier = Tier::parse(&quoted(text, "tier")?);
    let trending = quoted(text, "trendingTier")
        .map(|t| Tier::parse(&t))
        .unwrap_or(tier);
    let total = number(text, "total").unwrap_or(0);
    Some(Status {
        tier,
        trending,
        total,
        fetched: now_secs(),
    })
}

fn fetch(appid: u32) -> Option<Status> {
    let url = format!(
        "https://www.protondb.com/api/v1/reports/summaries/{appid}.json"
    );
    let output = std::process::Command::new("curl")
        .args(["-sfL", "--max-time", "8", "-A", "tmx-t150-panel", &url])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse(&String::from_utf8_lossy(&output.stdout))
}

/// Ask ProtonDB in the background; the receiver yields the parsed status.
pub fn spawn(appid: u32) -> std::sync::mpsc::Receiver<(u32, Option<Status>)> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send((appid, fetch(appid)));
    });
    rx
}

#[derive(Default)]
pub struct Cache {
    entries: HashMap<u32, Status>,
}

impl Cache {
    pub fn load() -> Self {
        let mut cache = Cache::default();
        let Ok(text) = std::fs::read_to_string(cache_path()) else {
            return cache;
        };
        for line in text.lines() {
            let parts: Vec<&str> = line.split('|').collect();
            if parts.len() < 5 {
                continue;
            }
            let Ok(appid) = parts[0].parse::<u32>() else {
                continue;
            };
            let Ok(fetched) = parts[4].parse::<u64>() else {
                continue;
            };
            cache.entries.insert(
                appid,
                Status {
                    tier: Tier::parse(parts[1]),
                    trending: Tier::parse(parts[2]),
                    total: parts[3].parse().unwrap_or(0),
                    fetched,
                },
            );
        }
        cache
    }

    pub fn save(&self) {
        let path = cache_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut out = String::new();
        for (appid, status) in &self.entries {
            out.push_str(&format!(
                "{appid}|{}|{}|{}|{}\n",
                status.tier.name(),
                status.trending.name(),
                status.total,
                status.fetched
            ));
        }
        let _ = std::fs::write(path, out);
    }

    pub fn get(&self, appid: u32) -> Option<Status> {
        self.entries.get(&appid).copied()
    }

    pub fn needs_fetch(&self, appid: u32) -> bool {
        match self.entries.get(&appid) {
            Some(status) => status.stale(now_secs()),
            None => true,
        }
    }

    pub fn insert(&mut self, appid: u32, status: Status) {
        self.entries.insert(appid, status);
        self.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_borked_summary() {
        let json = r#"{
  "bestReportedTier": "platinum",
  "confidence": "strong",
  "score": 0.01,
  "tier": "borked",
  "total": 54,
  "trendingTier": "borked"
}"#;
        let status = parse(json).expect("parsed");
        assert_eq!(status.tier, Tier::Borked);
        assert_eq!(status.trending, Tier::Borked);
        assert_eq!(status.total, 54);
        assert!(status.tier.is_bad());
    }

    #[test]
    fn parses_a_gold_summary() {
        let json = r#"{"bestReportedTier":"platinum","tier":"gold","total":12,"trendingTier":"gold"}"#;
        let status = parse(json).expect("parsed");
        assert_eq!(status.tier, Tier::Gold);
        assert_eq!(status.total, 12);
        assert!(!status.tier.is_bad());
    }

    #[test]
    fn garbage_has_no_status() {
        assert!(parse("<html>nope</html>").is_none());
        assert!(parse("{}").is_none());
    }

    #[test]
    fn stale_after_a_day() {
        let status = Status {
            tier: Tier::Gold,
            trending: Tier::Gold,
            total: 1,
            fetched: 1_000_000,
        };
        assert!(!status.stale(1_000_000 + 60));
        assert!(status.stale(1_000_000 + 90_000));
    }
}
