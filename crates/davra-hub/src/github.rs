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

/// A GitHub event worth telling the room about.
#[derive(Debug, Clone, PartialEq)]
pub enum GhEvent {
    Push {
        branch: String,
        who: String,
        head: String,
        commits: usize,
        compare: String,
    },
    Pr {
        number: i64,
        /// opened, reopened, ready for review, merged, closed
        verb: &'static str,
        who: String,
        title: String,
        url: String,
    },
    Ci {
        name: String,
        /// passed, FAILED, or GitHub's own conclusion
        mark: String,
        branch: String,
        url: String,
        run_number: Option<i64>,
        sha: String,
        actor: String,
        head: String,
    },
}

impl GhEvent {
    pub fn parse(event: &str, v: &Value) -> Option<GhEvent> {
        match event {
            "push" => {
                let branch = s(v, &["ref"]).strip_prefix("refs/heads/")?;
                let commits = v["commits"].as_array()?;
                if commits.is_empty() {
                    return None;
                }
                Some(GhEvent::Push {
                    branch: branch.to_string(),
                    who: s(v, &["pusher", "name"]).to_string(),
                    head: first_line(s(v, &["head_commit", "message"])).to_string(),
                    commits: commits.len(),
                    compare: s(v, &["compare"]).to_string(),
                })
            }
            "pull_request" => {
                let pr = &v["pull_request"];
                let verb = match s(v, &["action"]) {
                    "opened" => "opened",
                    "reopened" => "reopened",
                    "ready_for_review" => "ready for review",
                    "closed" if pr["merged"].as_bool() == Some(true) => "merged",
                    "closed" => "closed",
                    _ => return None,
                };
                Some(GhEvent::Pr {
                    number: pr["number"].as_i64().unwrap_or_default(),
                    verb,
                    who: s(pr, &["user", "login"]).to_string(),
                    title: s(pr, &["title"]).to_string(),
                    url: s(pr, &["html_url"]).to_string(),
                })
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
                Some(GhEvent::Ci {
                    name: s(run, &["name"]).to_string(),
                    mark: mark.to_string(),
                    branch: s(run, &["head_branch"]).to_string(),
                    url: s(run, &["html_url"]).to_string(),
                    run_number: run["run_number"].as_i64(),
                    sha: s(run, &["head_sha"]).chars().take(7).collect(),
                    actor: s(run, &["actor", "login"]).to_string(),
                    head: first_line(s(run, &["head_commit", "message"])).to_string(),
                })
            }
            _ => None,
        }
    }

    /// The one line stored in the room (what agents read).
    pub fn text(&self) -> String {
        match self {
            GhEvent::Push {
                branch,
                who,
                head,
                commits,
                ..
            } => {
                let more = if *commits > 1 {
                    format!(" (+{} more)", commits - 1)
                } else {
                    String::new()
                };
                format!("push to {branch} by {who}: {head}{more}")
            }
            GhEvent::Pr {
                number,
                verb,
                who,
                title,
                url,
            } => format!("PR #{number} {verb} by {who}: {title} {url}"),
            GhEvent::Ci {
                name,
                mark,
                branch,
                url,
                ..
            } => format!("CI {name} {mark} on {branch}: {url}"),
        }
    }

    /// The same event with secrets masked in every free-text field.
    pub fn redacted(self) -> GhEvent {
        let r = |s: String| davra_core::redact::redact(&s).0;
        match self {
            GhEvent::Push {
                branch,
                who,
                head,
                commits,
                compare,
            } => GhEvent::Push {
                branch: r(branch),
                who: r(who),
                head: r(head),
                commits,
                compare,
            },
            GhEvent::Pr {
                number,
                verb,
                who,
                title,
                url,
            } => GhEvent::Pr {
                number,
                verb,
                who: r(who),
                title: r(title),
                url,
            },
            GhEvent::Ci {
                name,
                mark,
                branch,
                url,
                run_number,
                sha,
                actor,
                head,
            } => GhEvent::Ci {
                name: r(name),
                mark,
                branch: r(branch),
                url,
                run_number,
                sha,
                actor: r(actor),
                head: r(head),
            },
        }
    }

    /// Failures get their own loud card; everything else joins the digest.
    pub fn is_failure(&self) -> bool {
        matches!(self, GhEvent::Ci { mark, .. } if mark != "passed")
    }
}

