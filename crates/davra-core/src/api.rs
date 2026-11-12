//! HTTP API types shared by `davra-hub` and `davra`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemberKind {
    Human,
    Agent,
}

impl MemberKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MemberKind::Human => "human",
            MemberKind::Agent => "agent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "human" => Some(MemberKind::Human),
            "agent" => Some(MemberKind::Agent),
            _ => None,
        }
    }
}

/// How much the reader should trust a message, relative to the reader.
///
/// Only `Owner` (and `Myself`) may instruct an agent. Everything else is
/// information, never a command.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Trust {
    Owner,
    #[serde(rename = "self")]
    Myself,
    PeerHuman,
    PeerAgent,
    External,
}

impl Trust {
    pub fn as_str(self) -> &'static str {
        match self {
            Trust::Owner => "owner",
            Trust::Myself => "self",
            Trust::PeerHuman => "peer_human",
            Trust::PeerAgent => "peer_agent",
            Trust::External => "external",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProjectRef {
    pub slug: String,
    pub name: Option<String>,
    pub role: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MemberRef {
    pub handle: String,
    pub kind: MemberKind,
    pub display_name: Option<String>,
    /// Handle of the human who owns this agent.
    pub owner: Option<String>,
    /// Roles this member covers (handoffs addressed to a role reach them).
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Me {
    pub org: String,
    pub member: MemberRef,
    pub projects: Vec<ProjectRef>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MessageView {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    /// Handle for members, display name for unknown chat users.
    pub author: String,
    pub author_kind: Option<MemberKind>,
    pub trust: Trust,
    pub kind: String,
    pub topic: Option<String>,
    pub body: String,
    pub reply_to: Option<i64>,
    pub mentions: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Brief {
    pub project: ProjectRef,
    pub me: MemberRef,
    pub members: Vec<MemberRef>,
    /// Unread messages addressed to the reader (up to a limit).
    pub inbox: Vec<MessageView>,
    pub unread: i64,
    /// Latest messages in the room, oldest first.
    pub recent: Vec<MessageView>,
    pub total_messages: i64,
    /// Open handoffs addressed to the reader (by handle or role).
    #[serde(default)]
    pub handoffs: Vec<HandoffView>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Inbox {
    pub project: String,
    pub messages: Vec<MessageView>,
    /// Id of the last acknowledged message before this call.
    pub cursor: i64,
    /// True when more unread messages exist than were returned.
    pub more: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AckRequest {
    pub up_to: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AckResponse {
    pub cursor: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct PostRequest {
    pub text: String,
    /// Extra handles to address, in addition to `@mentions` in the text.
    #[serde(default)]
    pub to: Vec<String>,
    pub topic: Option<String>,
    pub reply_to: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PostResponse {
    pub id: i64,
    /// Whether the message was relayed to the project's chat.
    pub relayed: bool,
    /// True when secrets were removed from the text before storing.
    pub redacted: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchHit {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub author: String,
    pub trust: Trust,
    pub snippet: String,
    pub score: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchResults {
    pub project: String,
    pub query: String,
    pub hits: Vec<SearchHit>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MessageContext {
    pub project: String,
    pub before: Vec<MessageView>,
    pub message: MessageView,
    pub after: Vec<MessageView>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ApiError {
    pub error: String,
    pub message: String,
}

/// Served at `/.well-known/davra.json` so clients can discover a hub.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WellKnown {
    pub name: String,
    pub version: String,
    pub protocol: u32,
    pub auth: Vec<String>,
    pub features: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HandoffView {
    pub id: i64,
    pub ticket: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub status: String,
    pub body: String,
    pub done_note: Option<String>,
    pub source_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub done_at: Option<DateTime<Utc>>,
    pub message_id: Option<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct HandoffCreate {
    /// Sender label; defaults to the caller's first role, else their handle.
    pub from: Option<String>,
    /// Handles or roles.
    pub to: Vec<String>,
    pub ticket: Option<String>,
    pub body: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HandoffDone {
    pub note: String,
}

/// One handoff file from a repository.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HandoffSyncItem {
    /// Path relative to the repository root, with forward slashes.
    pub source_path: String,
    pub source_hash: String,
    pub from: String,
    pub to: Vec<String>,
    pub ticket: Option<String>,
    pub status: String,
    /// `YYYY-MM-DD` from the frontmatter.
    pub created: Option<String>,
    pub body: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HandoffSync {
    pub items: Vec<HandoffSyncItem>,
    /// Post new and closed handoffs to the room (false for a first import).
    pub announce: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct HandoffSyncResult {
    pub created: usize,
    pub updated: usize,
    pub closed: usize,
    pub reopened: usize,
    pub unchanged: usize,
    /// Handoffs created or changed by this sync.
    #[serde(default)]
    pub touched: Vec<SyncedHandoff>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SyncedHandoff {
    pub id: i64,
    pub source_path: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HandoffList {
    pub project: String,
    pub handoffs: Vec<HandoffView>,
}
