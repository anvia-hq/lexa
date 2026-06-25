//! Regression coverage for `brief` ranking behavior.

use lexa::engine::{ContextOptions, Engine};

fn fixture_engine() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().expect("temp dir");
    let fixture = include_str!("ranking/fixtures/lib.rs");
    let path = dir.path().join("src").join("lib.rs");
    std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture dir");
    std::fs::write(&path, fixture).expect("write fixture");

    let mut engine = Engine::new(16);
    engine.index_project(dir.path());
    (dir, engine)
}

#[test]
fn brief_prefers_callable_definitions_over_aliases() {
    let (_dir, engine) = fixture_engine();
    let detail = engine.build_context_details_with_options(
        "create_project_agent",
        &ContextOptions {
            max_results: 5,
            ..Default::default()
        },
    );

    let top = detail
        .relevant_symbols
        .first()
        .expect("at least one relevant symbol");
    assert_eq!(top.name, "create_project_agent");
    assert_eq!(top.kind, "function");
}

#[test]
fn brief_includes_definition_of_exact_name_match() {
    let (_dir, engine) = fixture_engine();
    let detail = engine.build_context_details_with_options("Agent", &ContextOptions::default());

    assert!(
        detail
            .relevant_symbols
            .iter()
            .any(|symbol| symbol.name == "Agent" && symbol.kind == "struct"),
        "expected Agent struct definition in relevant_symbols; got {:?}",
        detail
            .relevant_symbols
            .iter()
            .map(|symbol| (&symbol.name, &symbol.kind))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn brief_scope_keeps_path_preferred_symbol() {
    let (_dir, engine) = fixture_engine();
    let detail = engine.build_context_details_with_options(
        "create project agent",
        &ContextOptions {
            max_results: 5,
            path_prefix: Some("src".to_string()),
            ..Default::default()
        },
    );

    let top = detail
        .relevant_symbols
        .first()
        .expect("at least one relevant symbol");
    assert_eq!(top.name, "create_project_agent");
    assert_eq!(top.path, "src/lib.rs");
}
