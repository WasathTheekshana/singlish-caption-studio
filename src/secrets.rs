//! The ElevenLabs API key lives in Windows Credential Manager, encrypted for the current
//! user. It is never written to a file, never logged and never sent to the UI.

use keyring::Entry;

const SERVICE: &str = "Singlish Caption Studio";
/// Service name used before the app was renamed. A key saved there is moved over on first read.
const LEGACY_SERVICE: &str = "Caption Studio";
const ACCOUNT: &str = "ElevenLabs API key";

fn entry(service: &str) -> Result<Entry, String> {
    Entry::new(service, ACCOUNT)
        .map_err(|_| "Windows Credential Manager is not available.".to_string())
}

fn read(service: &str) -> Result<Option<String>, String> {
    match entry(service)?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err("Could not read the saved key from Windows Credential Manager.".into()),
    }
}

fn remove(service: &str) -> Result<(), String> {
    match entry(service)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("Could not remove the saved key.".into()),
    }
}

/// Look in `service`; if nothing is there, move a key over from `legacy`.
fn read_with_migration(service: &str, legacy: &str) -> Result<Option<String>, String> {
    if let Some(k) = read(service)? {
        return Ok(Some(k));
    }
    let Some(old) = read(legacy)? else {
        return Ok(None);
    };
    entry(service)?
        .set_password(&old)
        .map_err(|_| "Could not move the saved key to its new name.".to_string())?;
    let _ = remove(legacy);
    Ok(Some(old))
}

pub fn get() -> Result<Option<String>, String> {
    read_with_migration(SERVICE, LEGACY_SERVICE)
}

pub fn exists() -> bool {
    matches!(get(), Ok(Some(_)))
}

pub fn set(key: &str) -> Result<(), String> {
    entry(SERVICE)?
        .set_password(key)
        .map_err(|_| "Could not save the key to Windows Credential Manager.".to_string())
}

pub fn delete() -> Result<(), String> {
    remove(SERVICE)?;
    remove(LEGACY_SERVICE)
}

/// Keys are 16 to 128 characters of letters, digits, '_' or '-'. Anything else (spaces, quotes,
/// control characters) is rejected before it can reach a header or the credential store.
pub fn key_format_ok(k: &str) -> bool {
    (16..=128).contains(&k.len())
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_format() {
        assert!(key_format_ok("sk_0123456789abcdef0123456789abcdef"));
        assert!(key_format_ok(&"a".repeat(32)));
        for bad in [
            "",
            "short",
            "has space in it 0123456789",
            "quote\"0123456789abcdef",
            "line\nbreak0123456789ab",
            &"a".repeat(129),
        ] {
            assert!(!key_format_ok(bad), "should reject {bad:?}");
        }
    }

    /// Uses throwaway service names, so a real saved key is never touched.
    #[test]
    fn a_key_saved_under_the_old_name_is_moved_to_the_new_one() {
        let tag = std::process::id();
        let (new, old) = (format!("scs-test-new-{tag}"), format!("scs-test-old-{tag}"));
        entry(&old)
            .unwrap()
            .set_password("secret-value-123456")
            .unwrap();
        assert_eq!(read(&new).unwrap(), None);
        assert_eq!(
            read_with_migration(&new, &old).unwrap().as_deref(),
            Some("secret-value-123456")
        );
        assert_eq!(read(&new).unwrap().as_deref(), Some("secret-value-123456"));
        assert_eq!(read(&old).unwrap(), None, "the old entry should be gone");
        // A second call just reads the new entry.
        assert!(read_with_migration(&new, &old).unwrap().is_some());
        remove(&new).unwrap();
        assert_eq!(read_with_migration(&new, &old).unwrap(), None);
    }
}
