use hashbrown::HashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

pub type AgentId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    Snapshot,
    Replace,
    Insert,
    Delete,
    Tombstone,
    Create,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub seq: u64,
    pub agent: AgentId,
    pub timestamp: i64,
    pub op: Op,
    pub hash: u64,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVersions {
    pub path: String,
    pub versions: Vec<Version>,
}

impl FileVersions {
    fn new(path: String) -> Self {
        Self {
            path,
            versions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEntry {
    pub path: String,
    pub seq: u64,
    pub op: Op,
    pub size: u64,
    pub timestamp: i64,
}

pub struct Store {
    files: Mutex<HashMap<String, FileVersions>>,
    seq: AtomicU64,
    max_versions: usize,
}

impl Store {
    pub fn new() -> Self {
        Self {
            files: Mutex::new(HashMap::new()),
            seq: AtomicU64::new(0),
            max_versions: 100,
        }
    }

    pub fn record_snapshot(&self, path: &str, size: u64, hash: u64) -> u64 {
        self.append_version(path, 0, Op::Snapshot, hash, size)
    }

    pub fn record_edit(&self, path: &str, agent: AgentId, op: Op, hash: u64, size: u64) -> u64 {
        self.append_version(path, agent, op, hash, size)
    }

    pub fn record_delete(&self, path: &str, agent: AgentId) -> u64 {
        self.append_version(path, agent, Op::Tombstone, 0, 0)
    }

    fn lock_files(&self) -> MutexGuard<'_, HashMap<String, FileVersions>> {
        self.files
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn append_version(&self, path: &str, agent: AgentId, op: Op, hash: u64, size: u64) -> u64 {
        let next_seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let mut files = self.lock_files();
        let entry = files
            .entry(path.to_string())
            .or_insert_with(|| FileVersions::new(path.to_string()));

        entry.versions.push(Version {
            seq: next_seq,
            agent,
            timestamp,
            op,
            hash,
            size,
        });

        if entry.versions.len() > self.max_versions {
            let excess = entry.versions.len() - self.max_versions;
            entry.versions.drain(0..excess);
        }

        next_seq
    }

    pub fn changes_since_detailed(&self, since: u64) -> Vec<ChangeEntry> {
        let files = self.lock_files();
        let mut result = Vec::new();
        for (path, fv) in files.iter() {
            let latest_change = fv
                .versions
                .iter()
                .filter(|v| v.seq > since)
                .max_by_key(|v| v.seq);

            if let Some(v) = latest_change {
                result.push(ChangeEntry {
                    path: path.clone(),
                    seq: v.seq,
                    op: v.op,
                    size: v.size,
                    timestamp: v.timestamp,
                });
            }
        }
        result
    }

    pub fn current_seq(&self) -> u64 {
        self.seq.load(Ordering::SeqCst)
    }
}

impl Default for Store {
    fn default() -> Self {
        Self::new()
    }
}
