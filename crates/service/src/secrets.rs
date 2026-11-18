//! Random secrets: session cookies, API tokens, setup codes and generated passwords. Only their
//! SHA-256 is stored, so a leaked database does not leak a usable session or token (D-023).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use prostor_domain::auth::codes;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// Prefix of personal API tokens: tells people and secret scanners what the string is.
pub const TOKEN_PREFIX: &str = "tfp_";

/// A new secret and the hash to store.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret {
    /// Shown to its owner once; never stored or logged.
    pub value: String,
    pub hash: Vec<u8>,
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// `N` bytes from the operating system's secure random source.
pub fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes)
        .map_err(|error| Error::Internal(format!("no secure random source: {error}")))?;
    Ok(bytes)
}

/// SHA-256 of a session cookie or token as sent by the client.
pub fn hash(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}

/// A session cookie value: 256 random bits.
pub fn session() -> Result<Secret> {
    let value = URL_SAFE_NO_PAD.encode(random_bytes::<32>()?);
    Ok(Secret {
        hash: hash(&value),
        value,
    })
}

/// A personal API token: the prefix and 256 random bits.
pub fn token() -> Result<Secret> {
    let value = format!(
        "{TOKEN_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(random_bytes::<32>()?)
    );
    Ok(Secret {
        hash: hash(&value),
        value,
    })
}

/// A setup code like `K7QM-4XPD-9TRB`: 60 random bits, easy to read in a log and to type.
pub fn setup_code() -> Result<String> {
    Ok(codes::format(&random_bytes::<12>()?, 3, 4))
}

/// A one-time password like `K7QM4-XPD9T-RB2A6-QWE3R`: 100 random bits.
pub fn password() -> Result<String> {
    Ok(codes::format(&random_bytes::<20>()?, 4, 5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_random_and_only_hashes_are_kept() {
        let a = session().unwrap();
        let b = session().unwrap();
        assert_ne!(a.value, b.value);
        assert_eq!(a.value.len(), 43);
        assert_eq!(a.hash, hash(&a.value));
        assert_eq!(a.hash.len(), 32);
        assert_eq!(format!("{a:?}"), "Secret(***)");

        let token = token().unwrap();
        assert!(token.value.starts_with(TOKEN_PREFIX));
        assert_eq!(token.value.len(), TOKEN_PREFIX.len() + 43);
    }

    #[test]
    fn codes_have_their_shape() {
        let code = setup_code().unwrap();
        assert_eq!(code.len(), 14);
        assert_eq!(code.matches('-').count(), 2);
        let password = password().unwrap();
        assert_eq!(password.len(), 23);
        assert_ne!(password, super::password().unwrap());
    }
}
