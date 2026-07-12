use super::*;
use crate::cli::Commands;
use clap::Parser;
use std::path::PathBuf;

#[test]
fn upgrade_default_version_does_not_conflict_with_global_version_flag() {
    let cli = Cli::try_parse_from(["lexa", "upgrade"]).unwrap();

    assert!(!cli.version);
    match cli.command {
        Some(Commands::Upgrade {
            version,
            install_dir,
        }) => {
            assert_eq!(version, "latest");
            assert!(install_dir.is_none());
        }
        _ => panic!("expected upgrade command"),
    }
}

#[test]
fn mcp_defaults_to_refresh_with_standard_debounce() {
    let cli = Cli::try_parse_from(["lexa", "mcp", "."]).unwrap();

    match cli.command {
        Some(Commands::Mcp {
            no_refresh,
            debounce,
            structured_content,
            ..
        }) => {
            assert!(!no_refresh);
            assert_eq!(debounce, 500);
            assert!(!structured_content);
        }
        _ => panic!("expected mcp command"),
    }
}

#[test]
fn mcp_accepts_no_refresh_and_custom_debounce() {
    let cli =
        Cli::try_parse_from(["lexa", "mcp", ".", "--no-refresh", "--debounce", "250"]).unwrap();

    match cli.command {
        Some(Commands::Mcp {
            no_refresh,
            debounce,
            structured_content,
            ..
        }) => {
            assert!(no_refresh);
            assert_eq!(debounce, 250);
            assert!(!structured_content);
        }
        _ => panic!("expected mcp command"),
    }
}

#[test]
fn removed_output_flags_are_detected_before_clap_parse() {
    for flag in ["--json", "--structured-content", "--json-output"] {
        assert_eq!(removed_output_flag(["lexa", "mcp", ".", flag]), Some(flag));
    }
}

#[test]
fn mcp_accepts_log_file_flag() {
    let cli = Cli::try_parse_from(["lexa", "mcp", ".", "--log-file", "/tmp/lexa-mcp.log"]).unwrap();

    match cli.command {
        Some(Commands::Mcp { log_file, .. }) => {
            assert_eq!(log_file, Some(PathBuf::from("/tmp/lexa-mcp.log")));
        }
        _ => panic!("expected mcp command"),
    }
}

#[test]
fn removed_output_flag_detects_equals_forms() {
    assert_eq!(removed_output_flag(["lexa", "--json=true"]), Some("--json"));
    assert_eq!(
        removed_output_flag(["lexa", "mcp", ".", "--structured-content=true"]),
        Some("--structured-content")
    );
    assert_eq!(
        removed_output_flag(["lexa", "mcp", ".", "--json-output=true"]),
        Some("--json-output")
    );
}

#[test]
fn removed_output_flag_respects_end_of_options_sentinel() {
    assert_eq!(
        removed_output_flag(["lexa", "pipeline", "--", "--json"]),
        None
    );
    assert_eq!(
        removed_output_flag(["lexa", "--json", "--", "--structured-content"]),
        Some("--json")
    );
}

#[test]
fn parse_line_range_supports_single_bounded_and_open_ranges() {
    assert_eq!(parse_line_range("7").unwrap(), (Some(7), Some(7)));
    assert_eq!(parse_line_range("3-9").unwrap(), (Some(3), Some(9)));
    assert_eq!(parse_line_range("-9").unwrap(), (None, Some(9)));
    assert_eq!(parse_line_range("3-").unwrap(), (Some(3), None));
    assert!(parse_line_range("abc").is_err());
    assert!(parse_line_range("3-abc").is_err());
}
