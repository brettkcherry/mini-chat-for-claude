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
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("failed to delete key: {e}")),
    }
}
