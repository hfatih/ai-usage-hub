//! Encrypts account sign-in tokens at rest with Windows DPAPI.
//!
//! The key belongs to the current Windows user, so a copied database file cannot be read under
//! another user or on another PC. It does not protect against programs already running as the
//! same user. Stored values carry a `dpapi:` prefix followed by hex; anything without the prefix
//! is treated as legacy plain text and returned as is.

const PREFIX: &str = "dpapi:";

#[cfg(windows)]
mod dpapi {
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

    pub fn transform(bytes: &[u8], protect: bool) -> Option<Vec<u8>> {
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

#[cfg(not(windows))]
mod dpapi {
    pub fn transform(_bytes: &[u8], _protect: bool) -> Option<Vec<u8>> {
        None
    }
}

/// Returns the value to store for `secret`. Empty and already protected values pass through;
/// if Windows refuses to encrypt, the plain value is kept rather than losing the sign-in.
pub fn protect(secret: &str) -> String {
    if secret.is_empty() || secret.starts_with(PREFIX) {
        return secret.to_owned();
    }
    match dpapi::transform(secret.as_bytes(), true) {
        Some(bytes) => format!("{PREFIX}{}", bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()),
        None => secret.to_owned(),
    }
}

/// Returns the usable secret for a stored value. A protected value that cannot be decrypted
/// (database moved to another Windows user) yields an empty string: the account then simply
/// has no saved sign-in and must be signed in again.
pub fn reveal(stored: &str) -> String {
    let Some(encoded) = stored.strip_prefix(PREFIX) else {
        return stored.to_owned();
    };
    let bytes: Option<Vec<u8>> = (0..encoded.len())
        .step_by(2)
        .map(|index| encoded.get(index..index + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok()))
        .collect();
    bytes
        .and_then(|bytes| dpapi::transform(&bytes, false))
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default()
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
