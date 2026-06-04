use anyhow::{bail, Context, Result};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathMode {
    Existing,
    Create,
}

pub fn normalize_project_path(root: &Path, input: &str, mode: PathMode) -> Result<String> {
    if input.trim().is_empty() {
        bail!("path must not be empty");
    }

    let root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize project root {}", root.display()))?;
    let input_path = Path::new(input);
    reject_parent_dirs(input_path)?;

    match mode {
        PathMode::Existing => normalize_existing_path(&root, input_path),
        PathMode::Create => normalize_create_path(&root, input_path),
    }
}

pub fn project_target_path(root: &Path, relative_path: &str) -> PathBuf {
    root.join(relative_path)
}

fn normalize_existing_path(root: &Path, input: &Path) -> Result<String> {
    let target = if input.is_absolute() {
        input.to_path_buf()
    } else {
        root.join(input)
    };
    if !target.exists() {
        bail!("file not found: {}", display_input_path(input));
    }
    let target = target
        .canonicalize()
        .with_context(|| format!("failed to canonicalize path {}", target.display()))?;
    path_under_root(root, &target)
}

fn normalize_create_path(root: &Path, input: &Path) -> Result<String> {
    let target = if input.is_absolute() {
        input.to_path_buf()
    } else {
        root.join(input)
    };

    let Some(parent) = target.parent() else {
        bail!("path must include a file name");
    };
    let Some(file_name) = target.file_name() else {
        bail!("path must include a file name");
    };
    if file_name.is_empty() {
        bail!("path must include a file name");
    }

    let parent = parent.canonicalize().with_context(|| {
        format!(
            "failed to canonicalize parent directory {}",
            parent.display()
        )
    })?;
    let mut relative = match parent.strip_prefix(root) {
        Ok(parent) => parent.to_path_buf(),
        Err(_) => bail!(
            "path {} is outside project root {}",
            parent.display(),
            root.display()
        ),
    };
    relative.push(file_name);
    path_to_project_string(&relative)
}

fn reject_parent_dirs(path: &Path) -> Result<()> {
    for component in path.components() {
        if matches!(component, Component::ParentDir) {
            bail!("path must not contain ..");
        }
    }
    Ok(())
}

fn path_under_root(root: &Path, target: &Path) -> Result<String> {
    let relative = target.strip_prefix(root).with_context(|| {
        format!(
            "path {} is outside project root {}",
            target.display(),
            root.display()
        )
    })?;
    path_to_project_string(relative)
}

fn path_to_project_string(path: &Path) -> Result<String> {
    let value = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    if value.is_empty() {
        bail!("path must refer to a file inside the project root");
    }
    Ok(value)
}

fn display_input_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("lexa-path-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        root
    }

    #[test]
    fn normalizes_relative_existing_path() {
        let root = temp_root("relative");

        let path = normalize_project_path(&root, "src/main.rs", PathMode::Existing).unwrap();

        assert_eq!(path, "src/main.rs");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn normalizes_absolute_existing_path_under_root() {
        let root = temp_root("absolute");
        let absolute = root.join("src/main.rs");

        let path =
            normalize_project_path(&root, absolute.to_str().unwrap(), PathMode::Existing).unwrap();

        assert_eq!(path, "src/main.rs");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_parent_dir_components() {
        let root = temp_root("parent");

        let err = normalize_project_path(&root, "../main.rs", PathMode::Existing).unwrap_err();

        assert!(err.to_string().contains(".."));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_absolute_path_outside_root() {
        let root = temp_root("outside");
        let outside = std::env::temp_dir().join(format!("lexa-outside-{}", std::process::id()));
        std::fs::write(&outside, "outside\n").unwrap();

        let err = normalize_project_path(&root, outside.to_str().unwrap(), PathMode::Existing)
            .unwrap_err();

        assert!(err.to_string().contains("outside project root"));
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_file(outside).unwrap();
    }

    #[test]
    fn missing_existing_path_reports_input_path() {
        let root = temp_root("missing");

        let err =
            normalize_project_path(&root, "nonexistent/file.ts", PathMode::Existing).unwrap_err();

        assert_eq!(err.to_string(), "file not found: nonexistent/file.ts");
        assert!(!err.to_string().contains(root.to_string_lossy().as_ref()));
        assert!(!err.to_string().contains("canonicalize"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_path_requires_parent_under_root() {
        let root = temp_root("create");

        let path = normalize_project_path(&root, "src/new.rs", PathMode::Create).unwrap();

        assert_eq!(path, "src/new.rs");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_path_allows_root_level_file() {
        let root = temp_root("create-root");

        let path = normalize_project_path(&root, "new.rs", PathMode::Create).unwrap();

        assert_eq!(path, "new.rs");
        std::fs::remove_dir_all(root).unwrap();
    }
}
