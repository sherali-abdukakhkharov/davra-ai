//! Agent usage: Claude Code exports its OpenTelemetry metrics (OTLP/HTTP JSON,
//! delta temporality) to `POST /otlp/v1/metrics` with the agent's token, and
//! the web app shows them per project, member, day and model.
//!
//! Only the counters below are kept, summed per local day. Attributes such as
//! `user.email`, `session.id` or `user.account_uuid` are read past and dropped.

use std::collections::BTreeMap;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::accounts::SessionUser;
use crate::db::Caller;
use crate::error::{AppError, AppResult};
use crate::AppState;

pub const PROJECT_HEADER: &str = "x-davra-project";

/// One row's counters; also the JSON shape the web app reads.
#[derive(Default, Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct Usage {
    pub cost_usd: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub sessions: i64,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub commits: i64,
    pub pull_requests: i64,
    pub edits_accepted: i64,
    pub edits_rejected: i64,
    pub active_secs: f64,
}

impl Usage {
    fn is_empty(&self) -> bool {
        *self == Usage::default()
    }
}

/// What one OTLP request adds, by model ('' for counters without one), plus
/// the latest point time (seconds) and how many points were refused.
#[derive(Default, Debug)]
pub struct Batch {
    pub by_model: BTreeMap<String, Usage>,
    pub time_secs: Option<f64>,
    pub rejected: i64,
}

fn attr<'a>(point: &'a Value, key: &str) -> Option<&'a str> {
    point["attributes"]
        .as_array()?
        .iter()
        .find(|a| a["key"] == key)?["value"]["stringValue"]
        .as_str()
}

/// OTLP JSON writes 64-bit integers as strings; accept numbers too.
fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn is_delta(sum: &Value) -> bool {
    match &sum["aggregationTemporality"] {
        Value::Number(n) => n.as_i64() == Some(1),
        Value::String(s) => s == "AGGREGATION_TEMPORALITY_DELTA",
        // Unspecified: Claude Code's default is delta.
        Value::Null => true,
        _ => false,
    }
}

/// Reads an `ExportMetricsServiceRequest`. Unknown metrics are ignored;
/// cumulative points are refused (we only add deltas).
pub fn parse(body: &Value) -> Batch {
    let mut b = Batch::default();
    let empty = vec![];
    for rm in body["resourceMetrics"].as_array().unwrap_or(&empty) {
        for sm in rm["scopeMetrics"].as_array().unwrap_or(&empty) {
            for m in sm["metrics"].as_array().unwrap_or(&empty) {
                let Some(name) = m["name"].as_str() else {
                    continue;
                };
                if !name.starts_with("claude_code.") {
                    continue;
                }
                let sum = &m["sum"];
                let points = sum["dataPoints"].as_array().unwrap_or(&empty);
                if !is_delta(sum) {
                    b.rejected += points.len() as i64;
                    continue;
                }
                for p in points {
                    let Some(v) = num(&p["asDouble"]).or_else(|| num(&p["asInt"])) else {
                        continue;
                    };
                    if !v.is_finite() || v < 0.0 {
                        b.rejected += 1;
                        continue;
                    }
                    if let Some(t) = num(&p["timeUnixNano"]) {
                        let secs = t / 1e9;
                        b.time_secs = Some(b.time_secs.map_or(secs, |x: f64| x.max(secs)));
                    }
                    add(&mut b, name, p, v);
                }
            }
        }
    }
    b
}

fn slot(b: &mut Batch, model: String) -> &mut Usage {
    b.by_model.entry(model).or_default()
}

fn add(b: &mut Batch, name: &str, p: &Value, v: f64) {
    let n = v.round() as i64;
    let model = || {
        attr(p, "model")
            .unwrap_or("")
            .chars()
            .take(80)
            .collect::<String>()
    };
    let kind = attr(p, "type").unwrap_or("");
    match name {
        "claude_code.cost.usage" => slot(b, model()).cost_usd += v,
        "claude_code.token.usage" => {
            let u = slot(b, model());
            match kind {
                "input" => u.input_tokens += n,
                "output" => u.output_tokens += n,
                "cacheRead" => u.cache_read_tokens += n,
                "cacheCreation" => u.cache_write_tokens += n,
                _ => {}
            }
        }
        "claude_code.lines_of_code.count" => {
            let u = slot(b, model());
            match kind {
                "added" => u.lines_added += n,
                "removed" => u.lines_removed += n,
                _ => {}
            }
        }
        "claude_code.session.count" => slot(b, String::new()).sessions += n,
        "claude_code.commit.count" => slot(b, String::new()).commits += n,
        "claude_code.pull_request.count" => slot(b, String::new()).pull_requests += n,
        "claude_code.code_edit_tool.decision" => {
            let u = slot(b, String::new());
            match attr(p, "decision") {
                Some("accept") => u.edits_accepted += n,
                Some("reject") => u.edits_rejected += n,
                _ => {}
            }
        }
        "claude_code.active_time.total" => slot(b, String::new()).active_secs += v,
        _ => {}
    }
}

