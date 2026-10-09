//! Token persistence in the OS credential store (Windows Credential Manager,
//! macOS Keychain, Linux keyutils).

const SERVICE: &str = "discord-gpui";
const ACCOUNT: &str = "token";

fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new(SERVICE, ACCOUNT).ok()
}

pub fn load() -> Option<String> {
    entry()?.get_password().ok().filter(|t| !t.is_empty())
}

pub fn save(token: &str) {
    if let Some(e) = entry() {
        let _ = e.set_password(token);
    }
}

pub fn clear() {
    if let Some(e) = entry() {
        let _ = e.delete_credential();
    }
}
