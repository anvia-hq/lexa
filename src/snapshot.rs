use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use crate::engine::Engine;
use crate::types::{
    EngineIndexSnapshot, EngineSnapshotData, FileMeta, FileOutline, UnresolvedImport,
};

const MAGIC: &[u8; 8] = b"LEXA\0\0\0\0";
const FORMAT_VERSION: u16 = 3;
const BINARY_V1_FORMAT_VERSION: u16 = 1;
const CHECKSUM_V2_FORMAT_VERSION: u16 = 2;
const MAX_SNAPSHOT_BYTES: usize = 500 * 1024 * 1024;

#[derive(Debug)]
struct SnapshotHeader {
    file_count: u32,
}

#[derive(Debug)]
pub struct SnapshotData {
    header: SnapshotHeader,
    outlines: Vec<(String, FileOutline)>,
    file_meta: Vec<(String, FileMeta)>,
    contents: Vec<(String, String)>,
    forward_deps: Vec<(String, Vec<String>)>,
    unresolved_imports: Vec<(String, Vec<UnresolvedImport>)>,
    indexes: Option<EngineIndexSnapshot>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotPayloadV3 {
    created_at: u64,
    outlines: Vec<(String, FileOutline)>,
    file_meta: Vec<(String, FileMeta)>,
    contents: Vec<(String, String)>,
    forward_deps: Vec<(String, Vec<String>)>,
    unresolved_imports: Vec<(String, Vec<UnresolvedImport>)>,
    indexes: EngineIndexSnapshot,
}

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotPayloadV2 {
    created_at: u64,
    root_hash: u64,
    outlines: Vec<(String, FileOutline)>,
    file_meta: Vec<(String, FileMeta)>,
    contents: Vec<(String, String)>,
    forward_deps: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacySnapshotHeader {
    magic: [u8; 8],
    version: u16,
    file_count: u32,
    created_at: u64,
    root_hash: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacySnapshotData {
    header: LegacySnapshotHeader,
    outlines: Vec<(String, FileOutline)>,
    file_meta: Vec<(String, FileMeta)>,
    contents: Vec<(String, String)>,
    forward_deps: Vec<(String, Vec<String>)>,
}

pub fn write_snapshot(engine: &Engine, output_path: impl AsRef<Path>) -> Result<()> {
    let data = engine.to_snapshot_data();
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let file_count = u32::try_from(data.file_meta.len()).context("Too many files to snapshot")?;
    let indexes = data
        .indexes
        .context("Current snapshots require hydrated engine indexes")?;
    let payload = SnapshotPayloadV3 {
        created_at,
        outlines: data.outlines,
        file_meta: data.file_meta,
        contents: data.contents,
        forward_deps: data.forward_deps,
        unresolved_imports: data.unresolved_imports,
        indexes,
    };
    let encoded = postcard::to_allocvec(&payload).context("Failed to serialize snapshot")?;
    checked_snapshot_len(encoded.len() as u64)?;
    let payload_hash = *blake3::hash(&encoded).as_bytes();

    let path = output_path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create snapshot directory {}", parent.display()))?;
    }

    let tmp_path = temp_snapshot_path(path);
    let write_result =
        write_snapshot_file(&tmp_path, file_count, created_at, &payload_hash, &encoded);
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

fn write_snapshot_file(
    path: &Path,
    file_count: u32,
    created_at: u64,
    payload_hash: &[u8; 32],
    encoded: &[u8],
) -> Result<()> {
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("Failed to create snapshot file {}", path.display()))?;
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
        .write_all(payload_hash)
        .context("Failed to write snapshot BLAKE3 hash")?;
    writer
        .write_all(&(encoded.len() as u64).to_le_bytes())
        .context("Failed to write snapshot length")?;
    writer
        .write_all(encoded)
        .context("Failed to write snapshot data")?;
    writer.flush().context("Failed to flush snapshot data")?;
    writer
        .get_ref()
        .sync_all()
        .context("Failed to sync snapshot data")?;
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
        read_header_prefixed_snapshot(reader)
    } else {
        read_v09_snapshot(reader, first_bytes)
    }
}

fn read_header_prefixed_snapshot(mut reader: BufReader<fs::File>) -> Result<SnapshotData> {
    let version = read_u16(&mut reader, "version")?;
    if version > FORMAT_VERSION {
        anyhow::bail!(
            "Snapshot version {} is newer than supported version {}",
            version,
            FORMAT_VERSION
        );
    }
    if version == 0 {
        anyhow::bail!("Unsupported snapshot version 0");
    }

    let file_count = read_u32(&mut reader, "file count")?;
    let _created_at = read_u64(&mut reader, "timestamp")?;
    match version {
        BINARY_V1_FORMAT_VERSION | CHECKSUM_V2_FORMAT_VERSION => {
            let legacy_hash = read_u64(&mut reader, "root hash")?;
            let encoded = read_payload_bytes(&mut reader)?;
            let payload = decode_v1_v2_payload(version, legacy_hash, &encoded)?;
            snapshot_from_v2_payload(file_count, payload)
        }
        FORMAT_VERSION => {
            let mut expected_hash = [0u8; 32];
            reader
                .read_exact(&mut expected_hash)
                .context("Failed to read snapshot BLAKE3 hash")?;
            let encoded = read_payload_bytes(&mut reader)?;
            let actual_hash = *blake3::hash(&encoded).as_bytes();
            if actual_hash != expected_hash {
                anyhow::bail!(
                    "Snapshot checksum mismatch: expected {}, actual {}",
                    hex_hash(&expected_hash),
                    hex_hash(&actual_hash)
                );
            }
            let payload: SnapshotPayloadV3 =
                postcard::from_bytes(&encoded).context("Failed to deserialize v3 snapshot")?;
            if payload.file_meta.len() != file_count as usize {
                anyhow::bail!(
                    "Snapshot file count mismatch: header {file_count}, payload {}",
                    payload.file_meta.len()
                );
            }
            Ok(SnapshotData {
                header: SnapshotHeader { file_count },
                outlines: payload.outlines,
                file_meta: payload.file_meta,
                contents: payload.contents,
                forward_deps: payload.forward_deps,
                unresolved_imports: payload.unresolved_imports,
                indexes: Some(payload.indexes),
            })
        }
        _ => anyhow::bail!("Unsupported snapshot version {version}"),
    }
}

fn decode_v1_v2_payload(
    version: u16,
    expected_checksum: u64,
    encoded: &[u8],
) -> Result<SnapshotPayloadV2> {
    match version {
        BINARY_V1_FORMAT_VERSION => {
            bincode::deserialize(encoded).context("Failed to deserialize v1 snapshot")
        }
        CHECKSUM_V2_FORMAT_VERSION => {
            let actual_checksum = checksum_v2(encoded);
            if actual_checksum != expected_checksum {
                anyhow::bail!(
                    "Snapshot checksum mismatch: expected {expected_checksum:016x}, actual {actual_checksum:016x}"
                );
            }
            postcard::from_bytes(encoded).context("Failed to deserialize v2 snapshot")
        }
        _ => anyhow::bail!("Unsupported legacy snapshot version {version}"),
    }
}

fn snapshot_from_v2_payload(file_count: u32, payload: SnapshotPayloadV2) -> Result<SnapshotData> {
    if payload.file_meta.len() != file_count as usize {
        anyhow::bail!(
            "Snapshot file count mismatch: header {file_count}, payload {}",
            payload.file_meta.len()
        );
    }
    Ok(SnapshotData {
        header: SnapshotHeader { file_count },
        outlines: payload.outlines,
        file_meta: payload.file_meta,
        contents: payload.contents,
        forward_deps: payload.forward_deps,
        unresolved_imports: Vec::new(),
        indexes: None,
    })
}

fn read_v09_snapshot(mut reader: BufReader<fs::File>, len_bytes: [u8; 8]) -> Result<SnapshotData> {
    let len = checked_snapshot_len(u64::from_le_bytes(len_bytes))?;
    let mut encoded = vec![0u8; len];
    reader
        .read_exact(&mut encoded)
        .context("Failed to read v0.9 snapshot data")?;
    let snapshot: LegacySnapshotData =
        bincode::deserialize(&encoded).context("Failed to deserialize v0.9 snapshot")?;
    if snapshot.header.magic != *MAGIC {
        anyhow::bail!("Invalid snapshot magic");
    }
    if snapshot.header.version > BINARY_V1_FORMAT_VERSION {
        anyhow::bail!(
            "Legacy snapshot version {} is not supported",
            snapshot.header.version
        );
    }
    if snapshot.file_meta.len() != snapshot.header.file_count as usize {
        anyhow::bail!(
            "Snapshot file count mismatch: header {}, payload {}",
            snapshot.header.file_count,
            snapshot.file_meta.len()
        );
    }

    Ok(SnapshotData {
        header: SnapshotHeader {
            file_count: snapshot.header.file_count,
        },
        outlines: snapshot.outlines,
        file_meta: snapshot.file_meta,
        contents: snapshot.contents,
        forward_deps: snapshot.forward_deps,
        unresolved_imports: Vec::new(),
        indexes: None,
    })
}

fn read_payload_bytes(reader: &mut BufReader<fs::File>) -> Result<Vec<u8>> {
    let len = checked_snapshot_len(read_u64(reader, "length")?)?;
    let mut encoded = vec![0u8; len];
    reader
        .read_exact(&mut encoded)
        .context("Failed to read snapshot data")?;
    Ok(encoded)
}

fn read_u16(reader: &mut impl Read, field: &str) -> Result<u16> {
    let mut bytes = [0u8; 2];
    reader
        .read_exact(&mut bytes)
        .with_context(|| format!("Failed to read snapshot {field}"))?;
    Ok(u16::from_le_bytes(bytes))
}

fn read_u32(reader: &mut impl Read, field: &str) -> Result<u32> {
    let mut bytes = [0u8; 4];
    reader
        .read_exact(&mut bytes)
        .with_context(|| format!("Failed to read snapshot {field}"))?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(reader: &mut impl Read, field: &str) -> Result<u64> {
    let mut bytes = [0u8; 8];
    reader
        .read_exact(&mut bytes)
        .with_context(|| format!("Failed to read snapshot {field}"))?;
    Ok(u64::from_le_bytes(bytes))
}

fn checksum_v2(payload: &[u8]) -> u64 {
    let hash = blake3::hash(payload);
    let prefix: [u8; 8] = hash.as_bytes()[..8]
        .try_into()
        .expect("BLAKE3 hashes always contain eight bytes");
    u64::from_le_bytes(prefix)
}

fn hex_hash(hash: &[u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
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
            unresolved_imports: self.unresolved_imports,
            indexes: self.indexes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;

    fn sample_engine() -> Engine {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/main.rs",
            "use crate::dep;\nfn main() { dep::run(); }\n",
        );
        engine.index_file("src/dep.rs", "pub fn run() {}\n");
        engine
    }

    #[test]
    fn snapshot_round_trip_uses_v3_with_full_blake3_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.lexa");
        write_snapshot(&sample_engine(), &path).unwrap();

        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[..MAGIC.len()], MAGIC);
        assert_eq!(
            u16::from_le_bytes(bytes[8..10].try_into().unwrap()),
            FORMAT_VERSION
        );
        let mut loaded = Engine::new(4);
        let count = load_snapshot_into_engine(&mut loaded, &path).unwrap();
        assert_eq!(count, 2);
        assert!(!loaded.find_symbol("main").is_empty());
        assert!(!loaded.search("dep::run", 10).is_empty());
        assert_eq!(loaded.get_depends_on("src/main.rs"), vec!["src/dep.rs"]);
    }

