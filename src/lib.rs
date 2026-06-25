#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod audit;
mod cache;
pub mod edit;
pub mod engine;
pub mod freshness;
pub(crate) mod glob;
mod index;
pub mod mcp;
pub mod output;
mod parser;
pub mod pipeline;
pub mod project_path;
pub mod snapshot;
pub mod store;
pub mod types;
mod walker;
