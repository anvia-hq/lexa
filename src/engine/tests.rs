use super::ranking::MAX_CONTEXT_SYMBOL_LINES;
use super::shared::now_ms;
use super::*;
use crate::types::SymbolKind;

#[test]
fn snapshot_keeps_all_contents_beyond_cache_capacity() {
    let mut engine = Engine::new(1);
    engine.index_file("a.rs", "fn a() {}\n");
    engine.index_file("b.rs", "fn b() {}\n");
    engine.index_file("c.rs", "fn c() {}\n");

    let data = engine.to_snapshot_data();

    assert_eq!(data.contents.len(), 3);
}

#[test]
fn snapshot_freshness_treats_equal_timestamps_as_ambiguous() {
    let mut engine = Engine::new(1);
    engine.set_freshness_watermark(Some(10));

    assert!(engine.content_unchanged_since_snapshot(Some(9)));
    assert!(!engine.content_unchanged_since_snapshot(Some(10)));
    assert!(!engine.content_unchanged_since_snapshot(Some(11)));
    assert!(!engine.content_unchanged_since_snapshot(None));
}

#[test]
fn load_from_snapshot_replaces_existing_engine_state() {
    let mut source = Engine::new(4);
    source.index_file("fresh.rs", "fn fresh() {}\n");
    let data = source.to_snapshot_data();

    let mut engine = Engine::new(4);
    engine.index_file("stale.rs", "fn stale() {}\n");

    engine.load_snapshot_data(data);

    assert!(engine
        .find_symbol("fresh")
        .iter()
        .any(|hit| hit.path == "fresh.rs"));
    assert!(engine.find_symbol("stale").is_empty());
    assert!(engine.read_file("stale.rs", None, None).is_none());
    assert!(engine.file_map().iter().all(|(path, _)| path != "stale.rs"));
    assert_eq!(engine.store().current_seq(), 0);
}

#[test]
fn invalid_hydrated_indexes_fall_back_to_content_rebuild() {
    let mut source = Engine::new(4);
    source.index_file("src/main.rs", "fn searchable_token() {}\n");
    let mut data = source.to_snapshot_data();
    let indexes = data.indexes.as_mut().unwrap();
    indexes.words.postings[0].1[0].0 = u32::MAX;

    let mut loaded = Engine::new(4);
    loaded.load_snapshot_data(data);

    assert!(!loaded.find_symbol("searchable_token").is_empty());
    assert!(!loaded.search("searchable_token", 5).is_empty());
}

#[test]
fn read_file_out_of_range_returns_empty_content() {
    let mut engine = Engine::new(4);
    engine.index_file("a.rs", "one\ntwo\n");

    assert_eq!(
        engine.read_file("a.rs", Some(99), None),
        Some(String::new())
    );
    assert_eq!(
        engine.read_file("a.rs", Some(3), Some(2)),
        Some(String::new())
    );
}

#[test]
fn dependency_graph_rebuilds_after_later_files_are_indexed() {
    let mut engine = Engine::new(4);
    engine.index_file("src/a.rs", "use crate::b;\nfn a() {}\n");
    engine.index_file("src/b.rs", "pub fn b() {}\n");

    assert_eq!(engine.get_depends_on("src/a.rs"), vec!["src/b.rs"]);
}

#[test]
fn dependency_graph_prefers_specific_nested_rust_module_imports() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "src/root.rs",
        "use crate::api::client::Client;\nfn root() {}\n",
    );
    engine.index_file("src/client.rs", "pub struct WrongClient;\n");
    engine.index_file("src/api/client.rs", "pub struct Client;\n");

    assert_eq!(
        engine.get_depends_on("src/root.rs"),
        vec!["src/api/client.rs"]
    );
}

#[test]
fn dependency_graph_resolves_grouped_nested_rust_module_imports() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "src/root.rs",
        "use crate::api::{client::Client};\nfn root() {}\n",
    );
    engine.index_file("src/client.rs", "pub struct WrongClient;\n");
    engine.index_file("src/api/client.rs", "pub struct Client;\n");

    assert_eq!(
        engine.get_depends_on("src/root.rs"),
        vec!["src/api/client.rs"]
    );
}

