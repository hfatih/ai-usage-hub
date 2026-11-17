//! Encrypts account sign-in tokens at rest.
//!
//! On Windows the bytes are protected with DPAPI; on macOS they are sealed with
//! ChaCha20-Poly1305 under a random key kept in the login Keychain. Either way the key belongs to
//! the current OS user, so a copied database cannot be read under another user or on another
//! computer. It does not protect against programs already running as the same user. Stored values
//! carry a `dpapi:` or `keychain:` prefix followed by hex; anything without a prefix is treated as
//! legacy plain text and returned as is.

#[cfg(windows)]
const PREFIX: &str = "dpapi:";
#[cfg(not(windows))]
const PREFIX: &str = "keychain:";

/// Every prefix any platform writes. A value with one of these is never re-encrypted.
pub const PROTECTED_PREFIXES: [&str; 2] = ["dpapi:", "keychain:"];

/// Why a protected value could not be opened.
#[cfg_attr(windows, allow(dead_code))]
enum Unprotect {
    /// The key is temporarily out of reach (locked Keychain); the value may open later.
    KeyUnavailable,
    /// Wrong key or damaged data: the value will never open on this account.
    Invalid,
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};

    #[repr(C)]
    struct DataBlob {
        len: u32,
        data: *mut u8,
    }

    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            input: *const DataBlob,
            description: *const u16,
            entropy: *const DataBlob,
            reserved: *const c_void,
            prompt: *const c_void,
            flags: u32,
            output: *mut DataBlob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const DataBlob,
            description: *mut *mut u16,
            entropy: *const DataBlob,
            reserved: *const c_void,
            prompt: *const c_void,
            flags: u32,
            output: *mut DataBlob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    /// Never show a Windows prompt; fail instead.
    const UI_FORBIDDEN: u32 = 0x1;

    pub fn protect(bytes: &[u8]) -> Option<Vec<u8>> {
        transform(bytes, true)
    }

    pub fn unprotect(bytes: &[u8]) -> Result<Vec<u8>, super::Unprotect> {
        transform(bytes, false).ok_or(super::Unprotect::Invalid)
    }

    fn transform(bytes: &[u8], protect: bool) -> Option<Vec<u8>> {
        let input = DataBlob { len: u32::try_from(bytes.len()).ok()?, data: bytes.as_ptr().cast_mut() };
        let mut output = DataBlob { len: 0, data: null_mut() };
        // SAFETY: `input` points at `bytes`, which outlives the call and is only read. On success
        // Windows fills `output` with a buffer it allocated; it is copied and then released with
        // LocalFree exactly once.
        unsafe {
            let ok = if protect {
                CryptProtectData(&input, null(), null(), null(), null(), UI_FORBIDDEN, &mut output)
            } else {
                CryptUnprotectData(&input, null_mut(), null(), null(), null(), UI_FORBIDDEN, &mut output)
            };
            if ok == 0 || output.data.is_null() {
                return None;
            }
            let result = std::slice::from_raw_parts(output.data, output.len as usize).to_vec();
            LocalFree(output.data.cast());
            Some(result)
        }
    }
}

/// Seals values with a key that lives only in the macOS login Keychain.
#[cfg(not(windows))]
mod platform {
    use chacha20poly1305::{
        aead::{Aead, AeadCore, KeyInit, OsRng},
        ChaCha20Poly1305, Key, Nonce,
    };

    const NONCE_LEN: usize = 12;

