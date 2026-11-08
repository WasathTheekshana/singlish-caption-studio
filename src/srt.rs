//! SubRip (.srt) export: the plain caption file that YouTube and Facebook take when you upload.
//!
//! The file is written next to the CapCut write, from the same captions, so the times match the
//! timeline. Nothing here touches the network, and the folder and file name are built in Rust from
//! the project title, never from a path the page sends.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{store, text::Token};

/// Facebook only accepts a caption file whose name ends in a language code, such as `name.si_LK.srt`.
/// YouTube ignores the name, so one file works for both.
const SUFFIX: &str = ".si_LK.srt";
const MAX_TITLE_CHARS: usize = 60;
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// `HH:MM:SS,mmm`
pub fn timestamp(seconds: f64) -> String {
    // `max` also turns NaN into 0, and the cap keeps the hours field at two digits.
    let total_ms = (seconds.max(0.0) * 1000.0).round().min(359_999_999.0) as u64;
    let (h, rest) = (total_ms / 3_600_000, total_ms % 3_600_000);
    let (m, rest) = (rest / 60_000, rest % 60_000);
    format!("{h:02}:{m:02}:{:02},{:03}", rest / 1000, rest % 1000)
}

/// One caption's text on a single clean line. A blank line would end the cue in an SRT file.
fn clean_text(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("-->", "->")
}

/// The whole file. Captions with no text are left out, and the numbering stays continuous.
pub fn render(captions: &[Token]) -> String {
    let mut out = String::new();
    let mut n = 0;
    for c in captions {
        let text = clean_text(&c.text);
        if text.is_empty() {
            continue;
        }
        n += 1;
        let end = c.end.max(c.start + 0.05);
        out.push_str(&format!(
            "{n}\r\n{} --> {}\r\n{text}\r\n\r\n",
            timestamp(c.start),
            timestamp(end)
        ));
    }
    out
}

/// A safe Windows file name for a project title.
pub fn file_name(title: &str) -> String {
    let kept: String = title
        .chars()
        .filter(|c| !c.is_control() && !r#"<>:"/\|?*"#.contains(*c))
        .take(MAX_TITLE_CHARS)
        .collect();
    let mut stem = kept.trim().trim_end_matches('.').trim().to_string();
    if stem.is_empty() {
        stem = "captions".into();
    }
    let first_part = stem.split('.').next().unwrap_or_default().to_uppercase();
    if RESERVED.contains(&first_part.as_str()) {
        stem = format!("captions {stem}");
    }
    format!("{stem}{SUFFIX}")
}

/// Windows keeps the real Documents location (it may be moved to OneDrive) in the registry.
fn documents_dir() -> Result<PathBuf, String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Shell Folders")
        .ok()
        .and_then(|k| k.get_value::<String, _>("Personal").ok())
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|p| PathBuf::from(p).join("Documents"))
                .filter(|p| p.is_dir())
        })
        .ok_or_else(|| "Could not find your Documents folder.".to_string())
}

/// Documents\Singlish Caption Studio\Subtitles
pub fn output_dir() -> Result<PathBuf, String> {
    Ok(documents_dir()?
        .join("Singlish Caption Studio")
        .join("Subtitles"))
}

/// Where the caption file for a project lives.
pub fn path_for(title: &str) -> Result<PathBuf, String> {
    Ok(output_dir()?.join(file_name(title)))
}

/// Write the file for a project and return its name. Replaces the earlier file for that project.
pub fn save(title: &str, captions: &[Token]) -> Result<String, String> {
    let path = path_for(title)?;
    save_to(&path, captions)?;
    Ok(file_name(title))
}

