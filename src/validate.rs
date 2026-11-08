//! Validation of everything the UI sends us. Nothing here touches the filesystem.

use serde::{Deserialize, Serialize};

const MAX_NAME_CHARS: usize = 80;

/// A project is addressed by its CapCut folder name only, never by a path from the UI.
pub fn project_name_ok(name: &str) -> bool {
    let n = name.chars().count();
    n > 0
        && n <= MAX_NAME_CHARS
        && !name.starts_with('.')
        && !name.ends_with(['.', ' '])
        && !name.contains("..")
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lang {
    pub font: String,
    pub size: f64,
    #[serde(default)]
    pub bold: bool,
}

pub const MIN_WORDS_PER_CAPTION: usize = 2;
pub const MAX_WORDS_PER_CAPTION: usize = 10;
pub const DEFAULT_WORDS_PER_CAPTION: usize = 4;

fn default_words() -> usize {
    DEFAULT_WORDS_PER_CAPTION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub sinhala: Lang,
    pub english: Lang,
    pub color: [f64; 3],
    pub position_y: f64,
    /// The most words in one caption box. Longer sentences are split at natural pauses.
    #[serde(default = "default_words")]
    pub max_words: usize,
}

fn in_range(v: f64, lo: f64, hi: f64) -> bool {
    v.is_finite() && v >= lo && v <= hi
}

/// Fonts must be ones Windows reports as installed; numbers must be sane.
pub fn check_settings(s: &Settings, installed_fonts: &[String]) -> Result<(), String> {
    for (label, lang) in [("Sinhala", &s.sinhala), ("English", &s.english)] {
        if !installed_fonts
            .iter()
            .any(|f| f.eq_ignore_ascii_case(&lang.font))
        {
            return Err(format!("{label} font is not installed."));
        }
        if !in_range(lang.size, 6.0, 80.0) {
            return Err(format!("{label} size must be between 6 and 80."));
        }
    }
    if !s.color.iter().all(|c| in_range(*c, 0.0, 1.0)) {
        return Err("Color values must be between 0 and 1.".into());
    }
    if !in_range(s.position_y, -1.0, 1.0) {
        return Err("Caption height must be between -1 and 1.".into());
    }
    if !(MIN_WORDS_PER_CAPTION..=MAX_WORDS_PER_CAPTION).contains(&s.max_words) {
        return Err(format!(
            "Words per caption must be between {MIN_WORDS_PER_CAPTION} and {MAX_WORDS_PER_CAPTION}."
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        Settings {
            sinhala: Lang {
                font: "AF Sigiri".into(),
                size: 17.0,
                bold: false,
            },
            english: Lang {
                font: "Arial Black".into(),
                size: 14.0,
                bold: true,
            },
            color: [1.0, 1.0, 1.0],
            position_y: -0.45,
            max_words: 4,
        }
    }
    fn fonts() -> Vec<String> {
        vec!["AF Sigiri".into(), "Arial Black".into()]
    }

    #[test]
    fn names_accept_normal_and_sinhala() {
        assert!(project_name_ok("captiontest2"));
        assert!(project_name_ok("1107 (1)"));
        assert!(project_name_ok("ඔයාට වීඩියෝ"));
    }

    #[test]
    fn names_reject_traversal_and_junk() {
        for bad in [
            "",
            ".",
            "..",
            "..\\x",
            "a/b",
            "a\\b",
            "C:\\Windows",
            "a:b",
            ".hidden",
            "x..y",
            "name.",
            "a|b",
            "a\nb",
        ] {
            assert!(!project_name_ok(bad), "should reject {bad:?}");
        }
        assert!(!project_name_ok(&"a".repeat(81)));
    }

    #[test]
    fn settings_ok_and_bad() {
        assert!(check_settings(&settings(), &fonts()).is_ok());
        let mut s = settings();
        s.sinhala.font = "Evil Font".into();
        assert!(check_settings(&s, &fonts()).is_err());
        let mut s = settings();
        s.english.size = 500.0;
        assert!(check_settings(&s, &fonts()).is_err());
        let mut s = settings();
        s.english.size = f64::NAN;
        assert!(check_settings(&s, &fonts()).is_err());
        let mut s = settings();
        s.color = [2.0, 0.0, 0.0];
        assert!(check_settings(&s, &fonts()).is_err());
        let mut s = settings();
        s.position_y = f64::INFINITY;
        assert!(check_settings(&s, &fonts()).is_err());
        for bad in [0, 1, 11, 500] {
            let mut s = settings();
            s.max_words = bad;
            assert!(
                check_settings(&s, &fonts()).is_err(),
                "{bad} words should be refused"
            );
        }
        for good in [2, 4, 10] {
            let mut s = settings();
            s.max_words = good;
            assert!(check_settings(&s, &fonts()).is_ok());
        }
    }

    #[test]
    fn font_match_is_case_insensitive() {
        let mut s = settings();
        s.sinhala.font = "af sigiri".into();
        assert!(check_settings(&s, &fonts()).is_ok());
    }
}
