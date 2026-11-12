//! GitHub webhooks: `POST /webhooks/github/{org}/{project}`.
//!
//! Every delivery is verified with `X-Hub-Signature-256` (HMAC-SHA256 of the
//! raw body with the project's secret, compared in constant time) before the
//! body is even parsed. Events become short `system` messages from "github",
//! which agents see as `[external]`: information, never instructions.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

use crate::api::relay;
use crate::db::{self, NewMessage};
use crate::error::{AppError, AppResult};
use crate::AppState;

type HmacSha256 = Hmac<Sha256>;

pub fn signature_ok(secret: &str, body: &[u8], header: Option<&str>) -> bool {
    let Some(hex_sig) = header.and_then(|h| h.strip_prefix("sha256=")) else {
        return false;
    };
    let Ok(sig) = hex::decode(hex_sig.trim()) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&sig).is_ok()
}

fn s<'a>(v: &'a Value, path: &[&str]) -> &'a str {
    let mut cur = v;
    for p in path {
        cur = &cur[*p];
    }
    cur.as_str().unwrap_or("")
}

fn first_line(t: &str) -> &str {
    t.lines().next().unwrap_or("")
}

/// Turns an event into one line of text, or `None` to ignore it.
pub fn summarize(event: &str, v: &Value) -> Option<String> {
    match event {
        "push" => {
            let branch = s(v, &["ref"]).strip_prefix("refs/heads/")?;
            let commits = v["commits"].as_array()?;
            if commits.is_empty() {
                return None;
            }
            let who = s(v, &["pusher", "name"]);
            let head = first_line(s(v, &["head_commit", "message"]));
            let more = if commits.len() > 1 {
                format!(" (+{} more)", commits.len() - 1)
            } else {
                String::new()
            };
            Some(format!("push to {branch} by {who}: {head}{more}"))
        }
        "pull_request" => {
            let action = s(v, &["action"]);
            let pr = &v["pull_request"];
            let verb = match action {
                "opened" => "opened",
                "reopened" => "reopened",
                "ready_for_review" => "ready for review",
                "closed" if pr["merged"].as_bool() == Some(true) => "merged",
                "closed" => "closed",
                _ => return None,
            };
            Some(format!(
                "PR #{} {verb} by {}: {} {}",
                pr["number"].as_i64().unwrap_or_default(),
                s(pr, &["user", "login"]),
                s(pr, &["title"]),
                s(pr, &["html_url"])
            ))
        }
        "workflow_run" => {
            if s(v, &["action"]) != "completed" {
                return None;
            }
            let run = &v["workflow_run"];
            let mark = match s(run, &["conclusion"]) {
                "success" => "passed",
                "failure" => "FAILED",
                "cancelled" | "skipped" => return None,
                other => other,
            };
            Some(format!(
                "CI {} {mark} on {}: {}",
                s(run, &["name"]),
                s(run, &["head_branch"]),
                s(run, &["html_url"])
            ))
        }
        _ => None,
    }
}

pub async fn webhook(
    State(state): State<AppState>,
    Path((org, slug)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<StatusCode> {
    let row: Option<(Uuid, Uuid, Option<String>, Option<i64>)> = sqlx::query_as(
        "select p.id, p.org_id, p.github_secret, p.tg_chat_id from projects p join orgs o on o.id = p.org_id \
         where o.slug = $1 and p.slug = $2",
    )
    .bind(&org)
    .bind(&slug)
    .fetch_optional(&state.pool)
    .await?;
    // Unknown project and bad signature look the same from outside.
    let reject = || AppError::Unauthorized;
    let (pid, org_id, secret, chat) = row.ok_or_else(reject)?;
    let secret = secret.ok_or_else(reject)?;
    let sig = headers
        .get("x-hub-signature-256")
        .and_then(|h| h.to_str().ok());
    if !signature_ok(&secret, &body, sig) {
        tracing::warn!(project = %slug, "github webhook with a bad signature");
        return Err(reject());
    }
    let event = headers
        .get("x-github-event")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if event == "ping" {
        return Ok(StatusCode::NO_CONTENT);
    }
    let v: Value = serde_json::from_slice(&body)
        .map_err(|_| AppError::BadRequest("body is not JSON".into()))?;
    let Some(text) = summarize(event, &v) else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let text = davra_core::redact::redact(&davra_core::format::truncate_chars(&text, 600)).0;
    let id = db::insert_message(
        &state.pool,
        &NewMessage {
            org_id,
            project_id: pid,
            author_id: None,
            author_name: Some("github".into()),
            kind: "system",
            topic: Some("ci".into()),
            body: text.clone(),
            reply_to: None,
            mentions: vec![],
            source: "api",
            tg_message_id: None,
            tg_thread_id: None,
            created_at: None,
        },
    )
    .await?
    .ok_or_else(|| AppError::Internal(anyhow::anyhow!("insert returned no id")))?;
    relay(&state, chat, id, &format!("🐙 {text}"), None).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn verifies_github_signatures() {
        // Example from GitHub's docs: secret "It's a Secret to Everybody", body "Hello, World!".
        let sig = "sha256=757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17";
        assert!(signature_ok(
            "It's a Secret to Everybody",
            b"Hello, World!",
            Some(sig)
        ));
        assert!(!signature_ok("wrong", b"Hello, World!", Some(sig)));
        assert!(!signature_ok(
            "It's a Secret to Everybody",
            b"Hello, World?",
            Some(sig)
        ));
        assert!(!signature_ok(
            "It's a Secret to Everybody",
            b"Hello, World!",
            None
        ));
        assert!(!signature_ok(
            "It's a Secret to Everybody",
            b"Hello, World!",
            Some("sha1=abc")
        ));
    }

    #[test]
    fn summarizes_events() {
        let push = json!({ "ref": "refs/heads/main", "pusher": { "name": "gulom" },
            "commits": [{}, {}], "head_commit": { "message": "MOS-032: counter\n\nbody" } });
        assert_eq!(
            summarize("push", &push).unwrap(),
            "push to main by gulom: MOS-032: counter (+1 more)"
        );
        let tag = json!({ "ref": "refs/tags/v1", "commits": [{}] });
        assert!(summarize("push", &tag).is_none());
        let pr = json!({ "action": "closed", "pull_request": { "number": 12, "merged": true,
            "user": { "login": "lionali" }, "title": "Counter", "html_url": "https://github.com/x/y/pull/12" } });
        assert_eq!(
            summarize("pull_request", &pr).unwrap(),
            "PR #12 merged by lionali: Counter https://github.com/x/y/pull/12"
        );
        let ci = json!({ "action": "completed", "workflow_run": { "name": "ci", "conclusion": "failure",
            "head_branch": "main", "html_url": "https://github.com/x/y/actions/runs/1" } });
        assert_eq!(
            summarize("workflow_run", &ci).unwrap(),
            "CI ci FAILED on main: https://github.com/x/y/actions/runs/1"
        );
        assert!(summarize("star", &json!({})).is_none());
    }
}
