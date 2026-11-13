//! Embeddings through an Ollama-compatible `/api/embed` endpoint (bge-m3 by
//! default, which handles Uzbek, Russian and English in one space).
//!
//! A background worker fills `messages.embedding`; search embeds the query and
//! fuses vector neighbours with the lexical results. Without an embedder the
//! hub simply stays lexical.

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

pub const DIM: usize = 1024;
const BATCH: i64 = 32;
const MAX_ATTEMPTS: i16 = 5;
/// Long messages are cut before embedding; the head carries most meaning.
const MAX_CHARS: usize = 2000;

pub struct Embedder {
    http: reqwest::Client,
    url: String,
    model: String,
}

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

/// pgvector's text form: `[0.1,0.2,...]`.
pub fn to_pgvector(v: &[f32]) -> String {
    let mut s = String::with_capacity(v.len() * 10 + 2);
    s.push('[');
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let x = if x.is_finite() { *x } else { 0.0 };
        s.push_str(&format!("{x}"));
    }
    s.push(']');
    s
}

impl Embedder {
    pub fn new(url: &str, model: &str) -> Result<Self> {
        Ok(Embedder {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()?,
            url: url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        })
    }

    pub async fn embed(&self, texts: Vec<String>, timeout: Duration) -> Result<Vec<Vec<f32>>> {
        let n = texts.len();
        let input = texts
            .into_iter()
            .map(|t| davra_core::format::truncate_chars(&t, MAX_CHARS))
            .collect();
        let resp = self
            .http
            .post(format!("{}/api/embed", self.url))
            .timeout(timeout)
            .json(&EmbedRequest {
                model: &self.model,
                input,
            })
            .send()
            .await
            .map_err(|e| anyhow!("embedder unreachable: {e}"))?;
        if !resp.status().is_success() {
            bail!("embedder returned HTTP {}", resp.status());
        }
        let r: EmbedResponse = resp.json().await.context("bad embedder response")?;
        if r.embeddings.len() != n || r.embeddings.iter().any(|e| e.len() != DIM) {
            bail!(
                "embedder returned {} vectors (want {n} of {DIM} dims)",
                r.embeddings.len()
            );
        }
        Ok(r.embeddings)
    }

    pub async fn embed_one(&self, text: &str) -> Result<String> {
        let mut v = self
            .embed(vec![text.to_string()], Duration::from_secs(3))
            .await?;
        Ok(to_pgvector(&v.remove(0)))
    }

    /// Embeds messages that have no vector yet, newest first. Runs forever.
    pub async fn run_worker(self: std::sync::Arc<Self>, pool: PgPool) {
        tracing::info!(model = %self.model, "embedding worker started");
        loop {
            match self.step(&pool).await {
                Ok(0) => tokio::time::sleep(Duration::from_secs(5)).await,
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "embedding batch failed, retrying in 30s");
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            }
        }
    }

    async fn step(&self, pool: &PgPool) -> Result<usize> {
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "select id, body from messages where embedding is null and embed_attempts < $1 \
             order by id desc limit $2",
        )
        .bind(MAX_ATTEMPTS)
        .bind(BATCH)
        .fetch_all(pool)
        .await?;
        if rows.is_empty() {
            return Ok(0);
        }
        let ids: Vec<i64> = rows.iter().map(|r| r.0).collect();
        sqlx::query("update messages set embed_attempts = embed_attempts + 1 where id = any($1)")
            .bind(&ids)
            .execute(pool)
            .await?;
        let vectors = self
            .embed(
                rows.iter().map(|r| r.1.clone()).collect(),
                Duration::from_secs(120),
            )
            .await?;
        for (id, v) in ids.iter().zip(vectors) {
            sqlx::query("update messages set embedding = $2::vector where id = $1")
                .bind(id)
                .bind(to_pgvector(&v))
                .execute(pool)
                .await?;
        }
        Ok(ids.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pgvector_text() {
        assert_eq!(to_pgvector(&[0.5, -1.0, f32::NAN]), "[0.5,-1,0]");
        assert_eq!(to_pgvector(&[]), "[]");
    }
}