/// `POST /otlp/v1/metrics`. The project comes from `X-Davra-Project` (set by
/// `davra otel-headers` from the repo's .davra.toml), or is the caller's only one.
pub async fn ingest(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Json<Value>> {
    let doc: Value = serde_json::from_slice(&body).map_err(|_| {
        AppError::BadRequest(
            "expected OTLP/HTTP JSON (OTEL_EXPORTER_OTLP_METRICS_PROTOCOL=http/json)".into(),
        )
    })?;
    let batch = parse(&doc);
    let mut tx = crate::api::scoped(&state, &caller).await?;
    let slug = match headers.get(PROJECT_HEADER).and_then(|v| v.to_str().ok()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => {
            let projects = crate::db::projects_of(&mut *tx, &caller).await?;
            match projects.as_slice() {
                [only] => only.slug.clone(),
                _ => {
                    return Err(AppError::BadRequest(format!(
                    "set the {PROJECT_HEADER} header (run `davra setup claude` in the repository)"
                )))
                }
            }
        }
    };
    let p = crate::api::project_in(&mut *tx, &caller, &slug).await?;
    let now = chrono::Utc::now().timestamp() as f64;
    // A point from far in the past or future is dated today.
    let t = batch
        .time_secs
        .filter(|t| (t - now).abs() < 3.0 * 86_400.0)
        .unwrap_or(now);
    for (model, u) in &batch.by_model {
        if u.is_empty() {
            continue;
        }
        sqlx::query(
            "insert into agent_usage (org_id, project_id, member_id, day, model, cost_usd, input_tokens, \
             output_tokens, cache_read_tokens, cache_write_tokens, sessions, lines_added, lines_removed, \
             commits, pull_requests, edits_accepted, edits_rejected, active_secs) \
             values ($1, $2, $3, to_timestamp($4)::date, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18) \
             on conflict (project_id, member_id, day, model) do update set \
             cost_usd = agent_usage.cost_usd + excluded.cost_usd, \
             input_tokens = agent_usage.input_tokens + excluded.input_tokens, \
             output_tokens = agent_usage.output_tokens + excluded.output_tokens, \
             cache_read_tokens = agent_usage.cache_read_tokens + excluded.cache_read_tokens, \
             cache_write_tokens = agent_usage.cache_write_tokens + excluded.cache_write_tokens, \
             sessions = agent_usage.sessions + excluded.sessions, \
             lines_added = agent_usage.lines_added + excluded.lines_added, \
             lines_removed = agent_usage.lines_removed + excluded.lines_removed, \
             commits = agent_usage.commits + excluded.commits, \
             pull_requests = agent_usage.pull_requests + excluded.pull_requests, \
             edits_accepted = agent_usage.edits_accepted + excluded.edits_accepted, \
             edits_rejected = agent_usage.edits_rejected + excluded.edits_rejected, \
             active_secs = agent_usage.active_secs + excluded.active_secs, updated_at = now()",
        )
        .bind(p.org_id)
        .bind(p.id)
        .bind(caller.member_id)
        .bind(t)
        .bind(model)
        .bind(u.cost_usd)
        .bind(u.input_tokens)
        .bind(u.output_tokens)
        .bind(u.cache_read_tokens)
        .bind(u.cache_write_tokens)
        .bind(u.sessions)
        .bind(u.lines_added)
        .bind(u.lines_removed)
        .bind(u.commits)
        .bind(u.pull_requests)
        .bind(u.edits_accepted)
        .bind(u.edits_rejected)
        .bind(u.active_secs)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    if batch.rejected > 0 {
        return Ok(Json(json!({ "partialSuccess": {
            "rejectedDataPoints": batch.rejected.to_string(),
            "errorMessage": "only delta temporality is accepted (OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE=delta)"
        }})));
    }
    Ok(Json(json!({})))
}

const SUMS: &str = "coalesce(sum(u.cost_usd), 0)::float8 as cost_usd, \
    coalesce(sum(u.input_tokens), 0)::int8 as input_tokens, \
    coalesce(sum(u.output_tokens), 0)::int8 as output_tokens, \
    coalesce(sum(u.cache_read_tokens), 0)::int8 as cache_read_tokens, \
    coalesce(sum(u.cache_write_tokens), 0)::int8 as cache_write_tokens, \
    coalesce(sum(u.sessions), 0)::int8 as sessions, \
    coalesce(sum(u.lines_added), 0)::int8 as lines_added, \
    coalesce(sum(u.lines_removed), 0)::int8 as lines_removed, \
    coalesce(sum(u.commits), 0)::int8 as commits, \
    coalesce(sum(u.pull_requests), 0)::int8 as pull_requests, \
    coalesce(sum(u.edits_accepted), 0)::int8 as edits_accepted, \
    coalesce(sum(u.edits_rejected), 0)::int8 as edits_rejected, \
    coalesce(sum(u.active_secs), 0)::float8 as active_secs";

#[derive(Serialize, sqlx::FromRow)]
pub struct DayUsage {
    day: NaiveDate,
    #[sqlx(flatten)]
    #[serde(flatten)]
    usage: Usage,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct MemberUsage {
    handle: String,
    owner: Option<String>,
    last_day: NaiveDate,
    #[sqlx(flatten)]
    #[serde(flatten)]
    usage: Usage,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ModelUsage {
    model: String,
    cost_usd: f64,
    input_tokens: i64,
    output_tokens: i64,
    cache_read_tokens: i64,
    cache_write_tokens: i64,
}

#[derive(Serialize)]
pub struct UsageReport {
    days: i64,
    since: NaiveDate,
    tz: String,
    totals: Usage,
    daily: Vec<DayUsage>,
    members: Vec<MemberUsage>,
    models: Vec<ModelUsage>,
}

#[derive(Deserialize)]
pub struct DaysQuery {
    days: Option<i64>,
}

/// `GET /api/v1/web/projects/{org}/{slug}/usage?days=N`
pub async fn report(
    State(state): State<AppState>,
    SessionUser(user): SessionUser,
    Path((org, slug)): Path<(String, String)>,
    Query(q): Query<DaysQuery>,
) -> AppResult<Json<UsageReport>> {
    let days = q.days.unwrap_or(14).clamp(1, 90);
    let (caller, p) = crate::webapp::open_project(&state, user.id, &org, &slug).await?;
    let mut tx = crate::api::scoped(&state, &caller).await?;
    let (since, tz): (NaiveDate, String) =
        sqlx::query_as("select (current_date - ($1::int - 1))::date, current_setting('TimeZone')")
            .bind(days as i32)
            .fetch_one(&mut *tx)
            .await?;
    let scope = "from agent_usage u where u.project_id = $1 and u.day >= $2";
    let totals: Usage = sqlx::query_as(&format!("select {SUMS} {scope}"))
        .bind(p.id)
        .bind(since)
        .fetch_one(&mut *tx)
        .await?;
    let daily: Vec<DayUsage> = sqlx::query_as(&format!(
        "select u.day, {SUMS} {scope} group by u.day order by u.day"
    ))
    .bind(p.id)
    .bind(since)
    .fetch_all(&mut *tx)
    .await?;
    let members: Vec<MemberUsage> = sqlx::query_as(&format!(
        "select m.handle, ow.handle as owner, max(u.day) as last_day, {SUMS} \
         from agent_usage u join members m on m.id = u.member_id left join members ow on ow.id = m.owner_id \
         where u.project_id = $1 and u.day >= $2 group by m.handle, ow.handle \
         order by cost_usd desc, m.handle"
    ))
    .bind(p.id)
    .bind(since)
    .fetch_all(&mut *tx)
    .await?;
    let models: Vec<ModelUsage> = sqlx::query_as(
        "select u.model, coalesce(sum(u.cost_usd), 0)::float8 as cost_usd, \
         coalesce(sum(u.input_tokens), 0)::int8 as input_tokens, \
         coalesce(sum(u.output_tokens), 0)::int8 as output_tokens, \
         coalesce(sum(u.cache_read_tokens), 0)::int8 as cache_read_tokens, \
         coalesce(sum(u.cache_write_tokens), 0)::int8 as cache_write_tokens \
         from agent_usage u where u.project_id = $1 and u.day >= $2 and u.model <> '' \
         group by u.model order by cost_usd desc, u.model",
    )
    .bind(p.id)
    .bind(since)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(UsageReport {
        days,
        since,
        tz,
        totals,
        daily,
        members,
        models,
    }))
}

/// One project's totals for one local day; None when nothing was reported.
pub async fn day_totals(
    db: impl sqlx::PgExecutor<'_>,
    project: uuid::Uuid,
    day: NaiveDate,
) -> sqlx::Result<Option<Usage>> {
    let u: Usage = sqlx::query_as(&format!(
        "select {SUMS} from agent_usage u where u.project_id = $1 and u.day = $2"
    ))
    .bind(project)
    .bind(day)
    .fetch_one(db)
    .await?;
    Ok((!u.is_empty()).then_some(u))
}

/// 1234 -> "1.2k", 3_400_000 -> "3.4M".
pub fn compact(n: i64) -> String {
    let f = n as f64;
    match n.abs() {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}k", f / 1e3).replace(".0k", "k"),
        _ => format!("{:.1}M", f / 1e6).replace(".0M", "M"),
    }
}

impl Usage {
    pub fn tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.cache_read_tokens + self.cache_write_tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_numbers_and_daily_line() {
        assert_eq!(compact(950), "950");
        assert_eq!(compact(1_000), "1k");
        assert_eq!(compact(12_340), "12.3k");
        assert_eq!(compact(3_400_000), "3.4M");
        let u = Usage {
            cost_usd: 2.104,
            input_tokens: 1_000,
            output_tokens: 200_000,
            cache_read_tokens: 1_000_000,
            lines_added: 340,
            lines_removed: 120,
            commits: 3,
            ..Usage::default()
        };
        assert_eq!(
            crate::tgcard::usage_line(crate::tgcard::Lang::Uz, &u),
            "AI sarfi: $2.10 · 1.2M token · +340/−120 qator · 3 commit"
        );
    }
    #[test]
    fn sums_claude_code_metrics_and_drops_identity() {
        let body = json!({ "resourceMetrics": [{
            "resource": { "attributes": [{ "key": "service.name", "value": { "stringValue": "claude-code" } }] },
            "scopeMetrics": [{ "metrics": [
                { "name": "claude_code.cost.usage", "unit": "USD", "sum": { "aggregationTemporality": 1, "dataPoints": [
                    { "attributes": [
                        { "key": "model", "value": { "stringValue": "claude-opus-5-5" } },
                        { "key": "user.email", "value": { "stringValue": "a@b.c" } }],
                      "timeUnixNano": "1763020800000000000", "asDouble": 0.25 },
                    { "attributes": [{ "key": "model", "value": { "stringValue": "claude-opus-5-5" } }],
                      "timeUnixNano": "1763020860000000000", "asDouble": 0.5 }]}},
                { "name": "claude_code.token.usage", "sum": { "aggregationTemporality": "AGGREGATION_TEMPORALITY_DELTA", "dataPoints": [
                    { "attributes": [
                        { "key": "model", "value": { "stringValue": "claude-opus-5-5" } },
                        { "key": "type", "value": { "stringValue": "cacheRead" } }], "asDouble": 1200 },
                    { "attributes": [
                        { "key": "model", "value": { "stringValue": "claude-opus-5-5" } },
                        { "key": "type", "value": { "stringValue": "output" } }], "asInt": "300" }]}},
                { "name": "claude_code.active_time.total", "sum": { "aggregationTemporality": 1, "dataPoints": [
                    { "attributes": [{ "key": "type", "value": { "stringValue": "cli" } }], "asDouble": 12.5 }]}},
                { "name": "claude_code.code_edit_tool.decision", "sum": { "aggregationTemporality": 1, "dataPoints": [
                    { "attributes": [{ "key": "decision", "value": { "stringValue": "accept" } }], "asInt": 2 }]}},
                { "name": "claude_code.commit.count", "sum": { "aggregationTemporality": 2, "dataPoints": [
                    { "asInt": "4" }]}},
                { "name": "other.metric", "sum": { "dataPoints": [{ "asInt": "9" }]}}
            ]}]
        }]});
        let b = parse(&body);
        assert_eq!(b.rejected, 1, "cumulative points are refused");
        assert_eq!(b.time_secs, Some(1_763_020_860.0));
        let opus = &b.by_model["claude-opus-5-5"];
        assert_eq!(opus.cost_usd, 0.75);
        assert_eq!(opus.cache_read_tokens, 1200);
        assert_eq!(opus.output_tokens, 300);
        let none = &b.by_model[""];
        assert_eq!(none.active_secs, 12.5);
        assert_eq!(none.edits_accepted, 2);
        assert_eq!(none.commits, 0);
        assert_eq!(b.by_model.len(), 2);
    }
}
