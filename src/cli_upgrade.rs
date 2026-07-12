use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LEXA_REPO: &str = "anvia-hq/lexa";
const VERSION_CACHE_TTL_SECS: u64 = 60 * 60 * 12;

pub(crate) fn cmd_version(json_output: bool) -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    let latest = latest_version_with_cache();
    let update_available = latest
        .as_deref()
        .is_some_and(|latest| version_is_newer(latest, current));

    if json_output {
        return print_json(json!({
            "version": current,
            "latest": latest,
            "update_available": update_available,
        }));
    }

    println!("lexa {current}");
    if update_available {
        if let Some(latest) = latest {
            println!("update available: {latest}");
            println!("run: lexa upgrade");
        }
    }
    Ok(())
}

pub(crate) fn cmd_upgrade(
    version: &str,
    install_dir: Option<&PathBuf>,
    json_output: bool,
) -> Result<()> {
    validate_upgrade_version(version)?;
    let install_dir = upgrade_install_dir(install_dir)?;

    if cfg!(windows) {
        return cmd_upgrade_windows(version, &install_dir, json_output);
    }

    cmd_upgrade_unix(version, &install_dir, json_output)
}

#[derive(Debug, Deserialize, Serialize)]
struct VersionCache {
    checked_at: u64,
    latest: String,
}

fn latest_version_with_cache() -> Option<String> {
    if update_check_disabled() {
        return None;
    }

    let cache_path = version_cache_path()?;
    let now = unix_secs();
    if let Ok(content) = std::fs::read_to_string(&cache_path) {
        if let Ok(cache) = serde_json::from_str::<VersionCache>(&content) {
            if now.saturating_sub(cache.checked_at) <= VERSION_CACHE_TTL_SECS {
                return Some(cache.latest);
            }
        }
    }

    let latest = fetch_latest_release_tag()?;
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let cache = VersionCache {
        checked_at: now,
        latest: latest.clone(),
    };
    if let Ok(content) = serde_json::to_string(&cache) {
        let _ = std::fs::write(cache_path, content);
    }
    Some(latest)
}

fn update_check_disabled() -> bool {
    std::env::var_os("LEXA_NO_UPDATE_CHECK").is_some()
        || std::env::var_os("LEXA_SKIP_UPDATE_CHECK").is_some()
        || std::env::var_os("NO_UPDATE_CHECK").is_some()
}

fn version_cache_path() -> Option<PathBuf> {
    if let Some(cache_home) = std::env::var_os("XDG_CACHE_HOME") {
        return Some(PathBuf::from(cache_home).join("lexa/version-check.json"));
    }
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        return Some(PathBuf::from(local_app_data).join("lexa/version-check.json"));
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".cache/lexa/version-check.json"))
}

fn fetch_latest_release_tag() -> Option<String> {
    let url = format!("https://api.github.com/repos/{LEXA_REPO}/releases/latest");
    let output = fetch_url(&url)?;
    extract_json_string_field(&output, "tag_name")
}

