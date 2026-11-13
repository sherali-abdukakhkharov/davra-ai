//! Secrets at rest. Values the hub must read back (GitHub webhook secrets)
//! are sealed with ChaCha20-Poly1305 under `DAVRA_MASTER_KEY`, bound to the
//! row they belong to, and stored as `v1:<hex nonce||ciphertext>`. A database
//! dump alone then reveals nothing. Plaintext from before the key was set is
//! sealed at startup.

use anyhow::{anyhow, bail, Context, Result};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use sqlx::PgPool;
use uuid::Uuid;

const PREFIX: &str = "v1:";

pub struct MasterKey(ChaCha20Poly1305);

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(..)")
    }
}

impl MasterKey {
    /// 32 bytes as 64 hex characters (`openssl rand -hex 32`).
    pub fn parse(hex_key: &str) -> Result<Self> {
        let bytes = hex::decode(hex_key.trim())
            .map_err(|_| anyhow!("DAVRA_MASTER_KEY must be 64 hex characters"))?;
        if bytes.len() != 32 {
            bail!("DAVRA_MASTER_KEY must be 32 bytes (64 hex characters)");
        }
        Ok(MasterKey(ChaCha20Poly1305::new(Key::from_slice(&bytes))))
    }

    /// Seals `plain`; `context` (e.g. the project id) must match on open.
    pub fn seal(&self, context: &[u8], plain: &str) -> String {
        let nonce: [u8; 12] = rand::random();
        let ct = self
            .0
            .encrypt(
                Nonce::from_slice(&nonce),
                Payload {
                    msg: plain.as_bytes(),
                    aad: context,
                },
            )
            .expect("chacha20poly1305 encryption cannot fail for small inputs");
        let mut out = nonce.to_vec();
        out.extend_from_slice(&ct);
        format!("{PREFIX}{}", hex::encode(out))
    }

    pub fn open(&self, context: &[u8], stored: &str) -> Result<String> {
        let raw = stored
            .strip_prefix(PREFIX)
            .and_then(|h| hex::decode(h).ok())
            .filter(|b| b.len() > 12)
            .ok_or_else(|| anyhow!("not a sealed secret"))?;
        let (nonce, ct) = raw.split_at(12);
        let plain = self
            .0
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ct,
                    aad: context,
                },
            )
            .map_err(|_| anyhow!("sealed secret does not open with this DAVRA_MASTER_KEY"))?;
        String::from_utf8(plain).context("sealed secret is not text")
    }
}

pub fn is_sealed(stored: &str) -> bool {
    stored.starts_with(PREFIX)
}

/// The value to store: sealed when a key is configured, else as is.
pub fn protect(key: Option<&MasterKey>, context: &[u8], plain: &str) -> String {
    match key {
        Some(k) => k.seal(context, plain),
        None => plain.to_string(),
    }
}

/// Reads a stored value back. Plaintext (no key yet) passes through.
pub fn reveal(key: Option<&MasterKey>, context: &[u8], stored: &str) -> Result<String> {
    match (is_sealed(stored), key) {
        (false, _) => Ok(stored.to_string()),
        (true, Some(k)) => k.open(context, stored),
        (true, None) => bail!("secret is sealed but DAVRA_MASTER_KEY is not set"),
    }
}

/// Seals GitHub secrets stored before a key was configured. Returns how many.
pub async fn seal_existing(pool: &PgPool, key: &MasterKey) -> Result<usize> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "select id, github_secret from projects where github_secret is not null and github_secret not like 'v1:%'",
    )
    .fetch_all(pool)
    .await?;
    for (id, plain) in &rows {
        sqlx::query("update projects set github_secret = $2 where id = $1 and github_secret = $3")
            .bind(id)
            .bind(key.seal(id.as_bytes(), plain))
            .bind(plain)
            .execute(pool)
            .await?;
    }
    Ok(rows.len())
}

/// How many GitHub secrets are still stored in the clear.
pub async fn count_plain(pool: &PgPool) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "select count(*) from projects where github_secret is not null and github_secret not like 'v1:%'",
    )
    .fetch_one(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const K: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    #[test]
    fn seals_and_opens_bound_to_context() {
        let k = MasterKey::parse(K).unwrap();
        let s = k.seal(b"project-a", "ghw_secret");
        assert!(is_sealed(&s) && !s.contains("ghw_secret"));
        assert_ne!(
            s,
            k.seal(b"project-a", "ghw_secret"),
            "fresh nonce each time"
        );
        assert_eq!(k.open(b"project-a", &s).unwrap(), "ghw_secret");
        assert!(k.open(b"project-b", &s).is_err(), "moved to another row");
        let other = MasterKey::parse(&K.replace("1f", "ff")).unwrap();
        assert!(other.open(b"project-a", &s).is_err(), "wrong key");
    }

    #[test]
    fn plaintext_passes_and_sealed_needs_the_key() {
        let k = MasterKey::parse(K).unwrap();
        assert_eq!(reveal(None, b"p", "ghw_x").unwrap(), "ghw_x");
        assert_eq!(reveal(Some(&k), b"p", "ghw_x").unwrap(), "ghw_x");
        let s = protect(Some(&k), b"p", "ghw_x");
        assert!(reveal(None, b"p", &s).is_err());
        assert_eq!(protect(None, b"p", "ghw_x"), "ghw_x");
        assert!(MasterKey::parse("abcd").is_err());
        assert!(format!("{k:?}") == "MasterKey(..)");
    }
}
