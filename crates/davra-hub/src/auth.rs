//! Bearer tokens. v0.1 uses long-lived per-device tokens created by an
//! admin; only their SHA-256 hash is stored. OAuth 2.1 replaces this in v0.3.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::db::{self, Caller};
use crate::error::AppError;
use crate::AppState;

pub const TOKEN_PREFIX: &str = "dvr_";

pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    format!("{TOKEN_PREFIX}{}", hex::encode(bytes))
}

pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;
        let token = header
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthorized)?
            .trim();
        if !token.starts_with(TOKEN_PREFIX) {
            return Err(AppError::Unauthorized);
        }
        db::caller_by_token_hash(&state.pool, &hash_token(token))
            .await?
            .ok_or(AppError::Unauthorized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_and_prefixed() {
        let a = new_token();
        let b = new_token();
        assert_ne!(a, b);
        assert!(a.starts_with(TOKEN_PREFIX));
        assert_eq!(a.len(), TOKEN_PREFIX.len() + 64);
        assert_eq!(hash_token(&a).len(), 32);
        // Tokens must be caught by redaction if pasted into chat.
        assert!(davra_core::redact::redact(&a).1);
    }
}