#[test]
fn dependency_graph_resolves_multiple_grouped_rust_imports() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "src/root.rs",
        "use crate::api::{client::Client, server::Server};\nfn root() {}\n",
    );
    engine.index_file("src/api/client.rs", "pub struct Client;\n");
    engine.index_file("src/api/server.rs", "pub struct Server;\n");

    assert_eq!(
        engine.get_depends_on("src/root.rs"),
        vec!["src/api/client.rs", "src/api/server.rs"]
    );
}

#[test]
fn dependency_graph_resolves_multiline_grouped_rust_imports() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "src/root.rs",
        "use crate::{\n    api::client::Client,\n    api::server::Server,\n};\nfn root() {}\n",
    );
    engine.index_file("src/api/client.rs", "pub struct Client;\n");
    engine.index_file("src/api/server.rs", "pub struct Server;\n");

    assert_eq!(
        engine.get_depends_on("src/root.rs"),
        vec!["src/api/client.rs", "src/api/server.rs"]
    );
}

#[test]
fn dependency_graph_resolves_self_and_super_rust_imports() {
    let mut engine = Engine::new(4);
    engine.index_file("src/api/mod.rs", "use self::client::Client;\nfn api() {}\n");
    engine.index_file(
        "src/api/routes.rs",
        "use super::client::Client;\nfn routes() {}\n",
    );
    engine.index_file("src/client.rs", "pub struct WrongClient;\n");
    engine.index_file("src/api/client.rs", "pub struct Client;\n");

    assert_eq!(
        engine.get_depends_on("src/api/mod.rs"),
        vec!["src/api/client.rs"]
    );
    assert_eq!(
        engine.get_depends_on("src/api/routes.rs"),
        vec!["src/api/client.rs"]
    );
}

#[test]
fn dependency_graph_resolves_rust_imports_inside_nested_crate_roots() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "crates/core/src/root.rs",
        "use crate::client::Client;\nfn root() {}\n",
    );
    engine.index_file("src/client.rs", "pub struct WrongClient;\n");
    engine.index_file("crates/core/src/client.rs", "pub struct Client;\n");

    assert_eq!(
        engine.get_depends_on("crates/core/src/root.rs"),
        vec!["crates/core/src/client.rs"]
    );
}

#[test]
fn dependency_graph_resolves_rust_bin_target_crate_roots() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "src/bin/tool.rs",
        "use crate::client::Client;\nfn main() {}\n",
    );
    engine.index_file("src/client.rs", "pub struct WrongClient;\n");
    engine.index_file("src/bin/tool/client.rs", "pub struct Client;\n");

    assert_eq!(
        engine.get_depends_on("src/bin/tool.rs"),
        vec!["src/bin/tool/client.rs"]
    );
}

#[test]
fn dependency_graph_resolves_nested_rust_bin_target_crate_roots() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "src/bin/tool/main.rs",
        "use crate::client::Client;\nfn main() {}\n",
    );
    engine.index_file("src/client.rs", "pub struct WrongClient;\n");
    engine.index_file("src/bin/tool/client.rs", "pub struct Client;\n");

    assert_eq!(
        engine.get_depends_on("src/bin/tool/main.rs"),
        vec!["src/bin/tool/client.rs"]
    );
}

#[test]
fn dependency_graph_prefers_specific_relative_js_imports() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "src/app.ts",
        "import { client } from './feature/client';\nclient();\n",
    );
    engine.index_file("src/client.ts", "export const client = () => 'wrong';\n");
    engine.index_file(
        "src/feature/client.ts",
        "export const client = () => 'right';\n",
    );

    assert_eq!(
        engine.get_depends_on("src/app.ts"),
        vec!["src/feature/client.ts"]
    );
}

#[test]
fn dependency_graph_resolves_parent_relative_js_imports() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "src/feature/app.ts",
        "import { client } from '../client';\nclient();\n",
    );
    engine.index_file(
        "src/feature/client.ts",
        "export const client = () => 'wrong';\n",
    );
    engine.index_file("src/client.ts", "export const client = () => 'right';\n");

    assert_eq!(
        engine.get_depends_on("src/feature/app.ts"),
        vec!["src/client.ts"]
    );
}

