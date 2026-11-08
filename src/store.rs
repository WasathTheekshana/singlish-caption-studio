//! Per-user settings, glossary and transcript cache, kept under the user's own profile
//! (never next to the program). Every write is atomic; every read is size-limited.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    fonts::{self, Font},
    text::GlossaryData,
    validate::{Lang, Settings},
};

const MAX_FILE_BYTES: u64 = 1_000_000;
pub const MAX_GLOSSARY_ENTRIES: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppConfig {
    #[serde(flatten)]
    pub style: Settings,
    #[serde(default)]
    pub setup_done: bool,
    /// What the user pays per minute of audio, used only to estimate cost on the Usage tab.
    #[serde(default)]
    pub price_per_minute: Option<f64>,
    /// Look for a new version when the app starts. On unless the user turns it off.
    #[serde(default = "default_true")]
    pub auto_update_check: bool,
}

fn default_true() -> bool {
    true
}

fn env_dir(var: &str) -> Result<PathBuf, String> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{var} is not set."))
}

const APP_NAME: &str = "Singlish Caption Studio";
/// The app was called "Caption Studio" before 0.1.0 was released. Early installs keep their settings.
const LEGACY_APP_NAME: &str = "Caption Studio";

/// Move a settings folder from its old name to the new one, once. Never overwrites.
fn migrate_dir(old: &Path, new: &Path) -> bool {
    !new.exists() && old.is_dir() && fs::rename(old, new).is_ok()
}

/// Settings, glossary and usage log: %APPDATA%\Singlish Caption Studio
pub fn data_dir() -> Result<PathBuf, String> {
    let base = env_dir("APPDATA")?;
    let d = base.join(APP_NAME);
    migrate_dir(&base.join(LEGACY_APP_NAME), &d);
    fs::create_dir_all(&d).map_err(|_| "Could not create the settings folder.")?;
    Ok(d)
}

/// Transcript cache: %LOCALAPPDATA%\Singlish Caption Studio Data\cache. It sits next to the
/// install folder, never inside it. Only computes the path; the folder is created on save.
pub fn cache_dir() -> Result<PathBuf, String> {
    Ok(env_dir("LOCALAPPDATA")?
        .join(format!("{APP_NAME} Data"))
        .join("cache"))
}

/// CapCut's draft folder, built from LOCALAPPDATA (never from anything the UI sends).
pub fn drafts_root() -> Result<PathBuf, String> {
    let p = env_dir("LOCALAPPDATA")?
        .join("CapCut")
        .join("User Data")
        .join("Projects")
        .join(crate::draft::DRAFT_DIR_NAME);
    if p.is_dir() {
        Ok(p)
    } else {
        Err("CapCut's project folder was not found. Is CapCut installed?".into())
    }
}

/// Read a text file only if it is no bigger than `max` bytes.
pub fn read_capped(p: &Path, max: u64) -> Option<String> {
    if fs::metadata(p).ok()?.len() > max {
        return None;
    }
    fs::read_to_string(p).ok()
}

fn read_limited(p: &Path) -> Option<String> {
    read_capped(p, MAX_FILE_BYTES)
}

pub fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).map_err(|_| "Could not save.")?;
    fs::rename(&tmp, path).map_err(|_| {
        let _ = fs::remove_file(&tmp);
        "Could not save.".to_string()
    })
}

/// Versions this app has actually been tested against.
const TESTED_PREFIX: &str = "9.6";

pub fn parse_appver(xml: &str) -> Option<String> {
    let rest = xml.split("appver value=\"").nth(1)?;
    let v = rest.split('"').next()?;
    (!v.is_empty() && v.len() <= 20 && v.chars().all(|c| c.is_ascii_digit() || c == '.'))
        .then(|| v.to_string())
}

/// (installed, version, tested) for the CapCut desktop app on this PC.
pub fn capcut_status() -> (bool, Option<String>, bool) {
    let installed = drafts_root().is_ok();
    let version = env_dir("LOCALAPPDATA")
        .ok()
        .and_then(|d| read_limited(&d.join("CapCut").join("Apps").join("ProductInfo.xml")))
        .and_then(|x| parse_appver(&x));
    let tested = version
        .as_deref()
        .is_some_and(|v| v.starts_with(TESTED_PREFIX));
    (installed, version, tested)
}

const SINHALA_PREFERENCE: &[&str] = &[
    "AF Sigiri",
    "Iskoola Pota",
    "Noto Sans Sinhala",
    "Nirmala UI",
];
const ENGLISH_PREFERENCE: &[&str] = &["Arial Black", "Arial", "Segoe UI"];

pub fn default_config(fonts: &[Font]) -> AppConfig {
    let pick = |pref: &[&str]| {
        fonts::first_available(fonts, pref)
            .map(|f| f.name.clone())
            .unwrap_or_else(|| pref[pref.len() - 1].to_string())
    };
    AppConfig {
        style: Settings {
            sinhala: Lang {
                font: pick(SINHALA_PREFERENCE),
                size: 17.0,
                bold: false,
            },
            english: Lang {
                font: pick(ENGLISH_PREFERENCE),
                size: 14.0,
                bold: true,
            },
            color: [1.0, 1.0, 1.0],
            position_y: -0.45,
        },
        setup_done: false,
        price_per_minute: None,
        auto_update_check: true,
    }
}

