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

pub fn walk_project(root: impl AsRef<Path>) -> Vec<WalkedFile> {
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

        let path = entry.path();

        if should_skip_path(path) {
            continue;
        }

        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if SKIP_FILES.contains(&name) {
                continue;
            }
        }

        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();

        let language = detect_language(&relative);
        if language == Language::Unknown {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext_lower = ext.to_lowercase();
                if is_binary_extension(&ext_lower) {
                    continue;
                }
            }
        }

        let modified_ms = if let Ok(metadata) = entry.metadata() {
            if metadata.len() > MAX_FILE_SIZE {
                continue;
            }
            metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or_default()
        } else {
            0
        };

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        files.push(WalkedFile {
            path: relative,
            content,
            modified_ms,
        });
    }

    files
}

fn should_skip_path(path: &Path) -> bool {
    for component in path.components() {
        if let Some(name) = component.as_os_str().to_str() {
            if SKIP_DIRS.contains(&name) {
                return true;
            }
        }
    }
    false
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
            | "svg"
            | "webp"
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
