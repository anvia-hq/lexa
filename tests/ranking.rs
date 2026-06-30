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

fn brief_test_noise_engine() -> Engine {
    let mut engine = Engine::new(16);
    engine.index_file(
        "packages/core/src/tool/create-tool.ts",
        "export interface AgentToolOptions {\n  name: string;\n}\n\nexport function createTool(options: AgentToolOptions) {\n  return { type: 'tool', name: options.name };\n}\n\nexport function asTool(agent: unknown) {\n  return createTool({ name: 'agent' });\n}\n",
    );
    engine.index_file(
        "packages/core/src/agent/agent.ts",
        "import { createTool } from '../tool/create-tool';\n\nexport function createAgentTool() {\n  return createTool({ name: 'agent' });\n}\n\nexport const agentToolEventPayload = { kind: 'agent-tool' };\n",
    );
    engine.index_file(
        "packages/core/test/agent-tool.test.ts",
        "import { createAgentTool } from '../src/agent/agent';\n\nexport const agent = createAgentTool();\n\nexport function createAgentToolTest() {\n  return agent;\n}\n",
    );
    engine.index_file(
        "packages/core/src/tool/create-tool.test.ts",
        "import { createTool } from './create-tool';\n\nexport function createToolSpec() {\n  return createTool({ name: 'spec' });\n}\n",
    );
    engine
}

fn local_test_like_path(path: &str) -> bool {
    path.contains("/test/") || path.contains(".test.") || path.contains(".spec.")
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

#[test]
fn brief_suppresses_test_symbols_when_source_candidates_exist() {
    let engine = brief_test_noise_engine();
    let detail = engine.build_context_details_with_options(
        "create agent tool",
        &ContextOptions {
            max_results: 6,
            path_prefix: Some("packages/core".to_string()),
            ..Default::default()
        },
    );

    let top = detail
        .relevant_symbols
        .first()
        .expect("at least one relevant symbol");
    assert_eq!(top.name, "createAgentTool");
    assert_eq!(top.path, "packages/core/src/agent/agent.ts");
    assert!(
        detail.relevant_symbols.iter().any(|symbol| {
            symbol.path == "packages/core/src/agent/agent.ts"
                && symbol.name == "createAgentTool"
                && symbol.kind == "function"
        }),
        "expected source createAgentTool symbol; got {:?}",
        detail
            .relevant_symbols
            .iter()
            .map(|symbol| (&symbol.path, &symbol.name, &symbol.kind))
            .collect::<Vec<_>>(),
    );
    assert!(
        detail
            .relevant_symbols
            .iter()
            .all(|symbol| !local_test_like_path(&symbol.path)),
        "test symbols should be suppressed by default; got {:?}",
        detail
            .relevant_symbols
            .iter()
            .map(|symbol| (&symbol.path, &symbol.name))
            .collect::<Vec<_>>(),
    );
    assert_eq!(detail.confidence, "medium");
    assert!(detail.note.is_none());
    assert!(detail.suggested_next_steps.is_empty());
}

#[test]
fn brief_suppresses_test_snippets_when_source_candidates_exist() {
    let engine = brief_test_noise_engine();
    let detail = engine.build_context_details_with_options(
        "create agent tool",
        &ContextOptions {
            max_results: 8,
            path_prefix: Some("packages/core".to_string()),
            ..Default::default()
        },
    );

    assert!(!detail.snippets.is_empty(), "expected source snippets");
    assert_eq!(detail.snippets[0].path, "packages/core/src/agent/agent.ts");
    assert!(
        detail
            .snippets
            .iter()
            .all(|snippet| !local_test_like_path(&snippet.path)),
        "test snippets should be suppressed by default; got {:?}",
        detail
            .snippets
            .iter()
            .map(|snippet| (&snippet.path, snippet.line_num, &snippet.line_text))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn brief_returns_test_context_when_task_requests_tests() {
    let engine = brief_test_noise_engine();
    let detail = engine.build_context_details_with_options(
        "agent tool test",
        &ContextOptions {
            max_results: 8,
            path_prefix: Some("packages/core".to_string()),
            ..Default::default()
        },
    );

    assert!(
        detail
            .relevant_symbols
            .iter()
            .any(|symbol| local_test_like_path(&symbol.path))
            || detail
                .snippets
                .iter()
                .any(|snippet| local_test_like_path(&snippet.path)),
        "expected test context when task mentions tests; symbols={:?} snippets={:?}",
        detail
            .relevant_symbols
            .iter()
            .map(|symbol| (&symbol.path, &symbol.name))
            .collect::<Vec<_>>(),
        detail
            .snippets
            .iter()
            .map(|snippet| (&snippet.path, snippet.line_num))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn brief_marks_explicit_source_symbol_queries_high_confidence() {
    let (_dir, engine) = fixture_engine();
    let detail = engine.build_context_details_with_options(
        "create_project_agent",
        &ContextOptions {
            max_results: 5,
            ..Default::default()
        },
    );

    assert_eq!(detail.confidence, "high");
    assert!(detail.note.is_none());
    assert!(detail.suggested_next_steps.is_empty());
}