#[test]
fn dependency_graph_resolves_typescript_sources_from_esm_js_specifiers() {
    let mut engine = Engine::new(4);
    engine.index_file(
            "packages/email/src/client.ts",
            "import { EmailError } from './errors.js';\nimport type { SendOptions } from './types.js';\n",
        );
    engine.index_file(
        "packages/email/src/errors.ts",
        "export class EmailError extends Error {}\n",
    );
    engine.index_file(
        "packages/email/src/types.ts",
        "export type SendOptions = { to: string };\n",
    );

    assert_eq!(
        engine.get_depends_on("packages/email/src/client.ts"),
        vec![
            "packages/email/src/errors.ts".to_string(),
            "packages/email/src/types.ts".to_string()
        ]
    );
    assert!(engine
        .get_unresolved_imports("packages/email/src/client.ts")
        .is_empty());
}

#[test]
fn dependency_graph_resolves_tsx_sources_from_jsx_specifiers() {
    let mut engine = Engine::new(4);
    engine.index_file("src/app.tsx", "import Component from './component.jsx';\n");
    engine.index_file(
        "src/component.tsx",
        "export default function Component() { return null; }\n",
    );

    assert_eq!(
        engine.get_depends_on("src/app.tsx"),
        vec!["src/component.tsx"]
    );
    assert!(engine.get_unresolved_imports("src/app.tsx").is_empty());
}

#[test]
fn dependency_graph_resolves_mts_and_cts_sources_from_runtime_specifiers() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "src/app.ts",
        "import { esm } from './esm.mjs';\nimport { cjs } from './cjs.cjs';\n",
    );
    engine.index_file("src/esm.mts", "export const esm = true;\n");
    engine.index_file("src/cjs.cts", "export const cjs = true;\n");

    assert_eq!(
        engine.get_depends_on("src/app.ts"),
        vec!["src/cjs.cts".to_string(), "src/esm.mts".to_string()]
    );
    assert!(engine.get_unresolved_imports("src/app.ts").is_empty());
}

#[test]
fn dependency_graph_resolves_local_vite_query_imports() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "apps/admin/src/routes/__root.tsx",
        "import appCss from '../styles.css?url';\n",
    );
    engine.index_file("apps/admin/src/styles.css", "body { color: black; }\n");

    assert_eq!(
        engine.get_depends_on("apps/admin/src/routes/__root.tsx"),
        vec!["apps/admin/src/styles.css"]
    );
    assert!(engine
        .get_unresolved_imports("apps/admin/src/routes/__root.tsx")
        .is_empty());
}

#[test]
fn dependency_graph_prefers_real_js_file_over_typescript_source_fallback() {
    let mut engine = Engine::new(4);
    engine.index_file("src/app.ts", "import { runtime } from './runtime.js';\n");
    engine.index_file("src/runtime.js", "export const runtime = 'js';\n");
    engine.index_file("src/runtime.ts", "export const runtime = 'ts';\n");

    assert_eq!(engine.get_depends_on("src/app.ts"), vec!["src/runtime.js"]);
}

#[test]
fn dependency_graph_does_not_fuzzy_resolve_missing_relative_js_imports() {
    let mut engine = Engine::new(4);
    engine.index_file(
            "src/app.ts",
            "import { selectedModel } from './model-settings/selected-model';\nimport { provider } from './model-settings/lib/providers';\n",
        );
    engine.index_file(
        "src/model-settings/lib/selected-model.ts",
        "export const selectedModel = 'moved';\n",
    );
    engine.index_file(
        "src/model-settings/lib/providers.ts",
        "export const provider = 'ok';\n",
    );

    assert_eq!(
        engine.get_depends_on("src/app.ts"),
        vec!["src/model-settings/lib/providers.ts"]
    );
    let unresolved = engine.get_unresolved_imports("src/app.ts");
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].import, "./model-settings/selected-model");
    assert_eq!(unresolved[0].line_start, Some(1));
}

#[test]
fn dependency_graph_does_not_resolve_missing_relative_import_by_basename() {
    let mut engine = Engine::new(4);
    engine.index_file("src/app.ts", "import { foo } from './missing/foo';\n");
    engine.index_file("src/foo.ts", "export const foo = 'wrong';\n");

    assert!(engine.get_depends_on("src/app.ts").is_empty());
    let unresolved = engine.get_unresolved_imports("src/app.ts");
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].import, "./missing/foo");
}

