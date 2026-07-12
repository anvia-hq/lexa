use crate::types::{detect_language, Language};
use ignore::WalkBuilder;
use std::path::Path;

const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    ".zig-cache",
    "zig-out",
    ".next",
    ".nuxt",
    ".svelte-kit",
    "dist",
    "build",
    "__pycache__",
    ".venv",
    "venv",
    "target",
    ".gradle",
    ".idea",
    ".vscode",
    "vendor",
    "Pods",
    ".cargo",
    "pkg",
    "bin",
    "obj",
    ".cache",
    ".turbo",
    ".parcel-cache",
    "coverage",
    ".pytest_cache",
    ".mypy_cache",
    ".tox",
    ".eggs",
    "*.egg-info",
];

const SKIP_FILES: &[&str] = &[
    ".DS_Store",
    "Thumbs.db",
    "package-lock.json",
    "yarn.lock",
    "pnpm-lock.yaml",
    "Cargo.lock",
    "composer.lock",
    "Gemfile.lock",
    "poetry.lock",
    "go.sum",
];

pub struct WalkedFile {
    pub path: String,
    pub content: String,
    pub modified_ms: u64,
}

pub struct WalkedFileMeta {
    pub path: String,
    pub modified_ms: u64,
    pub byte_size: u64,
    pub indexable: bool,
    pub change_ns: Option<u128>,
}

pub fn walk_project_meta(root: impl AsRef<Path>) -> Vec<WalkedFileMeta> {
    let mut files = Vec::new();
    let root = root.as_ref();

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .follow_links(false)
        .same_file_system(true);

    for entry in builder.build() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.file_type().is_none_or(|ft| ft.is_dir()) {
            continue;
        }

        if let Some(meta) = walked_file_meta(root, entry.path()) {
            files.push(meta);
        }
    }

    files
}

pub fn walk_single_file(root: impl AsRef<Path>, path: impl AsRef<Path>) -> Option<WalkedFile> {
    let root = root.as_ref();
    let path = path.as_ref();
    let meta = walked_file_meta(root, path)?;
    if !meta.indexable {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    Some(WalkedFile {
        path: meta.path,
        content,
        modified_ms: meta.modified_ms,
    })
}

pub fn walk_single_file_meta(
    root: impl AsRef<Path>,
    path: impl AsRef<Path>,
) -> Option<WalkedFileMeta> {
    walked_file_meta(root.as_ref(), path.as_ref())
}

pub fn relative_path(root: impl AsRef<Path>, path: impl AsRef<Path>) -> Option<String> {
    let root = root.as_ref();
    let path = path.as_ref();
    path.strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().to_string())
}

fn walked_file_meta(root: &Path, path: &Path) -> Option<WalkedFileMeta> {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if SKIP_FILES.contains(&name) {
            return None;
        }
    }

    let relative_path = path.strip_prefix(root).unwrap_or(path);
    if should_skip_path(relative_path) {
        return None;
    }
    let relative = relative_path.to_string_lossy().to_string();

    let language = detect_language(&relative);
    let mut indexable = true;
    if language == Language::Unknown {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let ext_lower = ext.to_lowercase();
            if is_binary_extension(&ext_lower) {
                if is_resolvable_asset_extension(&ext_lower) {
                    indexable = false;
                } else {
                    return None;
                }
            }
        }
    }

    let metadata = path.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_SIZE {
        return None;
    }

    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();

    Some(WalkedFileMeta {
        path: relative,
        modified_ms,
        byte_size: metadata.len(),
        indexable,
        change_ns: metadata_change_ns(&metadata),
    })
}

#[cfg(unix)]
fn metadata_change_ns(metadata: &std::fs::Metadata) -> Option<u128> {
    use std::os::unix::fs::MetadataExt;

    let seconds = u128::try_from(metadata.ctime()).ok()?;
    let nanos = u128::try_from(metadata.ctime_nsec()).ok()?;
    Some(seconds.saturating_mul(1_000_000_000).saturating_add(nanos))
}

#[cfg(not(unix))]
fn metadata_change_ns(_metadata: &std::fs::Metadata) -> Option<u128> {
    None
}

fn should_skip_path(path: &Path) -> bool {
    for component in path.components() {
        if let Some(name) = component.as_os_str().to_str() {
            if is_hidden_component(name) {
                return true;
            }
            if SKIP_DIRS.contains(&name) {
                return true;
            }
        }
    }
    false
}

fn is_hidden_component(name: &str) -> bool {
    name.starts_with('.') && name != "." && name != ".."
}

fn is_binary_extension(ext: &str) -> bool {
    matches!(
        ext,
        "exe"
            | "dll"
            | "so"
            | "dylib"
            | "o"
            | "a"
            | "lib"
            | "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "ico"
            | "webp"
            | "avif"
            | "mp3"
            | "mp4"
            | "wav"
            | "avi"
            | "mov"
            | "mkv"
            | "flv"
            | "wmv"
            | "zip"
            | "tar"
            | "gz"
            | "bz2"
            | "xz"
            | "7z"
            | "rar"
            | "pdf"
            | "doc"
            | "docx"
            | "xls"
            | "xlsx"
            | "ppt"
            | "pptx"
            | "woff"
            | "woff2"
            | "ttf"
            | "otf"
            | "eot"
            | "pyc"
            | "pyo"
            | "class"
            | "jar"
            | "min.js"
            | "min.css"
            | "wasm"
            | "node"
    )
}

fn is_resolvable_asset_extension(ext: &str) -> bool {
    matches!(
        ext,
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "webp" | "avif"
    )
}
