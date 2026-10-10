//! Small persisted UI preferences (theme, density, notifications…) in the user's config dir.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// "light" | "ash" | "dark" | "onyx".
    pub theme: String,
    /// "compact" | "default" | "spacious".
    pub density: String,
    pub sidebar_width: f32,
    pub notifications: bool,
    pub title_badge: bool,
    pub show_side: bool,
    /// Legacy field from earlier versions (light theme toggle).
    pub light: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            density: "default".into(),
            sidebar_width: 240.,
            notifications: true,
            title_badge: true,
            show_side: true,
            light: false,
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
    let mut p: Prefs = path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    if p.light {
        p.theme = "light".into();
        p.light = false;
    }
    p
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
