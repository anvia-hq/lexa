mod context;
mod context_helpers;
mod core;
mod dep_graph;
mod files;
mod imports;
mod indexing;
mod persistence;
mod ranking;
mod search;
mod shared;

pub use core::*;
pub use dep_graph::DepGraph;
pub use shared::is_comment_or_blank;

pub fn hash_content(content: &str) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for byte in content.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

impl Default for Engine {
    fn default() -> Self {
        Self::new(16384)
    }
}

#[cfg(test)]
mod tests;