    #[test]
    fn snapshot_v3_rejects_payload_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.lexa");
        write_snapshot(&sample_engine(), &path).unwrap();

        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 0xff;
        fs::write(&path, bytes).unwrap();

        let err = read_snapshot(&path).unwrap_err();
        assert!(err.to_string().contains("checksum mismatch"));
    }

    #[test]
    fn snapshot_v3_preserves_unresolved_imports_without_reresolving() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.lexa");
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/app.ts",
            "import { missing } from './missing';\nmissing();\n",
        );
        write_snapshot(&engine, &path).unwrap();

        let mut loaded = Engine::new(4);
        load_snapshot_into_engine(&mut loaded, &path).unwrap();

        let unresolved = loaded.get_unresolved_imports("src/app.ts");
        assert_eq!(unresolved.len(), 1);
        assert_eq!(unresolved[0].import, "./missing");
    }

    #[test]
    fn snapshot_reader_accepts_v2_postcard_payloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.lexa");
        let data = sample_engine().to_snapshot_data();
        let payload = SnapshotPayloadV2 {
            created_at: 2,
            root_hash: 0,
            outlines: data.outlines,
            file_meta: data.file_meta,
            contents: data.contents,
            forward_deps: data.forward_deps,
        };
        let encoded = postcard::to_allocvec(&payload).unwrap();
        write_v1_v2_fixture(
            &path,
            CHECKSUM_V2_FORMAT_VERSION,
            payload.file_meta.len() as u32,
            checksum_v2(&encoded),
            &encoded,
        );

        let mut loaded = Engine::new(4);
        assert_eq!(load_snapshot_into_engine(&mut loaded, &path).unwrap(), 2);
        assert!(!loaded.find_symbol("main").is_empty());
        assert_eq!(loaded.get_depends_on("src/main.rs"), vec!["src/dep.rs"]);
    }

    #[test]
    fn snapshot_reader_accepts_v1_binary_payloads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.lexa");
        let data = sample_engine().to_snapshot_data();
        let payload = SnapshotPayloadV2 {
            created_at: 1,
            root_hash: 0,
            outlines: data.outlines,
            file_meta: data.file_meta,
            contents: data.contents,
            forward_deps: data.forward_deps,
        };
        let encoded = bincode::serialize(&payload).unwrap();
        write_v1_v2_fixture(
            &path,
            BINARY_V1_FORMAT_VERSION,
            payload.file_meta.len() as u32,
            0,
            &encoded,
        );

        let mut loaded = Engine::new(4);
        assert_eq!(load_snapshot_into_engine(&mut loaded, &path).unwrap(), 2);
        assert!(!loaded.find_symbol("main").is_empty());
        assert_eq!(loaded.get_depends_on("src/main.rs"), vec!["src/dep.rs"]);
    }

    #[test]
    fn snapshot_reader_accepts_v09_length_prefixed_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("graph.lexa");
        let data = sample_engine().to_snapshot_data();
        let file_count = data.file_meta.len() as u32;
        let snapshot = LegacySnapshotData {
            header: LegacySnapshotHeader {
                magic: *MAGIC,
                version: BINARY_V1_FORMAT_VERSION,
                file_count,
                created_at: 1,
                root_hash: 0,
            },
            outlines: data.outlines,
            file_meta: data.file_meta,
            contents: data.contents,
            forward_deps: data.forward_deps,
        };
        let encoded = bincode::serialize(&snapshot).unwrap();
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(&(encoded.len() as u64).to_le_bytes())
            .unwrap();
        file.write_all(&encoded).unwrap();
        drop(file);

        let mut loaded = Engine::new(4);
        assert_eq!(load_snapshot_into_engine(&mut loaded, &path).unwrap(), 2);
        assert!(!loaded.find_symbol("main").is_empty());
        assert_eq!(loaded.get_depends_on("src/main.rs"), vec!["src/dep.rs"]);
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

        let err = read_snapshot(&path).unwrap_err();
        assert!(err.to_string().contains("newer than supported"));
    }

    fn write_v1_v2_fixture(
        path: &Path,
        version: u16,
        file_count: u32,
        root_hash: u64,
        encoded: &[u8],
    ) {
        let mut file = fs::File::create(path).unwrap();
        file.write_all(MAGIC).unwrap();
        file.write_all(&version.to_le_bytes()).unwrap();
        file.write_all(&file_count.to_le_bytes()).unwrap();
        file.write_all(&1u64.to_le_bytes()).unwrap();
        file.write_all(&root_hash.to_le_bytes()).unwrap();
        file.write_all(&(encoded.len() as u64).to_le_bytes())
            .unwrap();
        file.write_all(encoded).unwrap();
    }
}
