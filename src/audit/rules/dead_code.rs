use crate::engine::Engine;
use crate::glob::match_glob;
use crate::types::{Language, Symbol, SymbolKind};

use crate::audit::config::{AuditConfig, AuditIncludes, DeadCodeConfig, RuleSetting};
use crate::audit::report::{AuditActionability, AuditFinding, AuditNextStep, AuditSeverity};
use serde_json::json;

pub(crate) fn audit_dead_code_candidates(
    engine: &Engine,
    config: &AuditConfig,
    includes: AuditIncludes,
    findings: &mut Vec<AuditFinding>,
) {
    let rule_setting = if includes.dead_code {
        RuleSetting::Warning
    } else {
        config.rules.dead_code_candidate
    };
    let Some(severity) = rule_setting.finding_severity(AuditSeverity::Warning) else {
        return;
    };

    for (path, meta) in engine.file_map() {
        if !is_dead_code_source_language(meta.language) {
            continue;
        }
        if is_dead_code_entrypoint_path(&path, &config.dead_code.entrypoint_globs)
            || is_dead_code_suppressed_path(&path)
        {
            continue;
        }
        let Some(outline) = engine.get_outline(&path) else {
            continue;
        };

        for symbol in &outline.symbols {
            if !is_dead_code_symbol_candidate(symbol, &config.dead_code) {
                continue;
            }

            let refs = engine.search_word(&symbol.name);
            let external_refs = refs
                .iter()
                .filter(|result| result.path != path || result.line_num != symbol.line_start)
                .count();
            if external_refs > 0 {
                continue;
            }

            let reference_count = refs.len();
            let confidence = dead_code_confidence(symbol);
            findings.push(AuditFinding {
                id: format!("dead_code.candidate:{path}:{}:{}", symbol.line_start, symbol.name),
                rule: "dead_code.candidate".to_string(),
                severity,
                actionability: AuditActionability::Candidate,
                secondary: false,
                title: format!("Possible unused {} `{}`", symbol.kind, symbol.name),
                path: path.clone(),
                line_start: Some(symbol.line_start),
                line_end: Some(symbol.line_end),
                message: "This internal-looking symbol has no indexed references outside its definition line.".to_string(),
                evidence: vec![
                    format!("symbol: {}", symbol.name),
                    format!("reference_count: {reference_count}"),
                    format!("confidence: {confidence}"),
                    "classification: source symbol candidate".to_string(),
                ],
                related_paths: Vec::new(),
                suggestion:
                    "Verify external, framework, generated, or reflective usage before removing."
                        .to_string(),
                next_steps: vec![
                    AuditNextStep::new("callers", json!({ "name": symbol.name })),
                    AuditNextStep::new("word_refs", json!({ "word": symbol.name })),
                    AuditNextStep::new(
                        "read",
                        json!({
                            "path": path,
                            "line_start": symbol.line_start,
                            "line_end": symbol.line_end
                        }),
                    ),
                ],
            });
        }
    }
}

fn is_dead_code_entrypoint_path(path: &str, globs: &[String]) -> bool {
    globs.iter().any(|glob| match_glob(glob, path))
}

fn is_dead_code_source_language(language: Language) -> bool {
    matches!(
        language,
        Language::Zig
            | Language::C
            | Language::Cpp
            | Language::Python
            | Language::JavaScript
            | Language::TypeScript
            | Language::Rust
            | Language::Go
            | Language::Php
            | Language::Ruby
            | Language::Dart
            | Language::Java
            | Language::Kotlin
            | Language::Swift
            | Language::Svelte
            | Language::Vue
            | Language::Astro
            | Language::Shell
            | Language::Fortran
    )
}

fn is_dead_code_suppressed_path(path: &str) -> bool {
    const SUPPRESSED_PATHS: &[&str] = &[
        "**/*.d.ts",
        "**/*.test.*",
        "**/*.spec.*",
        "**/*.stories.*",
        "**/*.story.*",
        "**/test/**",
        "**/tests/**",
        "**/__tests__/**",
        "**/migrations/**",
        "**/seed/**",
        "**/seeds/**",
        "**/scripts/**",
        "**/vite.config.*",
        "**/vitest.config.*",
        "**/jest.config.*",
        "**/webpack.config.*",
        "**/rollup.config.*",
        "**/tailwind.config.*",
        "**/postcss.config.*",
        "**/eslint.config.*",
        "**/next.config.*",
        "**/nuxt.config.*",
        "**/svelte.config.*",
        "**/astro.config.*",
    ];

    SUPPRESSED_PATHS
        .iter()
        .any(|pattern| match_glob(pattern, path))
}

fn is_dead_code_symbol_candidate(symbol: &Symbol, config: &DeadCodeConfig) -> bool {
    if config.ignore_symbols.contains(&symbol.name) {
        return false;
    }
    if is_framework_symbol_name(&symbol.name) {
        return false;
    }
    if is_public_or_exported_symbol(symbol) {
        return false;
    }
    matches!(
        symbol.kind,
        SymbolKind::Function
            | SymbolKind::Method
            | SymbolKind::Constant
            | SymbolKind::Variable
            | SymbolKind::ClassDef
            | SymbolKind::StructDef
            | SymbolKind::EnumDef
            | SymbolKind::TraitDef
    )
}

fn is_framework_symbol_name(name: &str) -> bool {
    matches!(
        name,
        "default"
            | "config"
            | "metadata"
            | "loader"
            | "action"
            | "GET"
            | "POST"
            | "PUT"
            | "PATCH"
            | "DELETE"
            | "HEAD"
            | "OPTIONS"
    )
}

fn dead_code_confidence(symbol: &Symbol) -> &'static str {
    match symbol.kind {
        SymbolKind::Function | SymbolKind::Method | SymbolKind::Constant | SymbolKind::Variable => {
            "high"
        }
        SymbolKind::ClassDef
        | SymbolKind::StructDef
        | SymbolKind::EnumDef
        | SymbolKind::TraitDef => "medium",
        _ => "low",
    }
}

fn is_public_or_exported_symbol(symbol: &Symbol) -> bool {
    if symbol.name.starts_with('_') {
        return false;
    }
    if symbol
        .detail
        .as_deref()
        .is_some_and(|detail| detail.contains("pub ") || detail.contains("export "))
    {
        return true;
    }
    symbol.name.chars().next().is_some_and(char::is_uppercase)
}