    pub fn seal(key: &[u8; 32], plain: &[u8]) -> Option<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
        let nonce = ChaCha20Poly1305::generate_nonce(&mut OsRng);
        let mut sealed = nonce.to_vec();
        sealed.extend(cipher.encrypt(&nonce, plain).ok()?);
        Some(sealed)
    }

    pub fn open(key: &[u8; 32], sealed: &[u8]) -> Option<Vec<u8>> {
        if sealed.len() < NONCE_LEN {
            return None;
        }
        let (nonce, body) = sealed.split_at(NONCE_LEN);
        ChaCha20Poly1305::new(Key::from_slice(key)).decrypt(Nonce::from_slice(nonce), body).ok()
    }

    pub fn protect(bytes: &[u8]) -> Option<Vec<u8>> {
        seal(&key::get()?, bytes)
    }

    pub fn unprotect(bytes: &[u8]) -> Result<Vec<u8>, super::Unprotect> {
        let key = key::get().ok_or(super::Unprotect::KeyUnavailable)?;
        open(&key, bytes).ok_or(super::Unprotect::Invalid)
    }

    #[cfg(target_os = "macos")]
    mod key {
        use std::{
            sync::Mutex,
            time::{Duration, Instant},
        };

        use chacha20poly1305::{aead::OsRng, ChaCha20Poly1305, KeyInit};

        use crate::keychain;

        const SERVICE: &str = "com.aiusagehub.desktop";
        const ACCOUNT: &str = "token-encryption-key";
        /// After a failed Keychain read, wait this long before asking again instead of starting
        /// `security` for every token the database returns.
        const RETRY_AFTER: Duration = Duration::from_secs(60);

        static STATE: Mutex<(Option<[u8; 32]>, Option<Instant>)> = Mutex::new((None, None));

        pub fn get() -> Option<[u8; 32]> {
            if cfg!(test) {
                return Some([0x42; 32]); // Tests never touch the real Keychain.
            }
            let mut state = STATE.lock().ok()?;
            if let Some(key) = state.0 {
                return Some(key);
            }
            if state.1.is_some_and(|failed| failed.elapsed() < RETRY_AFTER) {
                return None;
            }
            match load_or_create() {
                Ok(key) => {
                    *state = (Some(key), None);
                    Some(key)
                }
                Err(error) => {
                    tracing::warn!(%error, "token encryption key is unavailable");
                    state.1 = Some(Instant::now());
                    None
                }
            }
        }

        fn load_or_create() -> Result<[u8; 32], String> {
            // A new key is made only when the item is definitely missing: replacing a key that
            // merely failed to load would make every saved token unreadable.
            if let Some(stored) = keychain::read(SERVICE, ACCOUNT)? {
                return keychain::hex_decode(&stored)
                    .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
                    .ok_or_else(|| "Keychain holds a malformed token key".to_owned());
            }
            let key: [u8; 32] = ChaCha20Poly1305::generate_key(&mut OsRng).into();
            keychain::write(SERVICE, ACCOUNT, &keychain::hex_encode(&key))?;
            Ok(key)
        }
    }

    /// No system key store is wired up on this OS yet, so tokens stay as they were.
    #[cfg(not(target_os = "macos"))]
    mod key {
        pub fn get() -> Option<[u8; 32]> {
            None
        }
    }
}

/// Returns the value to store for `secret`. Empty and already protected values pass through;
/// if the OS refuses to encrypt, the plain value is kept rather than losing the sign-in.
pub fn protect(secret: &str) -> String {
    if secret.is_empty() || PROTECTED_PREFIXES.iter().any(|prefix| secret.starts_with(prefix)) {
        return secret.to_owned();
    }
    match platform::protect(secret.as_bytes()) {
        Some(bytes) => format!("{PREFIX}{}", crate::keychain::hex_encode(&bytes)),
        None => secret.to_owned(),
    }
}

/// Returns the usable secret for a stored value. A protected value that cannot be decrypted
/// (database moved to another user or computer) yields an empty string: the account then simply
/// has no saved sign-in and must be signed in again. While the key is only temporarily out of
/// reach the stored value is handed back unchanged, so saving the account again keeps it intact.
pub fn reveal(stored: &str) -> String {
    let Some(encoded) = stored.strip_prefix(PREFIX) else {
        if PROTECTED_PREFIXES.iter().any(|prefix| stored.starts_with(prefix)) {
            return String::new(); // Written on another OS; its key is not here.
        }
        return stored.to_owned();
    };
    let Some(bytes) = crate::keychain::hex_decode(encoded) else {
        return String::new();
    };
    match platform::unprotect(&bytes) {
        Ok(bytes) => String::from_utf8(bytes).unwrap_or_default(),
        Err(Unprotect::KeyUnavailable) => stored.to_owned(),
        Err(Unprotect::Invalid) => String::new(),
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn secrets_round_trip_and_never_store_plain_text() {
        let stored = protect("ya29.sample-token");
        assert!(stored.starts_with(PREFIX));
        assert!(!stored.contains("sample-token"));
        assert_eq!(reveal(&stored), "ya29.sample-token");
        // Idempotent, and legacy plain text still reads.
        assert_eq!(protect(&stored), stored);
        assert_eq!(reveal("legacy-plain"), "legacy-plain");
        assert_eq!(protect(""), "");
        assert_eq!(reveal("dpapi:zz"), "");
    }
}

#[cfg(all(test, not(windows)))]
mod sealed_tests {
    use super::platform::{open, seal};

    #[test]
    fn sealed_values_open_only_with_their_key() {
        let key = [7u8; 32];
        let sealed = seal(&key, b"ya29.sample-token").unwrap();
        assert!(!sealed.windows(6).any(|window| window == b"sample"));
        assert_eq!(open(&key, &sealed).unwrap(), b"ya29.sample-token");
        assert_ne!(seal(&key, b"ya29.sample-token").unwrap(), sealed, "each seal uses a fresh nonce");
        assert!(open(&[8u8; 32], &sealed).is_none());
        assert!(open(&key, &sealed[..5]).is_none());
    }

    #[test]
    fn foreign_and_damaged_values_reveal_nothing() {
        assert_eq!(super::reveal("dpapi:00ff"), "");
        assert_eq!(super::reveal("keychain:zz"), "");
        assert_eq!(super::reveal("legacy-plain"), "legacy-plain");
        assert_eq!(super::protect("dpapi:00ff"), "dpapi:00ff");
    }
}
