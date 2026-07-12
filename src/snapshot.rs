use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::engine::Engine;
use crate::types::EngineSnapshotData;

const MAGIC: &[u8; 8] = b"LEXA\0\0\0\0";
const FORMAT_VERSION: u16 = 2;
const BINARY_V1_FORMAT_VERSION: u16 = 1;
const MAX_SNAPSHOT_BYTES: usize = 500 * 1024 * 1024;

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

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotPayload {
    created_at: u64,
    root_hash: u64,
    outlines: Vec<(String, crate::types::FileOutline)>,
    file_meta: Vec<(String, crate::types::FileMeta)>,
    contents: Vec<(String, String)>,
    forward_deps: Vec<(String, Vec<String>)>,
}

pub fn write_snapshot(engine: &Engine, output_path: impl AsRef<Path>) -> Result<()> {
    let data = engine.to_snapshot_data();
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let file_count = data.file_meta.len() as u32;

    let payload = SnapshotPayload {
        created_at,
        root_hash: 0,
        outlines: data.outlines,
        file_meta: data.file_meta,
        contents: data.contents,
        forward_deps: data.forward_deps,
    };

    let encoded = postcard::to_allocvec(&payload).context("Failed to serialize snapshot")?;
    let root_hash = payload_checksum(&encoded);

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
            .write_all(MAGIC)
            .context("Failed to write snapshot magic")?;
        writer
            .write_all(&FORMAT_VERSION.to_le_bytes())
            .context("Failed to write snapshot version")?;
        writer
            .write_all(&file_count.to_le_bytes())
            .context("Failed to write snapshot file count")?;
        writer
            .write_all(&created_at.to_le_bytes())
            .context("Failed to write snapshot timestamp")?;
        writer
            .write_all(&root_hash.to_le_bytes())
            .context("Failed to write snapshot root hash")?;
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

fn payload_checksum(payload: &[u8]) -> u64 {
    let hash = blake3::hash(payload);
    let prefix: [u8; 8] = hash.as_bytes()[..8]
        .try_into()
        .expect("BLAKE3 hashes always contain eight bytes");
    u64::from_le_bytes(prefix)
}

pub fn read_snapshot(path: impl AsRef<Path>) -> Result<SnapshotData> {
    let path = path.as_ref();
    let file = fs::File::open(path)
        .with_context(|| format!("Failed to open snapshot file {}", path.display()))?;
    let mut reader = BufReader::new(file);

    let mut first_bytes = [0u8; 8];
    reader
        .read_exact(&mut first_bytes)
        .context("Failed to read snapshot header")?;
    if &first_bytes == MAGIC {
        return read_current_snapshot(reader);
    }

    read_legacy_snapshot(reader, first_bytes)
}

fn read_current_snapshot(mut reader: BufReader<fs::File>) -> Result<SnapshotData> {
    let mut version_bytes = [0u8; 2];
    reader
        .read_exact(&mut version_bytes)
        .context("Failed to read snapshot version")?;
    let version = u16::from_le_bytes(version_bytes);
    if version > FORMAT_VERSION {
        anyhow::bail!(
            "Snapshot version {} is newer than supported version {}",
            version,
            FORMAT_VERSION
        );
    }

    let mut file_count_bytes = [0u8; 4];
    reader
        .read_exact(&mut file_count_bytes)
        .context("Failed to read snapshot file count")?;
    let file_count = u32::from_le_bytes(file_count_bytes);

    let mut created_at_bytes = [0u8; 8];
    reader
        .read_exact(&mut created_at_bytes)
        .context("Failed to read snapshot timestamp")?;
    let created_at = u64::from_le_bytes(created_at_bytes);

    let mut root_hash_bytes = [0u8; 8];
    reader
        .read_exact(&mut root_hash_bytes)
        .context("Failed to read snapshot root hash")?;
    let root_hash = u64::from_le_bytes(root_hash_bytes);

    let payload = read_payload(reader, version, root_hash)?;
    Ok(SnapshotData {
        header: SnapshotHeader {
            magic: *MAGIC,
            version,
            file_count,
            created_at,
            root_hash,
        },
        outlines: payload.outlines,
        file_meta: payload.file_meta,
        contents: payload.contents,
        forward_deps: payload.forward_deps,
    })
}

fn read_legacy_snapshot(
    mut reader: BufReader<fs::File>,
    len_bytes: [u8; 8],
) -> Result<SnapshotData> {
    let len = checked_snapshot_len(u64::from_le_bytes(len_bytes))?;
    let mut data = vec![0u8; len];
    reader
        .read_exact(&mut data)
        .context("Failed to read legacy snapshot data")?;

    let snapshot: SnapshotData =
        bincode::deserialize(&data).context("Failed to deserialize legacy snapshot")?;

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

fn read_payload(
    mut reader: BufReader<fs::File>,
    version: u16,
    expected_checksum: u64,
) -> Result<SnapshotPayload> {
    let mut len_bytes = [0u8; 8];
    reader
        .read_exact(&mut len_bytes)
        .context("Failed to read snapshot length")?;
    let len = checked_snapshot_len(u64::from_le_bytes(len_bytes))?;

    let mut data = vec![0u8; len];
    reader
        .read_exact(&mut data)
        .context("Failed to read snapshot data")?;

    match version {
        BINARY_V1_FORMAT_VERSION => {
            bincode::deserialize(&data).context("Failed to deserialize v1 snapshot")
        }
        FORMAT_VERSION => {
            let actual_checksum = payload_checksum(&data);
            if actual_checksum != expected_checksum {
                anyhow::bail!(
                    "Snapshot checksum mismatch: expected {expected_checksum:016x}, actual {actual_checksum:016x}"
                );
            }
            postcard::from_bytes(&data).context("Failed to deserialize v2 snapshot")
        }
        _ => anyhow::bail!("Unsupported snapshot version {version}"),
    }
}

fn checked_snapshot_len(len: u64) -> Result<usize> {
    if len > MAX_SNAPSHOT_BYTES as u64 {
        anyhow::bail!("Snapshot file too large: {} bytes", len);
    }
    Ok(len as usize)
}

pub fn load_snapshot_into_engine(engine: &mut Engine, path: impl AsRef<Path>) -> Result<usize> {
    let path = path.as_ref();
    let snapshot = read_snapshot(path)?;
    let count = snapshot.header.file_count as usize;
    engine.load_snapshot_data(snapshot.into_engine_data());
    engine.set_freshness_watermark(snapshot_modified_ns(path));
    Ok(count)
}

fn snapshot_modified_ns(path: &Path) -> Option<u128> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_nanos())
}