#[cfg(not(windows))]
fn fetch_url(url: &str) -> Option<String> {
    let output = std::process::Command::new("curl")
        .arg("-fsSL")
        .arg("--max-time")
        .arg("1")
        .arg("-H")
        .arg("User-Agent: lexa")
        .arg(url)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[cfg(windows)]
fn fetch_url(url: &str) -> Option<String> {
    let output = std::process::Command::new("powershell")
        .arg("-NoProfile")
        .arg("-Command")
        .arg(format!(
            "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; \
             (Invoke-WebRequest -UseBasicParsing -TimeoutSec 1 -Headers @{{'User-Agent'='lexa'}} -Uri '{}').Content",
            url.replace('\'', "''")
        ))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn extract_json_string_field(json: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let after_field = json.split(&needle).nth(1)?;
    let after_colon = after_field.split_once(':')?.1.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let end = value.find('"')?;
    Some(value[..end].to_string())
}

fn version_is_newer(candidate: &str, current: &str) -> bool {
    let candidate = parse_version_numbers(candidate);
    let current = parse_version_numbers(current);
    candidate > current
}

fn parse_version_numbers(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches('v')
        .split(['.', '-', '+'])
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

#[cfg(not(windows))]
fn cmd_upgrade_unix(version: &str, install_dir: &Path, json_output: bool) -> Result<()> {
    let tag = release_tag(version);
    let installer_url = if version == "latest" {
        format!("https://github.com/{LEXA_REPO}/releases/latest/download/install.sh")
    } else {
        format!("https://github.com/{LEXA_REPO}/releases/download/{tag}/install.sh")
    };
    let script = r#"curl -fsSL "$1" | sh -s -- "$2""#;
    let mut command = std::process::Command::new("sh");
    command
        .arg("-c")
        .arg(script)
        .arg("lexa-upgrade")
        .arg(installer_url)
        .arg(version)
        .env("LEXA_INSTALL_DIR", install_dir);

    if json_output {
        let output = command.output().context("failed to run Lexa upgrade")?;
        if !output.status.success() {
            bail!(
                "Lexa upgrade failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        return print_json(json!({
            "operation": "upgrade",
            "version": version,
            "install_dir": install_dir.display().to_string(),
            "status": "ok",
            "stdout": String::from_utf8_lossy(&output.stdout),
        }));
    }

    println!("Upgrading Lexa binary to {version}...");
    println!("Install directory: {}", install_dir.display());
    println!("Note: project indexes are updated with 'lexa index', not 'lexa upgrade'.");
    let status = command.status().context("failed to run Lexa upgrade")?;
    if !status.success() {
        bail!("Lexa upgrade failed with status {status}");
    }
    Ok(())
}

#[cfg(windows)]
fn cmd_upgrade_unix(_version: &str, _install_dir: &Path, _json_output: bool) -> Result<()> {
    unreachable!("cmd_upgrade_unix is only called on non-Windows platforms")
}

#[cfg(windows)]
fn cmd_upgrade_windows(version: &str, install_dir: &Path, json_output: bool) -> Result<()> {
    if json_output {
        bail!("JSON output is not supported for Windows deferred upgrades");
    }

    let pid = std::process::id();
    let escaped_version = version.replace('\'', "''");
    let escaped_install_dir = install_dir.display().to_string().replace('\'', "''");
    let tag = release_tag(version);
    let installer_url = if version == "latest" {
        format!("https://github.com/{LEXA_REPO}/releases/latest/download/install.ps1")
    } else {
        format!("https://github.com/{LEXA_REPO}/releases/download/{tag}/install.ps1")
    };
    let command = format!(
        "Wait-Process -Id {pid}; $env:LEXA_VERSION = '{escaped_version}'; $env:LEXA_INSTALL_DIR = '{escaped_install_dir}'; irm '{installer_url}' | iex"
    );

    std::process::Command::new("powershell")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(command)
        .spawn()
        .context("failed to start Lexa upgrade")?;

    println!("Started Lexa binary upgrade to {version}.");
    println!("Install directory: {}", install_dir.display());
    println!("The updater will run after this lexa.exe process exits.");
    println!("Note: project indexes are updated with 'lexa index', not 'lexa upgrade'.");
    Ok(())
}

#[cfg(not(windows))]
fn cmd_upgrade_windows(_version: &str, _install_dir: &Path, _json_output: bool) -> Result<()> {
    unreachable!("cmd_upgrade_windows is only called on Windows platforms")
}

fn upgrade_install_dir(install_dir: Option<&PathBuf>) -> Result<PathBuf> {
    if let Some(dir) = install_dir {
        return Ok(dir.clone());
    }
    if let Some(dir) = std::env::var_os("LEXA_INSTALL_DIR") {
        return Ok(PathBuf::from(dir));
    }

    let current_exe = std::env::current_exe().context("failed to locate current lexa binary")?;
    current_exe
        .parent()
        .map(Path::to_path_buf)
        .context("failed to locate current lexa binary directory")
}

fn validate_upgrade_version(version: &str) -> Result<()> {
    if version.is_empty() {
        bail!("upgrade version must not be empty");
    }
    if version
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        return Ok(());
    }
    bail!("upgrade version must contain only letters, numbers, '.', '_', or '-'")
}

fn release_tag(version: &str) -> String {
    if version == "latest" || version.starts_with('v') {
        version.to_string()
    } else {
        format!("v{version}")
    }
}

fn print_json(value: serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_version_validation_allows_release_tags() {
        assert!(validate_upgrade_version("latest").is_ok());
        assert!(validate_upgrade_version("v0.1.0").is_ok());
        assert!(validate_upgrade_version("0.1.0").is_ok());
    }

    #[test]
    fn upgrade_version_validation_rejects_shell_metacharacters() {
        assert!(validate_upgrade_version("").is_err());
        assert!(validate_upgrade_version("v0.1.0;rm").is_err());
        assert!(validate_upgrade_version("$(echo bad)").is_err());
    }

    #[test]
    fn release_tags_are_normalized_for_versioned_installers() {
        assert_eq!(release_tag("latest"), "latest");
        assert_eq!(release_tag("v1.2.3"), "v1.2.3");
        assert_eq!(release_tag("1.2.3"), "v1.2.3");
    }

    #[test]
    fn upgrade_install_dir_prefers_explicit_value() {
        let explicit = PathBuf::from("/tmp/lexa-explicit");

        assert_eq!(upgrade_install_dir(Some(&explicit)).unwrap(), explicit);
    }

    #[test]
    fn version_comparison_detects_newer_release_tags() {
        assert!(version_is_newer("v0.5.2", "0.5.1"));
        assert!(!version_is_newer("v0.5.1", "0.5.1"));
        assert!(!version_is_newer("v0.4.9", "0.5.1"));
    }
}
