use crate::engine::Engine;
use crate::types::{Symbol, SymbolKind};

use crate::audit::config::AuditConfig;
use crate::audit::report::{AuditActionability, AuditFinding, AuditNextStep, AuditSeverity};
use serde_json::json;

pub(crate) fn audit_large_files(
    engine: &Engine,
    config: &AuditConfig,
    findings: &mut Vec<AuditFinding>,
) {
    for (path, meta) in engine.file_map() {
        let base_severity = if meta.line_count >= config.thresholds.large_file_high {
            AuditSeverity::High
        } else if meta.line_count >= config.thresholds.large_file_warning {
            AuditSeverity::Warning
        } else {
            continue;
        };
        let Some(severity) = config.rules.file_large.finding_severity(base_severity) else {
            continue;
        };
        let outline_path = path.clone();
        let read_path = path.clone();

        findings.push(AuditFinding {
            id: format!("file.large:{path}"),
            rule: "file.large".to_string(),
            severity,
            actionability: AuditActionability::Actionable,
            secondary: false,
            title: "Large file".to_string(),
            path,
            line_start: Some(1),
            line_end: Some(meta.line_count),
            message: "Large files are harder for humans and agents to review safely.".to_string(),
            evidence: vec![format!("{} lines", meta.line_count)],
            related_paths: Vec::new(),
            suggestion: "Look for separable responsibilities that can move into focused modules."
                .to_string(),
            next_steps: vec![
                AuditNextStep::new("outline", json!({ "path": outline_path })),
                AuditNextStep::new("read", json!({ "path": read_path, "compact": true })),
            ],
        });
    }
}

pub(crate) fn audit_large_symbols(
    engine: &Engine,
    config: &AuditConfig,
    findings: &mut Vec<AuditFinding>,
) {
    for (path, _) in engine.file_map() {
        let Some(outline) = engine.get_outline(&path) else {
            continue;
        };
        for symbol in &outline.symbols {
            if !is_large_symbol_candidate(symbol) {
                continue;
            }

            let span = symbol.line_end.saturating_sub(symbol.line_start) + 1;
            let base_severity = if span >= config.thresholds.large_symbol_high {
                AuditSeverity::High
            } else if span >= config.thresholds.large_symbol_warning {
                AuditSeverity::Warning
            } else {
                continue;
            };
            let Some(severity) = config.rules.symbol_large.finding_severity(base_severity) else {
                continue;
            };

            findings.push(AuditFinding {
                id: format!("symbol.large:{path}:{}:{}", symbol.line_start, symbol.name),
                rule: "symbol.large".to_string(),
                severity,
                actionability: AuditActionability::Actionable,
                secondary: false,
                title: format!("Large {} `{}`", symbol.kind, symbol.name),
                path: path.clone(),
                line_start: Some(symbol.line_start),
                line_end: Some(symbol.line_end),
                message: "Large symbols concentrate behavior and increase review risk.".to_string(),
                evidence: vec![format!("{span} lines")],
                related_paths: Vec::new(),
                suggestion:
                    "Extract smaller helpers or split responsibilities before making broad changes."
                        .to_string(),
                next_steps: vec![
                    AuditNextStep::new(
                        "read",
                        json!({
                            "path": path,
                            "line_start": symbol.line_start,
                            "line_end": symbol.line_end
                        }),
                    ),
                    AuditNextStep::new("callers", json!({ "name": symbol.name })),
                ],
            });
        }
    }
}

fn is_large_symbol_candidate(symbol: &Symbol) -> bool {
    matches!(
        symbol.kind,
        SymbolKind::Function
            | SymbolKind::Method
            | SymbolKind::ImplBlock
            | SymbolKind::ClassDef
            | SymbolKind::InterfaceDef
            | SymbolKind::StructDef
            | SymbolKind::TraitDef
    )
}
