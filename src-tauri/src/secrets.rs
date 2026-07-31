// API key storage, backed by the OS credential store.
// Windows: Credential Manager. macOS: Keychain. Linux (v0.2): secret-service.
//
// One tiny interface so the rest of the app never touches `keyring` directly.

use keyring::Entry;

const SERVICE: &str = "claude-mini";
const ACCOUNT: &str = "anthropic-api-key";

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("credential store unavailable: {e}"))
}

/// Validate + persist the key. Catches the classic paste mistakes before
/// they become confusing 401s (we learned this the hard way — see PLAN.md).
pub fn save(key: &str) -> Result<(), String> {
    let k = key.trim();

    if k.is_empty() {
        return Err("Key is empty.".into());
    }
    if k.starts_with("sk-ant-sk-ant-") {
        return Err(
            "That key has a doubled 'sk-ant-' prefix — paste just the key itself.".into(),
        );
    }
    if !k.starts_with("sk-ant-") {
        return Err("That doesn't look like an Anthropic API key (should start with 'sk-ant-').".into());
    }
    if k.len() < 40 {
        return Err("That key looks too short — did the paste get cut off?".into());
    }

    entry()?
        .set_password(k)
        .map_err(|e| format!("failed to save key: {e}"))
}

/// None if no key has been stored (or the store is unreachable).
pub fn load() -> Option<String> {
    entry().ok()?.get_password().ok()
}

pub fn delete() -> Result<(), String> {
    delete_entry(&entry()?)
}

/// Split out from `delete` so the uninstall path's exact semantics can be
/// tested against a scratch credential — deleting the real one to prove it
/// works is not an option.
///
/// `NoEntry` maps to `Ok`: deleting a key that was never saved is a success,
/// not a failure. The uninstall hook relies on this for users who never got
/// as far as entering a key.
fn delete_entry(entry: &Entry) -> Result<(), String> {
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("failed to delete key: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the real OS credential store, but only ever under a scratch
    /// service name — never SERVICE/ACCOUNT, which hold the user's live key.
    #[test]
    fn delete_entry_removes_credential_and_is_idempotent() {
        let scratch = Entry::new("claude-mini-test-scratch", "scratch-account")
            .expect("scratch credential store unavailable");

        scratch
            .set_password("sk-ant-scratch-value-for-tests-only")
            .expect("could not write scratch credential");
        assert!(
            scratch.get_password().is_ok(),
            "scratch credential should exist before deletion"
        );

        delete_entry(&scratch).expect("delete should succeed");
        assert!(
            matches!(scratch.get_password(), Err(keyring::Error::NoEntry)),
            "credential should be gone after delete"
        );

        // The uninstall hook fires whether or not a key was ever saved, so a
        // second delete has to be a no-op rather than an error.
        delete_entry(&scratch).expect("deleting a missing credential must be Ok");
    }
}
