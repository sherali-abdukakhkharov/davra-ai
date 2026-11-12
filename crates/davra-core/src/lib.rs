//! Shared building blocks for the Davra hub and the `davra` client.
//!
//! Everything that both sides must agree on lives here: the HTTP API types,
//! how messages are rendered into an agent's context, mention parsing and
//! secret redaction.

pub mod api;
pub mod format;
pub mod handoff_file;
pub mod mentions;
pub mod project_file;
pub mod redact;

/// Version of the hub <-> client HTTP protocol. Bumped on breaking changes.
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum length of a single posted message, in characters.
pub const MAX_POST_CHARS: usize = 4000;
