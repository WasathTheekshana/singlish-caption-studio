//! In-app updates.
//!
//! The page never talks to the updater plugin. It can only ask the commands in `main.rs`, which
//! use this module. An update is installed only when its signature matches the public key built
//! into the app (`tauri.conf.json`) and names the exact version being offered, so a tampered
//! manifest cannot push an older or different build.

use std::sync::Mutex;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::{Error, Update, UpdaterExt};

const MAX_NOTES_CHARS: usize = 4000;

/// The update found by the last check, kept until the user chooses to install it.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

/// Version text is shown to people, so only plain version characters are accepted.
pub fn version_ok(v: &str) -> bool {
    (1..=40).contains(&v.len())
        && v.starts_with(|c: char| c.is_ascii_digit())
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

/// Release notes come from the update manifest, so treat them as plain, bounded text.
pub fn clean_notes(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(MAX_NOTES_CHARS)
        .collect()
}

fn explain_check(e: &Error) -> String {
    match e {
        Error::ReleaseNotFound => "No update information has been published yet.".into(),
        Error::Reqwest(_) | Error::Network(_) => {
            "Could not reach GitHub to look for updates. Check your internet connection.".into()
        }
        _ => "Could not check for updates right now. Try again later.".into(),
    }
}

fn explain_install(e: &Error) -> String {
    match e {
        Error::Minisign(_)
        | Error::Base64(_)
        | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. }
        | Error::MissingSignedVersion => {
            "The update could not be verified, so it was not installed.".into()
        }
        Error::Reqwest(_) | Error::Network(_) => {
            "The download did not finish. Check your internet connection and try again.".into()
        }
        _ => "The update could not be installed. Your current version was not changed.".into(),
    }
}

/// Look for a newer release. Returns what the page needs to show, and remembers the update.
pub async fn check(app: &AppHandle, pending: &Pending) -> Result<Value, String> {
    let current = app.package_info().version.to_string();
    let updater = app
        .updater()
        .map_err(|_| "Updates are not available in this build.".to_string())?;
    let found = updater.check().await.map_err(|e| explain_check(&e))?;
    let mut slot = pending
        .0
        .lock()
        .map_err(|_| "Update state is unavailable.".to_string())?;
    match found {
        Some(u) if version_ok(&u.version) => {
            let info = json!({
                "available": true,
                "current": current,
                "version": u.version,
                "notes": clean_notes(u.body.as_deref().unwrap_or("")),
            });
            *slot = Some(u);
            Ok(info)
        }
        Some(_) => {
            *slot = None;
            Err("The update information looked wrong, so it was ignored.".into())
        }
        None => {
            *slot = None;
            Ok(json!({"available": false, "current": current}))
        }
    }
}

/// Download the update found by `check`, verify it, and run its installer. On Windows the app
/// exits when the installer starts, and the installer starts the new version again.
pub async fn install(app: &AppHandle, pending: &Pending) -> Result<(), String> {
    let update = pending
        .0
        .lock()
        .map_err(|_| "Update state is unavailable.".to_string())?
        .clone()
        .ok_or("There is no update to install. Check for updates first.")?;

    let progress = app.clone();
    let finished = app.clone();
    let mut downloaded: u64 = 0;
    let mut last_percent: Option<u64> = None;
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                let percent = total
                    .filter(|t| *t > 0)
                    .map(|t| (downloaded.saturating_mul(100) / t).min(100));
                if percent != last_percent {
                    last_percent = percent;
                    let _ = progress.emit(
                        "update-event",
                        json!({"event": "progress", "percent": percent}),
                    );
                }
            },
            move || {
                let _ = finished.emit("update-event", json!({"event": "installing"}));
            },
        )
        .await
        .map_err(|e| explain_install(&e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_must_look_like_versions() {
        for ok in ["0.2.0", "1.0.0", "10.20.30", "1.0.0-rc.1", "2.1.0+build5"] {
            assert!(version_ok(ok), "{ok}");
        }
        for bad in [
            "",
            "v1.0.0",
            "latest",
            "1.0.0 <script>",
            "1.0.0; calc",
            "1.0\n.0",
            "../1.0.0",
            &"1".repeat(41),
        ] {
            assert!(!version_ok(bad), "{bad:?}");
        }
    }

    #[test]
    fn notes_are_plain_bounded_text() {
        assert_eq!(
            clean_notes("Fix: captions\nFeature: usage"),
            "Fix: captions\nFeature: usage"
        );
        assert_eq!(clean_notes("a\u{0}b\u{7}c\rd"), "abcd");
        assert_eq!(clean_notes("ඔයාට new phone එකක්"), "ඔයාට new phone එකක්");
        assert_eq!(
            clean_notes(&"x".repeat(10_000)).chars().count(),
            MAX_NOTES_CHARS
        );
        assert_eq!(clean_notes(""), "");
    }

    #[test]
    fn error_messages_are_friendly_and_leak_nothing() {
        let all = [
            explain_check(&Error::ReleaseNotFound),
            explain_check(&Error::Network("https://secret.example/x?token=1".into())),
            explain_check(&Error::EmptyEndpoints),
            explain_install(&Error::MissingSignedVersion),
            explain_install(&Error::SignedVersionMismatch {
                signed: "0.1.0".into(),
                announced: "9.9.9".into(),
            }),
            explain_install(&Error::Network("https://secret.example/x?token=1".into())),
            explain_install(&Error::TempDirNotFound),
        ];
        for m in all {
            assert!(
                !m.contains("secret.example") && !m.contains("token") && !m.contains("9.9.9"),
                "{m}"
            );
            assert!(m.ends_with('.'), "{m}");
        }
        assert!(explain_install(&Error::MissingSignedVersion).contains("could not be verified"));
    }

    /// The security-relevant settings live in tauri.conf.json, so pin them with a test.
    #[test]
    fn the_update_settings_are_locked_down() {
        let conf: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let u = &conf["plugins"]["updater"];
        let endpoints = u["endpoints"].as_array().unwrap();
        assert_eq!(endpoints.len(), 1, "exactly one update address");
        let url = endpoints[0].as_str().unwrap();
        assert!(
            url.starts_with(
                "https://github.com/WasathTheekshana/singlish-caption-studio/releases/"
            ),
            "{url}"
        );
        assert!(
            u.get("dangerousInsecureTransportProtocol").is_none(),
            "plain http must never be allowed"
        );
        assert_eq!(
            u["requireSignedVersion"], true,
            "the signature must name the version"
        );
        // A minisign public key, base64 encoded, begins with "untrusted comment".
        assert!(u["pubkey"]
            .as_str()
            .unwrap()
            .starts_with("dW50cnVzdGVkIGNvbW1lbnQ6"));
        assert_eq!(conf["bundle"]["createUpdaterArtifacts"], true);
    }

    /// The page may only use our own commands, never the plugin's.
    #[test]
    fn the_page_is_not_given_the_updater_plugin() {
        let caps: Value = serde_json::from_str(include_str!("../capabilities/main.json")).unwrap();
        let perms: Vec<&str> = caps["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(
            !perms.iter().any(|p| p.starts_with("updater:")),
            "{perms:?}"
        );
        for needed in [
            "allow-check-update",
            "allow-install-update",
            "allow-get-update-settings",
            "allow-set-auto-update-check",
        ] {
            assert!(perms.contains(&needed), "missing {needed}");
        }
    }
}