#[test]
fn dependency_graph_resolves_existing_asset_imports_without_text_indexing_binary_assets() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("src");
    let assets = src.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::write(
            src.join("providers.ts"),
            "import pngIcon from './assets/provider.png';\nimport svgIcon from './assets/provider.svg';\nexport const icons = [pngIcon, svgIcon];\n",
        )
        .unwrap();
    std::fs::write(assets.join("provider.png"), [0u8, 1, 2, 3]).unwrap();
    std::fs::write(assets.join("provider.svg"), "<svg></svg>\n").unwrap();

    let mut engine = Engine::new(4);
    engine.index_project(root.path());

    let asset_paths = engine.glob_files("src/assets/**");
    assert_eq!(
        asset_paths,
        vec![
            "src/assets/provider.png".to_string(),
            "src/assets/provider.svg".to_string()
        ]
    );
    assert!(engine.get_unresolved_imports("src/providers.ts").is_empty());
    assert_eq!(
        engine.get_depends_on("src/providers.ts"),
        vec![
            "src/assets/provider.png".to_string(),
            "src/assets/provider.svg".to_string()
        ]
    );
    assert!(engine
        .read_file("src/assets/provider.png", None, None)
        .unwrap()
        .contains("unindexed png file: 4 bytes"));
    assert_eq!(
        engine.read_file("src/assets/provider.svg", None, None),
        Some("<svg></svg>\n".to_string())
    );
}

#[test]
fn brief_prefers_exact_symbol_definitions_over_call_sites() {
    let mut engine = Engine::new(4);
    engine.index_file(
            "src/caller.ts",
            "import { createProjectAgent } from './agent';\nexport const agent = createProjectAgent();\n",
        );
    engine.index_file(
        "src/agent.ts",
        "export function createProjectAgent() {\n  return { run() {} };\n}\n",
    );

    let details = engine.build_context_details("how does createProjectAgent work", 5);

    assert_eq!(details.relevant_symbols[0].name, "createProjectAgent");
    assert_eq!(details.relevant_symbols[0].path, "src/agent.ts");
    assert!(details.relevant_symbols[0]
        .content
        .contains("function createProjectAgent"));
}

#[test]
fn brief_uses_path_phrases_to_find_hook_definitions() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "src/use-terminal-session.ts",
        "export function useTerminalSession() {\n  return { status: 'ready' };\n}\n",
    );
    engine.index_file(
        "src/types.ts",
        "export type TerminalSession = { status: string };\n",
    );
    engine.index_file(
            "src/terminal-pane.tsx",
            "import { useTerminalSession } from './use-terminal-session';\nexport function TerminalPane() {\n  return useTerminalSession().status;\n}\n",
        );

    let details = engine.build_context_details("what does the terminal session do", 5);

    assert_eq!(details.relevant_symbols[0].name, "useTerminalSession");
    assert_eq!(
        details.relevant_symbols[0].path,
        "src/use-terminal-session.ts"
    );
}

#[test]
fn brief_uses_package_paths_and_plural_phrases_to_find_project_agent_definitions() {
    let mut engine = Engine::new(4);
    engine.index_file(
            "packages/agents/src/index.ts",
            "export function createProjectAgent() {\n  return { kind: 'project' };\n}\nexport type ProjectAgent = ReturnType<typeof createProjectAgent>;\n",
        );
    engine.index_file(
        "packages/agents/src/mcp-settings-codec.ts",
        "export function encodeMcpSettings() {\n  return 'settings';\n}\n",
    );

    let details = engine.build_context_details("how do project agents work in packages/agents", 5);

    assert_eq!(details.relevant_symbols[0].name, "createProjectAgent");
    assert_eq!(
        details.relevant_symbols[0].path,
        "packages/agents/src/index.ts"
    );
}

#[test]
fn brief_marks_vague_natural_language_as_low_confidence() {
    let engine = Engine::new(4);

    let details = engine.build_context_details("how does this system work", 5);

    assert_eq!(details.confidence, "low");
    assert!(details
        .note
        .as_deref()
        .unwrap()
        .contains("not natural-language QA"));
    assert!(details
        .suggested_next_steps
        .iter()
        .any(|step| step.contains("symbol-search")));
}

