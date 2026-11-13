//! The library half of the `davra` CLI, shared with Davra Desktop: hub
//! client, local config and keychain, the MCP server, and the reviewable
//! plans that connect Claude Code, Cursor and Codex.

pub mod client;
pub mod config;
pub mod connect;
pub mod handoffs;
pub mod install;
pub mod mcp;
pub mod setup;
