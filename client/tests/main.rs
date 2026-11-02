//! The crate's one test harness (bmo-chat's docs/conventions.md, 「Rust builds」): each file of tests a
//! module of it, sharing the process.

mod bodies;
mod dependencies;
mod inbound;
mod missed;
mod numbering;
mod pieces;
mod provider_client;
mod send;
