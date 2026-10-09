//! Small persisted UI preferences (theme, notifications…) in the user's config dir.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Prefs {
    pub light: bool,
    pub notifications: bool,
    pub title_badge: bool,
    pub show_side: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            light: false,
            notifications: true,
            title_badge: true,
            show_side: true,
        }
    }
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("discord-gpui").join("prefs.json"))
}

pub fn load() -> Prefs {
    path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(prefs: &Prefs) {
    if let Some(p) = path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(prefs) {
            let _ = std::fs::write(p, s);
        }
    }
}
