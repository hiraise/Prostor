//! Encrypted setting values (NTF-004): secret settings live in the database as ciphertext, sealed
//! with a key from the environment, so a leaked database holds nothing usable (the D-023 rule).
//! AES-256-GCM: a random 12-byte nonce per value, the setting key as extra authenticated data, so
//! a copied row neither decrypts under another key nor passes as another setting.

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// The size of the sealing key, in bytes.
const KEY_LEN: usize = 32;
/// The size of the nonce, in bytes.
const NONCE_LEN: usize = 12;

/// The key secret settings are sealed with: `PROSTOR_SECRET_KEY`, 32 bytes base64-encoded.
#[derive(Clone, PartialEq, Eq)]
pub struct Key([u8; KEY_LEN]);

impl Key {
    /// Parses a base64-encoded 32-byte key.
    pub fn from_base64(raw: &str) -> Result<Self, String> {
        let bytes = STANDARD
            .decode(raw.trim())
            .map_err(|error| format!("not base64: {error}"))?;
        let bytes: [u8; KEY_LEN] = bytes
            .try_into()
            .map_err(|bytes: Vec<u8>| format!("must hold {KEY_LEN} bytes, has {}", bytes.len()))?;
        Ok(Self(bytes))
    }

    /// The raw key, for configuration that passes it on.
    pub fn into_bytes(self) -> [u8; KEY_LEN] {
        self.0
    }

    /// Seals `value` of the setting `key`: nonce, then ciphertext with its tag. An empty answer
    /// means the entropy source failed; the caller reports that as a failed save.
    pub fn seal(&self, setting: &str, value: &str) -> Vec<u8> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        if getrandom::fill(&mut nonce_bytes).is_err() {
            return Vec::new();
        }
        let cipher = Aes256Gcm::new((&self.0).into());
        cipher
            .encrypt(
                (&nonce_bytes).into(),
                Payload {
                    msg: value.as_bytes(),
                    aad: setting.as_bytes(),
                },
            )
            .map(|sealed| {
                let mut out = Vec::with_capacity(NONCE_LEN + sealed.len());
                out.extend_from_slice(&nonce_bytes);
                out.extend_from_slice(&sealed);
                out
            })
            .unwrap_or_default()
    }

    /// Opens a sealed value; `None` when it does not belong to this key and setting.
    pub fn open(&self, setting: &str, sealed: &[u8]) -> Option<String> {
        if sealed.len() <= NONCE_LEN {
            return None;
        }
        let (nonce, sealed) = sealed.split_at(NONCE_LEN);
        let plain = Aes256Gcm::new((&self.0).into())
            .decrypt(
                nonce.into(),
                Payload {
                    msg: sealed,
                    aad: setting.as_bytes(),
                },
            )
            .ok()?;
        String::from_utf8(plain).ok()
    }
}

impl From<[u8; KEY_LEN]> for Key {
    fn from(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> Key {
        Key::from_base64("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=").unwrap()
    }

    #[test]
    fn a_key_is_32_base64_bytes() {
        assert!(Key::from_base64("not base64 !!").is_err());
        let short = STANDARD.encode([0u8; 16]);
        assert!(Key::from_base64(&short).is_err());
        assert!(Key::from_base64(" AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8= ").is_ok());
        assert_eq!(format!("{:?}", key()), "Key(***)");
    }

    #[test]
    fn sealed_values_open_only_with_their_key_and_setting() {
        let key = key();
        let sealed = key.seal("smtp_password", "hunter2");
        assert!(!sealed.is_empty());
        assert_eq!(
            key.open("smtp_password", &sealed).as_deref(),
            Some("hunter2")
        );

        let other = Key::from_base64(&STANDARD.encode([1u8; 32])).unwrap();
        assert_eq!(other.open("smtp_password", &sealed), None);
        // The setting key is authenticated: a row copied to another setting does not open.
        assert_eq!(key.open("smtp_url", &sealed), None);
        assert_eq!(key.open("smtp_password", b"too short"), None);
    }

    #[test]
    fn sealing_spends_fresh_nonces() {
        let key = key();
        assert_ne!(key.seal("smtp_url", "a"), key.seal("smtp_url", "a"));
    }
}
