//! The chat protocol's harness plugin, `bmo-chat`.
//!
//! The `bmo-chat` command's MCP server drives any chat provider at a given address through bmo-chat-client,
//! one tool for each provider route a harness needs; the skill beside this crate tells a harness when and how
//! to use it. The crate also generates the protocol's OpenAPI documents, which carry the tools under
//! `x-tools`. The plugin is stated in `docs/protocol.md`, "The harness plugin".

mod openapi;
mod server;
mod tools;

pub use openapi::{Side, openapi, openapi_json};
pub use server::ChatServer;
pub use tools::{Answered, ChatTool, ToolFailure, tools};

/// The version this build answers with: the one the testbed hands the build as `BMO_VERSION`, else the
/// manifest's placeholder (`docs/conventions.md`, 「Versions」).
pub const VERSION: &str = match option_env!("BMO_VERSION") {
  Some(version) => version,
  None => env!("CARGO_PKG_VERSION"),
};
