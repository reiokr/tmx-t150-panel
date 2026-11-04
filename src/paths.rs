//! Where the panel keeps its files.

use std::path::PathBuf;

/// `~/.config/tmx-t150-panel` (or `$XDG_CONFIG_HOME/...`).
///
/// Older builds wrote to `~/.config/ttrs-wheel`.  That folder is moved once,
/// on first start after the rename, so existing profiles and the theme carry
/// over instead of being silently ignored.
pub fn config_dir() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| String::from("/tmp")))
                .join(".config")
        });
    let new = base.join("tmx-t150-panel");
    let old = base.join("ttrs-wheel");
    if !new.exists() && old.is_dir() {
        let _ = std::fs::create_dir_all(&base);
        if std::fs::rename(&old, &new).is_err() {
            // Different filesystem or permissions: copy the known files.
            if std::fs::create_dir_all(&new).is_ok() {
                for name in ["profiles.conf", "ui.conf", "protondb.cache", "bridge.conf"] {
                    let _ = std::fs::copy(old.join(name), new.join(name));
                }
            }
        }
    }
    new
}
