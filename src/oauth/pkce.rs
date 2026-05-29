//! PKCE verifier/challenge generation.
//!
//! Note: the Anthropic OAuth flow reuses the PKCE verifier as the `state`
//! parameter — there is no independent state token.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

fn random_b64url(byte_len: usize) -> Result<String> {
    let mut buf = vec![0u8; byte_len];
    getrandom::getrandom(&mut buf).map_err(|e| Error::OAuth(format!("rng: {e}")))?;
    Ok(URL_SAFE_NO_PAD.encode(&buf))
}

pub fn state_token() -> Result<String> {
    random_b64url(32)
}

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn generate() -> Result<Self> {
        let verifier = random_b64url(32)?;
        let digest = Sha256::digest(verifier.as_bytes());
        let challenge = URL_SAFE_NO_PAD.encode(digest);
        Ok(Self {
            verifier,
            challenge,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_verifier_and_challenge_are_distinct_and_well_formed() {
        let p = Pkce::generate().unwrap();
        assert_ne!(p.verifier, p.challenge);
        // 32 bytes b64url-no-pad → 43 chars; sha256 32 bytes b64url-no-pad → 43 chars
        assert_eq!(p.verifier.len(), 43);
        assert_eq!(p.challenge.len(), 43);
        // PKCE charset: [A-Za-z0-9_-]
        for c in p.verifier.chars().chain(p.challenge.chars()) {
            assert!(
                c.is_ascii_alphanumeric() || c == '-' || c == '_',
                "non-PKCE char {c:?}"
            );
        }
    }

    #[test]
    fn pkce_generate_produces_unique_outputs() {
        let a = Pkce::generate().unwrap();
        let b = Pkce::generate().unwrap();
        assert_ne!(a.verifier, b.verifier, "two PKCE verifiers collided");
    }

    #[test]
    fn state_tokens_are_unique() {
        let a = state_token().unwrap();
        let b = state_token().unwrap();
        assert_ne!(a, b, "two state tokens collided");
        assert_eq!(a.len(), 43);
    }
}