/// Turns an event into one line of text, or `None` to ignore it.
#[cfg(test)]
pub fn summarize(event: &str, v: &Value) -> Option<String> {
    GhEvent::parse(event, v).map(|e| e.text())
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
    let secret = crate::secrets::reveal(state.master_key.as_deref(), pid.as_bytes(), &secret)
        .map_err(|e| {
            tracing::error!(project = %slug, error = %e, "cannot read the GitHub webhook secret");
            reject()
        })?;
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
    let Some(ev) = GhEvent::parse(event, &v) else {
        return Ok(StatusCode::NO_CONTENT);
    };
    // Commit messages and titles may carry secrets: clean every field the
    // room or the Telegram card shows.
    let ev = ev.redacted();
    let text = davra_core::format::truncate_chars(&ev.text(), 600);
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
    if ev.is_failure() {
        relay(
            &state,
            chat,
            id,
            "",
            |lang, _| {
                crate::tgcard::ci_failed(lang, &ev)
                    .unwrap_or_else(|| crate::tgcard::notice(&format!("🐙 {text}"), false))
            },
            None,
        )
        .await?;
    } else if let Some(chat) = chat {
        // A Telegram hiccup must not make GitHub retry the delivery.
        if let Err(e) = add_to_digest(&state, pid, &slug, chat, &ev, &text).await {
            tracing::warn!(error = ?e, project = %slug, "github digest update failed");
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

/// An open digest: its Telegram message, when it started, its (unix time, HTML) lines.
type DigestRow = (
    i64,
    chrono::DateTime<chrono::Utc>,
    sqlx::types::Json<Vec<(i64, String)>>,
);

/// How long a digest message keeps collecting events, and how many at most.
const DIGEST_WINDOW_SECS: i64 = 15 * 60;
const DIGEST_MAX_LINES: usize = 25;

/// Adds a push, PR or passing CI run to the project's open digest message,
/// or starts a new one. Projects set to plain text get one message per event.
async fn add_to_digest(
    state: &AppState,
    pid: Uuid,
    slug: &str,
    chat: i64,
    ev: &GhEvent,
    text: &str,
) -> anyhow::Result<()> {
    let Some(tg) = &state.telegram else {
        return Ok(());
    };
    let format: String = sqlx::query_scalar("select tg_format from projects where id = $1")
        .bind(pid)
        .fetch_one(&state.pool)
        .await?;
    if format != "rich" {
        let card = crate::tgcard::notice(&format!("🐙 {text}"), true);
        tg.send_card(chat, &card, false, None, None, None).await?;
        return Ok(());
    }
    let now = chrono::Utc::now();
    let line = (now.timestamp(), crate::tgcard::digest_line(tg.lang, ev));
    let mut tx = state.pool.begin().await?;
    let open: Option<DigestRow> = sqlx::query_as(
        "select tg_message_id, started_at, lines from tg_digests where project_id = $1 for update",
    )
    .bind(pid)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((mid, started, sqlx::types::Json(mut lines))) = open {
        if (now - started).num_seconds() < DIGEST_WINDOW_SECS && lines.len() < DIGEST_MAX_LINES {
            lines.push(line.clone());
            let card = crate::tgcard::digest(tg.lang, slug, &lines);
            match tg.edit_card(chat, mid, &card).await {
                Ok(()) => {
                    sqlx::query("update tg_digests set lines = $2 where project_id = $1")
                        .bind(pid)
                        .bind(sqlx::types::Json(&lines))
                        .execute(&mut *tx)
                        .await?;
                    tx.commit().await?;
                    return Ok(());
                }
                // Deleted or too old to edit: start a fresh digest below.
                Err(e) => tracing::info!(error = %e, "digest edit failed; starting a new one"),
            }
        }
    }
    let lines = vec![line];
    let card = crate::tgcard::digest(tg.lang, slug, &lines);
    let mid = tg.send_card(chat, &card, true, None, None, None).await?;
    sqlx::query(
        "insert into tg_digests (project_id, tg_message_id, started_at, lines) values ($1, $2, now(), $3) \
         on conflict (project_id) do update set tg_message_id = excluded.tg_message_id, \
         started_at = excluded.started_at, lines = excluded.lines",
    )
    .bind(pid)
    .bind(mid)
    .bind(sqlx::types::Json(&lines))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
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