impl SnapshotData {
    pub fn into_engine_data(self) -> EngineSnapshotData {
        EngineSnapshotData {
            outlines: self.outlines,
            file_meta: self.file_meta,
            contents: self.contents,
            forward_deps: self.forward_deps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use std::io::Write;

    #[test]
    fn snapshot_round_trip_uses_header_prefixed_format() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.lexa");
        let mut engine = Engine::new(4);
        engine.index_file("src/main.rs", "fn main() {}\n");

        write_snapshot(&engine, &path).unwrap();

        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..MAGIC.len()], MAGIC);
        let mut loaded = Engine::new(4);
        let count = load_snapshot_into_engine(&mut loaded, &path).unwrap();
        assert_eq!(count, 1);
        assert!(!loaded.find_symbol("main").is_empty());
    }

    #[test]
    fn snapshot_v2_rejects_payload_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.lexa");
        let mut engine = Engine::new(4);
        engine.index_file("src/main.rs", "fn main() {}\n");
        write_snapshot(&engine, &path).unwrap();

        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.last_mut().unwrap();
        *last ^= 0xff;
        std::fs::write(&path, bytes).unwrap();

        let err = read_snapshot(&path).unwrap_err();
        assert!(err.to_string().contains("checksum mismatch"));
    }

    #[test]
    fn snapshot_reader_accepts_v1_binary_payloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.lexa");
        let mut engine = Engine::new(4);
        engine.index_file("src/main.rs", "fn main() {}\n");
        let data = engine.to_snapshot_data();
        let payload = SnapshotPayload {
            created_at: 1,
            root_hash: 0,
            outlines: data.outlines,
            file_meta: data.file_meta,
            contents: data.contents,
            forward_deps: data.forward_deps,
        };
        let encoded = bincode::serialize(&payload).unwrap();
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(MAGIC).unwrap();
        file.write_all(&BINARY_V1_FORMAT_VERSION.to_le_bytes())
            .unwrap();
        file.write_all(&(payload.file_meta.len() as u32).to_le_bytes())
            .unwrap();
        file.write_all(&1u64.to_le_bytes()).unwrap();
        file.write_all(&0u64.to_le_bytes()).unwrap();
        file.write_all(&(encoded.len() as u64).to_le_bytes())
            .unwrap();
        file.write_all(&encoded).unwrap();
        drop(file);

        let mut loaded = Engine::new(4);
        let count = load_snapshot_into_engine(&mut loaded, &path).unwrap();

        assert_eq!(count, 1);
        assert!(!loaded.find_symbol("main").is_empty());
    }

    #[test]
    fn snapshot_rejects_newer_header_version_before_payload_decode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.lexa");
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(MAGIC).unwrap();
        file.write_all(&(FORMAT_VERSION + 1).to_le_bytes()).unwrap();
        file.write_all(&0u32.to_le_bytes()).unwrap();
        file.write_all(&0u64.to_le_bytes()).unwrap();
        file.write_all(&0u64.to_le_bytes()).unwrap();
        file.write_all(&0u64.to_le_bytes()).unwrap();

        let err = read_snapshot(&path).unwrap_err();

        assert!(err.to_string().contains("newer than supported"));
    }
}
