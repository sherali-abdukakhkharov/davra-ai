//! Typed HTTP client for the hub API.

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use davra_core::api::*;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    hub: String,
    token: String,
}

fn enc(s: &str) -> String {
    // Project slugs are validated by the hub; still, never let one change the path.
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .collect()
}

impl Client {
    pub fn new(hub: &str, token: String, timeout: Duration) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(concat!("davra/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Client {
            http,
            hub: hub.to_string(),
            token,
        })
    }

    async fn handle<T: DeserializeOwned>(&self, resp: reqwest::Response) -> Result<T> {
        let status = resp.status();
        if status.is_success() {
            return resp.json().await.context("unexpected response from hub");
        }
        let err: Option<ApiError> = resp.json().await.ok();
        match err {
            Some(e) => bail!("{} ({})", e.message, e.error),
            None => bail!("hub returned HTTP {status}"),
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let resp = self
            .http
            .get(format!("{}{path}", self.hub))
            .bearer_auth(&self.token)
            .query(query)
            .send()
            .await
            .map_err(|e| anyhow!("could not reach {}: {e}", self.hub))?;
        self.handle(resp).await
    }

    async fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let resp = self
            .http
            .post(format!("{}{path}", self.hub))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .map_err(|e| anyhow!("could not reach {}: {e}", self.hub))?;
        self.handle(resp).await
    }

    pub async fn me(&self) -> Result<Me> {
        self.get("/api/v1/me", &[]).await
    }

    /// Picks the explicit project, or the member's only project.
    pub async fn resolve_project(&self, explicit: Option<&str>) -> Result<String> {
        if let Some(p) = explicit {
            return Ok(p.to_string());
        }
        let me = self.me().await?;
        match me.projects.as_slice() {
            [one] => Ok(one.slug.clone()),
            [] => bail!("@{} has no projects on this hub", me.member.handle),
            many => bail!(
                "several projects ({}); add a .davra.toml to the repo or pass a project",
                many.iter()
                    .map(|p| p.slug.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    pub async fn brief(&self, project: &str) -> Result<Brief> {
        self.get(&format!("/api/v1/projects/{}/brief", enc(project)), &[])
            .await
    }

    pub async fn inbox(&self, project: &str, limit: i64) -> Result<Inbox> {
        self.get(
            &format!("/api/v1/projects/{}/inbox", enc(project)),
            &[("limit", limit.to_string())],
        )
        .await
    }

    pub async fn ack(&self, project: &str, up_to: i64) -> Result<AckResponse> {
        self.post(
            &format!("/api/v1/projects/{}/ack", enc(project)),
            &AckRequest { up_to },
        )
        .await
    }

    pub async fn post_message(&self, project: &str, req: &PostRequest) -> Result<PostResponse> {
        self.post(&format!("/api/v1/projects/{}/messages", enc(project)), req)
            .await
    }

    pub async fn search(&self, project: &str, q: &str, k: i64) -> Result<SearchResults> {
        self.get(
            &format!("/api/v1/projects/{}/search", enc(project)),
            &[("q", q.to_string()), ("k", k.to_string())],
        )
        .await
    }

    pub async fn handoffs(&self, project: &str, status: &str, scope: &str) -> Result<HandoffList> {
        self.get(
            &format!("/api/v1/projects/{}/handoffs", enc(project)),
            &[("status", status.to_string()), ("scope", scope.to_string())],
        )
        .await
    }

    pub async fn handoff(&self, id: i64) -> Result<HandoffView> {
        self.get(&format!("/api/v1/handoffs/{id}"), &[]).await
    }

    pub async fn handoff_create(&self, project: &str, req: &HandoffCreate) -> Result<HandoffView> {
        self.post(&format!("/api/v1/projects/{}/handoffs", enc(project)), req)
            .await
    }

    pub async fn handoff_done(&self, id: i64, note: &str) -> Result<HandoffView> {
        self.post(
            &format!("/api/v1/handoffs/{id}/done"),
            &HandoffDone {
                note: note.to_string(),
            },
        )
        .await
    }

    pub async fn handoff_sync(
        &self,
        project: &str,
        req: &HandoffSync,
    ) -> Result<HandoffSyncResult> {
        self.post(
            &format!("/api/v1/projects/{}/handoffs/sync", enc(project)),
            req,
        )
        .await
    }

    /// Returns (question id, owner handle, shown with buttons).
    pub async fn ask_owner(
        &self,
        project: &str,
        question: &str,
        options: &[String],
    ) -> Result<(i64, String, bool)> {
        let v: serde_json::Value = self
            .post(
                &format!("/api/v1/projects/{}/questions", enc(project)),
                &serde_json::json!({ "question": question, "options": options }),
            )
            .await?;
        Ok((
            v["id"].as_i64().unwrap_or_default(),
            v["owner"].as_str().unwrap_or_default().to_string(),
            v["buttons"].as_bool().unwrap_or(false),
        ))
    }

    pub async fn answer_question(&self, id: i64, answer: &str) -> Result<()> {
        let _: serde_json::Value = self
            .post(
                &format!("/api/v1/questions/{id}/answer"),
                &serde_json::json!({ "answer": answer }),
            )
            .await?;
        Ok(())
    }

    pub async fn message(&self, id: i64, context: i64) -> Result<MessageContext> {
        self.get(
            &format!("/api/v1/messages/{id}"),
            &[("context", context.to_string())],
        )
        .await
    }
}
