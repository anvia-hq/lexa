use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::engine::Engine;

const MAGIC: &[u8; 8] = b"LEXA\0\0\0\0";
const FORMAT_VERSION: u16 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotHeader {
    magic: [u8; 8],
    version: u16,
    file_count: u32,
    created_at: u64,
    root_hash: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotData {
    header: SnapshotHeader,
    outlines: Vec<(String, crate::types::FileOutline)>,
    file_meta: Vec<(String, crate::types::FileMeta)>,
    contents: Vec<(String, String)>,
    forward_deps: Vec<(String, Vec<String>)>,
}

pub fn write_snapshot(engine: &Engine, output_path: impl AsRef<Path>) -> Result<()> {
    let data = engine.to_snapshot_data();

    let header = SnapshotHeader {
        magic: *MAGIC,
        version: FORMAT_VERSION,
        file_count: data.file_meta.len() as u32,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        root_hash: compute_root_hash(&data.file_meta),
    };

    let snapshot = SnapshotData {
        header,
        outlines: data.outlines,
        file_meta: data.file_meta,
        contents: data.contents,
        forward_deps: data.forward_deps,
    };

    let encoded = bincode::serialize(&snapshot).context("Failed to serialize snapshot")?;

    let path = output_path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create snapshot directory {}", parent.display()))?;
    }

    let tmp_path = temp_snapshot_path(path);
    let write_result = (|| -> Result<()> {
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .with_context(|| format!("Failed to create snapshot file {}", tmp_path.display()))?;
        let mut writer = BufWriter::new(file);
        writer
            .write_all(&(encoded.len() as u64).to_le_bytes())
            .context("Failed to write snapshot length")?;
        writer
            .write_all(&encoded)
            .context("Failed to write snapshot data")?;
        writer.flush().context("Failed to flush snapshot data")?;
        writer
            .get_ref()
            .sync_all()
            .context("Failed to sync snapshot data")?;
        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }

    fs::rename(&tmp_path, path).with_context(|| {
        let _ = fs::remove_file(&tmp_path);
        format!("Failed to replace snapshot file {}", path.display())
    })?;

    Ok(())
}

fn temp_snapshot_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("graph.lexa");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    parent.join(format!(".{filename}.{}.{}.tmp", std::process::id(), nonce))
}

fn compute_root_hash(file_meta: &[(String, crate::types::FileMeta)]) -> u64 {
    let mut entries: Vec<_> = file_meta.iter().collect();
    entries.sort_by_key(|(path, _)| path.as_str());

    let mut hasher = DefaultHasher::new();
    for (path, meta) in entries {
        path.hash(&mut hasher);
        meta.language.hash(&mut hasher);
        meta.line_count.hash(&mut hasher);
        meta.byte_size.hash(&mut hasher);
        meta.symbol_count.hash(&mut hasher);
        meta.modified_ms.hash(&mut hasher);
        meta.indexed.hash(&mut hasher);
    }
    hasher.finish()
}

pub fn read_snapshot(path: impl AsRef<Path>) -> Result<SnapshotData> {
    let path = path.as_ref();
    let file = fs::File::open(path)
        .with_context(|| format!("Failed to open snapshot file {}", path.display()))?;
    let mut reader = BufReader::new(file);

    let mut len_bytes = [0u8; 8];
    reader
        .read_exact(&mut len_bytes)
        .context("Failed to read snapshot length")?;
    let len = u64::from_le_bytes(len_bytes) as usize;

    if len > 500 * 1024 * 1024 {
        anyhow::bail!("Snapshot file too large: {} bytes", len);
    }

    let mut data = vec![0u8; len];
    reader
        .read_exact(&mut data)
        .context("Failed to read snapshot data")?;

    let snapshot: SnapshotData =
        bincode::deserialize(&data).context("Failed to deserialize snapshot")?;

    if snapshot.header.magic != *MAGIC {
        anyhow::bail!("Invalid snapshot magic");
    }

    if snapshot.header.version > FORMAT_VERSION {
        anyhow::bail!(
            "Snapshot version {} is newer than supported version {}",
            snapshot.header.version,
            FORMAT_VERSION
        );
    }

    Ok(snapshot)
}

pub fn load_snapshot_into_engine(engine: &mut Engine, path: impl AsRef<Path>) -> Result<usize> {
    let snapshot = read_snapshot(path)?;
    let count = snapshot.header.file_count as usize;
    engine.load_from_snapshot(snapshot);
    Ok(count)
}

pub struct SnapshotDataRaw {
    pub outlines: Vec<(String, crate::types::FileOutline)>,
    pub file_meta: Vec<(String, crate::types::FileMeta)>,
    pub contents: Vec<(String, String)>,
    pub forward_deps: Vec<(String, Vec<String>)>,
}

impl SnapshotData {
    pub fn into_raw(self) -> SnapshotDataRaw {
        SnapshotDataRaw {
            outlines: self.outlines,
            file_meta: self.file_meta,
            contents: self.contents,
            forward_deps: self.forward_deps,
        }
    }
}
