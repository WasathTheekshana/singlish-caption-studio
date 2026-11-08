//! Installed Windows fonts, read from the registry (no shelling out).

use std::path::PathBuf;

use winreg::{enums::*, types::FromRegValue, RegKey};

#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    pub name: String,
    /// Forward-slash path, as CapCut stores it.
    pub path: String,
    /// .ttc/.otc files hold several fonts; CapCut support for them is unverified.
    pub collection: bool,
}

const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts";

fn strip_type_suffix(name: &str) -> String {
    match name.rfind(" (") {
        Some(i) if name.ends_with(')') => name[..i].trim().to_string(),
        _ => name.trim().to_string(),
    }
}

pub fn installed() -> Vec<Font> {
    let fonts_dir = std::env::var_os("SYSTEMROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts");
    let mut out: Vec<Font> = Vec::new();
    for hive in [
        RegKey::predef(HKEY_LOCAL_MACHINE),
        RegKey::predef(HKEY_CURRENT_USER),
    ] {
        let Ok(key) = hive.open_subkey(KEY) else {
            continue;
        };
        for (name, value) in key.enum_values().filter_map(Result::ok) {
            let Ok(file) = String::from_reg_value(&value) else {
                continue;
            };
            let path = PathBuf::from(&file);
            let path = if path.is_absolute() {
                path
            } else {
                fonts_dir.join(path)
            };
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if !matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
                continue;
            }
            let name = strip_type_suffix(&name);
            if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
                continue;
            }
            if !out.iter().any(|f| f.name.eq_ignore_ascii_case(&name)) {
                out.push(Font {
                    name,
                    path: path.to_string_lossy().replace('\\', "/"),
                    collection: ext.ends_with('c'),
                });
            }
        }
    }
    out.sort_by_key(|f| f.name.to_lowercase());
    out
}

pub fn find<'a>(fonts: &'a [Font], name: &str) -> Option<&'a Font> {
    fonts.iter().find(|f| f.name.eq_ignore_ascii_case(name))
}

/// First installed font from a preference list, else None.
pub fn first_available<'a>(fonts: &'a [Font], preferred: &[&str]) -> Option<&'a Font> {
    preferred.iter().find_map(|p| find(fonts, p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_registry_suffix() {
        assert_eq!(strip_type_suffix("Arial Black (TrueType)"), "Arial Black");
        assert_eq!(
            strip_type_suffix("Cambria & Cambria Math (TrueType)"),
            "Cambria & Cambria Math"
        );
        assert_eq!(strip_type_suffix("Plain"), "Plain");
    }

    #[test]
    fn lists_real_fonts_with_usable_paths() {
        let fonts = installed();
        assert!(fonts.len() > 10, "expected installed fonts");
        let arial = find(&fonts, "arial black").expect("Arial Black ships with Windows");
        assert!(arial.path.ends_with(".ttf") && !arial.path.contains('\\') && !arial.collection);
        assert!(std::path::Path::new(&arial.path).is_file());
    }
}