pub fn save_to(path: &Path, captions: &[Token]) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or("Could not find a place to save the caption file.")?;
    fs::create_dir_all(dir).map_err(|_| "Could not create the Subtitles folder.")?;
    store::write_atomic(path, &render(captions))
        .map_err(|_| "Could not save the caption file.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(text: &str, start: f64, end: f64) -> Token {
        Token {
            text: text.into(),
            start,
            end,
        }
    }

    #[test]
    fn timestamps_use_a_comma_and_pad_every_field() {
        assert_eq!(timestamp(0.0), "00:00:00,000");
        assert_eq!(timestamp(1.5), "00:00:01,500");
        assert_eq!(timestamp(61.0), "00:01:01,000");
        assert_eq!(timestamp(3725.042), "01:02:05,042");
        assert_eq!(timestamp(0.9996), "00:00:01,000"); // rounds up across the second
    }

    #[test]
    fn bad_times_never_break_the_format() {
        assert_eq!(timestamp(-3.0), "00:00:00,000");
        assert_eq!(timestamp(f64::NAN), "00:00:00,000");
        assert_eq!(timestamp(f64::INFINITY), "99:59:59,999");
    }

    #[test]
    fn a_file_has_numbered_cues_with_windows_line_endings() {
        let s = render(&[tok("ඔයාට new phone", 0.0, 1.25), tok("එකක් බලමු", 1.3, 2.0)]);
        assert_eq!(
            s,
            "1\r\n00:00:00,000 --> 00:00:01,250\r\nඔයාට new phone\r\n\r\n\
             2\r\n00:00:01,300 --> 00:00:02,000\r\nඑකක් බලමු\r\n\r\n"
        );
    }

    #[test]
    fn empty_captions_are_skipped_and_numbering_stays_continuous() {
        let s = render(&[
            tok("one", 0.0, 1.0),
            tok("  \n ", 1.0, 2.0),
            tok("two", 2.0, 3.0),
        ]);
        assert!(s.contains("2\r\n00:00:02,000"));
        assert!(!s.contains("\r\n3\r\n"));
    }

    #[test]
    fn text_cannot_end_a_cue_or_forge_a_time_line() {
        let s = render(&[tok(
            "line one\r\n\r\n2\r\n00:00:09,000 --> 00:00:10,000\nfake",
            0.0,
            1.0,
        )]);
        // One cue only: no blank line inside it, and no arrow in the text.
        assert_eq!(s.matches("\r\n\r\n").count(), 1);
        assert_eq!(s.matches("-->").count(), 1);
        assert!(s.contains("line one 2 00:00:09,000 -> 00:00:10,000 fake"));
    }

    #[test]
    fn a_cue_always_ends_after_it_starts() {
        let s = render(&[tok("x", 2.0, 2.0)]);
        assert!(s.contains("00:00:02,000 --> 00:00:02,050"));
    }

    #[test]
    fn file_names_are_safe_and_carry_the_language_code() {
        assert_eq!(file_name("My trip"), "My trip.si_LK.srt");
        assert_eq!(file_name("a/b\\c:d*e?f\"g<h>i|j"), "abcdefghij.si_LK.srt");
        assert_eq!(file_name("  ...  "), "captions.si_LK.srt");
        assert_eq!(file_name("clip. "), "clip.si_LK.srt");
        assert_eq!(file_name("අද new phone"), "අද new phone.si_LK.srt");
    }

    #[test]
    fn reserved_windows_names_and_long_titles_are_handled() {
        assert_eq!(file_name("con"), "captions con.si_LK.srt");
        assert_eq!(file_name("NUL.txt"), "captions NUL.txt.si_LK.srt");
        let long = file_name(&"a".repeat(200));
        assert_eq!(long.chars().count(), MAX_TITLE_CHARS + SUFFIX.len());
        assert!(!file_name("a\u{0}b\nc").contains(['\u{0}', '\n']));
    }

    #[test]
    fn saving_writes_the_file_and_replaces_an_older_one() {
        let dir = std::env::temp_dir().join(format!("scs-srt-{}", std::process::id()));
        let path = dir.join("Subtitles").join("p.si_LK.srt");
        save_to(&path, &[tok("first", 0.0, 1.0)]).unwrap();
        save_to(&path, &[tok("second", 0.0, 1.0)]).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("second") && !text.contains("first"));
        let _ = fs::remove_dir_all(&dir);
    }
}