#[test]
fn fuzzy_symbols_finds_partial_symbol_names() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "src/runtime.ts",
        "export function createAgentRuntimeForRun() {}\nexport function createProjectAgent() {}\n",
    );

    let results = engine.fuzzy_symbols("createAgent", 5);

    assert!(results
        .iter()
        .any(|result| result.name == "createAgentRuntimeForRun"));
    assert!(results
        .iter()
        .any(|result| result.name == "createProjectAgent"));
}

#[test]
fn brief_bounds_large_symbol_bodies() {
    let mut engine = Engine::new(4);
    let mut content = String::from("export function useTerminalSession() {\n");
    for idx in 0..200 {
        content.push_str(&format!("  const value{idx} = {idx};\n"));
    }
    content.push_str("  return value199;\n}\n");
    engine.index_file("src/use-terminal-session.ts", &content);

    let details = engine.build_context_details("useTerminalSession", 5);
    let symbol = &details.relevant_symbols[0];

    assert_eq!(symbol.name, "useTerminalSession");
    assert_eq!(
        symbol.content.lines().count(),
        MAX_CONTEXT_SYMBOL_LINES as usize
    );
    assert!(symbol.content.contains("const value118"));
    assert!(!symbol.content.contains("const value180"));
}

#[test]
fn filtered_files_supports_path_language_line_and_limit_filters() {
    let mut engine = Engine::new(4);
    engine.index_file("src/a.ts", "export const a = 1;\n");
    engine.index_file(
        "src/deep/b.ts",
        "export const b = 1;\nexport const c = 2;\n",
    );
    engine.index_file("src/readme.md", "# Readme\n\nbody\n");
    engine.index_file("packages/pkg/index.ts", "export const pkg = 1;\n");

    let (files, total, truncated) = engine.filtered_files(&FileFilterOptions {
        path_prefix: Some("src".to_string()),
        path_glob: Some("**/*.ts".to_string()),
        language: Some("typescript".to_string()),
        min_lines: Some(1),
        max_lines: Some(2),
        max_results: Some(1),
    });

    assert_eq!(total, 2);
    assert!(truncated);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].0, "src/a.ts");
}

#[test]
fn project_index_rebuilds_dependency_graph_once_after_batch() {
    let root = std::env::temp_dir().join(format!(
        "lexa-engine-test-{}-{}",
        std::process::id(),
        now_ms()
    ));
    let src = root.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("a.rs"), "use crate::b;\nfn a() {}\n").unwrap();
    std::fs::write(src.join("b.rs"), "pub fn b() {}\n").unwrap();

    let mut engine = Engine::new(4);
    let count = engine.index_project(&root);

    assert_eq!(count, 2);
    assert_eq!(engine.get_depends_on("src/a.rs"), vec!["src/b.rs"]);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_index_parallel_path_preserves_search_and_dependencies() {
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("src");
    std::fs::create_dir_all(&src).unwrap();
    for index in 0..64 {
        let import = if index > 0 {
            format!("use crate::module_{};\n", index - 1)
        } else {
            String::new()
        };
        std::fs::write(
            src.join(format!("module_{index}.rs")),
            format!("{import}pub fn token_{index}() {{}}\n"),
        )
        .unwrap();
    }

    let mut engine = Engine::new(4);
    assert_eq!(engine.index_project(root.path()), 64);

    assert!(!engine.find_symbol("token_63").is_empty());
    assert!(!engine.search("token_63", 5).is_empty());
    assert_eq!(
        engine.get_depends_on("src/module_63.rs"),
        vec!["src/module_62.rs"]
    );
}

#[test]
fn python_outline_includes_class_methods() {
    let mut engine = Engine::new(4);
    engine.index_file(
        "service.py",
        "class Service:\n    def handle(self):\n        pass\n",
    );

    let outline = engine.get_outline("service.py").unwrap();
    assert!(outline
        .symbols
        .iter()
        .any(|symbol| symbol.name == "handle" && symbol.kind == SymbolKind::Method));
}