pub fn load_config(fonts: &[Font]) -> AppConfig {
    data_dir()
        .ok()
        .and_then(|d| read_limited(&d.join("config.json")))
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| default_config(fonts))
}

pub fn save_config(cfg: &AppConfig) -> Result<(), String> {
    let body = serde_json::to_string_pretty(cfg).map_err(|_| "Could not save settings.")?;
    write_atomic(&data_dir()?.join("config.json"), &body)
}

pub fn load_glossary() -> GlossaryData {
    data_dir()
        .ok()
        .and_then(|d| read_limited(&d.join("glossary.json")))
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| {
            let mut g = GlossaryData::default();
            g.english.insert("වීඩියෝ".into(), "Video".into());
            g
        })
}

pub fn check_glossary(g: &GlossaryData) -> Result<(), String> {
    if g.english.len() > MAX_GLOSSARY_ENTRIES || g.spelling.len() > MAX_GLOSSARY_ENTRIES {
        return Err(format!("At most {MAX_GLOSSARY_ENTRIES} entries per list."));
    }
    for (k, v) in g.english.iter().chain(g.spelling.iter()) {
        let bad = |s: &str| {
            s.trim().is_empty() || s.chars().count() > 80 || s.chars().any(char::is_control)
        };
        if bad(k) || bad(v) {
            return Err("Each entry needs text of 1 to 80 characters on both sides.".into());
        }
    }
    Ok(())
}

pub fn save_glossary(g: &GlossaryData) -> Result<(), String> {
    check_glossary(g)?;
    let body = serde_json::to_string_pretty(g).map_err(|_| "Could not save the glossary.")?;
    write_atomic(&data_dir()?.join("glossary.json"), &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_prefer_installed_fonts_and_fall_back() {
        let f = |n: &str| Font {
            name: n.into(),
            path: format!("C:/Fonts/{n}.ttf"),
            collection: false,
        };
        let cfg = default_config(&[f("Iskoola Pota"), f("Arial")]);
        assert_eq!(cfg.style.sinhala.font, "Iskoola Pota");
        assert_eq!(cfg.style.english.font, "Arial");
        let none = default_config(&[]);
        assert_eq!(none.style.sinhala.font, "Nirmala UI");
        assert!(!none.setup_done);
    }

    #[test]
    fn config_round_trips_with_flattened_style() {
        let cfg = default_config(&[]);
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("\"sinhala\"") && json.contains("\"setup_done\""));
        assert_eq!(serde_json::from_str::<AppConfig>(&json).unwrap(), cfg);
    }

    #[test]
    fn update_checks_are_on_unless_turned_off() {
        let old = r#"{"sinhala":{"font":"A","size":17.0},"english":{"font":"B","size":14.0},
                       "color":[1,1,1],"position_y":-0.45}"#;
        assert!(
            serde_json::from_str::<AppConfig>(old)
                .unwrap()
                .auto_update_check
        );
        let mut cfg = default_config(&[]);
        assert!(cfg.auto_update_check);
        cfg.auto_update_check = false;
        let back: AppConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert!(!back.auto_update_check);
    }

    #[test]
    fn an_old_settings_file_without_a_price_still_loads() {
        let json = r#"{"sinhala":{"font":"A","size":17.0},"english":{"font":"B","size":14.0,"bold":true},
                       "color":[1,1,1],"position_y":-0.45,"setup_done":true}"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.price_per_minute, None);
        assert!(cfg.setup_done);
    }

    #[test]
    fn a_settings_folder_with_the_old_name_is_moved_once_and_never_overwritten() {
        let base = std::env::temp_dir().join(format!("scs-migrate-{}", std::process::id()));
        let (old, new) = (
            base.join("Caption Studio"),
            base.join("Singlish Caption Studio"),
        );
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("config.json"), "old").unwrap();
        assert!(migrate_dir(&old, &new));
        assert_eq!(fs::read_to_string(new.join("config.json")).unwrap(), "old");
        assert!(!old.exists());
        // If the new folder already exists, the old one is left alone.
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("config.json"), "stale").unwrap();
        assert!(!migrate_dir(&old, &new));
        assert_eq!(fs::read_to_string(new.join("config.json")).unwrap(), "old");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn parses_capcut_version() {
        let xml = r#"<packet><appver value="9.6.0" /><full_appver value="9.6.0.4097" /></packet>"#;
        assert_eq!(parse_appver(xml).as_deref(), Some("9.6.0"));
        assert_eq!(parse_appver("<x/>"), None);
        assert_eq!(parse_appver(r#"appver value="<script>""#), None);
    }

    #[test]
    fn glossary_limits() {
        let mut g = GlossaryData::default();
        g.english.insert("වීඩියෝ".into(), "Video".into());
        assert!(check_glossary(&g).is_ok());
        g.english.insert("x".into(), "".into());
        assert!(check_glossary(&g).is_err());
        let mut big = GlossaryData::default();
        for i in 0..501 {
            big.english.insert(format!("k{i}"), "v".into());
        }
        assert!(check_glossary(&big).is_err());
    }
}
