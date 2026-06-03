use crate::engine::Engine;
use crate::types::{Symbol, SymbolKind};
use hashbrown::{HashMap, HashSet};
use serde::Serialize;

const DEFAULT_MAX_FINDINGS: usize = 100;
const LARGE_FILE_WARNING_LINES: u32 = 800;
const LARGE_FILE_HIGH_LINES: u32 = 1500;
const LARGE_SYMBOL_WARNING_LINES: u32 = 120;
const LARGE_SYMBOL_HIGH_LINES: u32 = 250;
const HOTSPOT_FAN_IN_WARNING: usize = 15;
const HOTSPOT_FAN_IN_HIGH: usize = 40;
const HOTSPOT_FAN_OUT_WARNING: usize = 20;
const HOTSPOT_FAN_OUT_HIGH: usize = 50;
const MAX_CYCLE_DEPTH: usize = 32;
const MAX_INTERNAL_CYCLES: usize = 1000;

#[derive(Debug, Clone, Copy)]
pub struct AuditOptions {
    pub max_results: usize,
}

impl Default for AuditOptions {
    fn default() -> Self {
        Self {
            max_results: DEFAULT_MAX_FINDINGS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditVerdict {
    Pass,
    Warn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSeverity {
    Warning,
    High,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditSummary {
    pub total_findings: usize,
    pub returned_findings: usize,
    pub high: usize,
    pub warnings: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditFinding {
    pub id: String,
    pub rule: String,
    pub severity: AuditSeverity,
    pub title: String,
    pub path: String,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub message: String,
    pub evidence: Vec<String>,
    pub related_paths: Vec<String>,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditReport {
    pub verdict: AuditVerdict,
    pub summary: AuditSummary,
    pub findings: Vec<AuditFinding>,
}

pub fn run_audit(engine: &Engine, options: AuditOptions) -> AuditReport {
    let max_results = if options.max_results == 0 {
        DEFAULT_MAX_FINDINGS
    } else {
        options.max_results
    };
    let mut findings = Vec::new();

    audit_cycles(engine, &mut findings);
    audit_large_files(engine, &mut findings);
    audit_large_symbols(engine, &mut findings);
    audit_dependency_hotspots(engine, &mut findings);

    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line_start.cmp(&b.line_start))
            .then_with(|| a.rule.cmp(&b.rule))
            .then_with(|| a.id.cmp(&b.id))
    });

    let total_findings = findings.len();
    let high = findings
        .iter()
        .filter(|finding| finding.severity == AuditSeverity::High)
        .count();
    let warnings = total_findings.saturating_sub(high);
    let truncated = total_findings > max_results;
    findings.truncate(max_results);

    AuditReport {
        verdict: if total_findings == 0 {
            AuditVerdict::Pass
        } else {
            AuditVerdict::Warn
        },
        summary: AuditSummary {
            total_findings,
            returned_findings: findings.len(),
            high,
            warnings,
            truncated,
        },
        findings,
    }
}

pub fn render_audit_report(report: &AuditReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "audit verdict: {:?}\nfindings: {} total, {} high, {} warning(s)",
        report.verdict, report.summary.total_findings, report.summary.high, report.summary.warnings
    ));
    if report.summary.truncated {
        out.push_str(&format!(" (showing {})", report.summary.returned_findings));
    }
    out.push('\n');

    if report.findings.is_empty() {
        out.push_str("No audit findings.\n");
        return out;
    }

    for finding in &report.findings {
        let location = match (finding.line_start, finding.line_end) {
            (Some(start), Some(end)) if start != end => {
                format!("{}:{}-{}", finding.path, start, end)
            }
            (Some(line), _) => format!("{}:{}", finding.path, line),
            _ => finding.path.clone(),
        };
        out.push_str(&format!(
            "\n[{:?}] {} at {}\n{}\nSuggestion: {}\n",
            finding.severity, finding.rule, location, finding.title, finding.suggestion
        ));
        if !finding.evidence.is_empty() {
            out.push_str("Evidence:\n");
            for item in &finding.evidence {
                out.push_str(&format!("  - {item}\n"));
            }
        }
    }

    out
}

fn audit_cycles(engine: &Engine, findings: &mut Vec<AuditFinding>) {
    let cycles = find_cycles(engine);
    for cycle in cycles {
        let Some(path) = cycle.first().cloned() else {
            continue;
        };
        findings.push(AuditFinding {
            id: format!("architecture.cycle:{path}"),
            rule: "architecture.cycle".to_string(),
            severity: AuditSeverity::High,
            title: "Import cycle detected".to_string(),
            path,
            line_start: None,
            line_end: None,
            message: "Files in this cycle depend on each other through parsed imports.".to_string(),
            evidence: vec![cycle.join(" -> ")],
            related_paths: cycle,
            suggestion:
                "Break the cycle by moving shared types or behavior into a lower-level module."
                    .to_string(),
        });
    }
}

fn audit_large_files(engine: &Engine, findings: &mut Vec<AuditFinding>) {
    for (path, meta) in engine.file_map() {
        let severity = if meta.line_count >= LARGE_FILE_HIGH_LINES {
            AuditSeverity::High
        } else if meta.line_count >= LARGE_FILE_WARNING_LINES {
            AuditSeverity::Warning
        } else {
            continue;
        };

        findings.push(AuditFinding {
            id: format!("file.large:{path}"),
            rule: "file.large".to_string(),
            severity,
            title: "Large file".to_string(),
            path,
            line_start: Some(1),
            line_end: Some(meta.line_count),
            message: "Large files are harder for humans and agents to review safely.".to_string(),
            evidence: vec![format!("{} lines", meta.line_count)],
            related_paths: Vec::new(),
            suggestion: "Look for separable responsibilities that can move into focused modules."
                .to_string(),
        });
    }
}

fn audit_large_symbols(engine: &Engine, findings: &mut Vec<AuditFinding>) {
    for (path, _) in engine.file_map() {
        let Some(outline) = engine.get_outline(&path) else {
            continue;
        };
        for symbol in &outline.symbols {
            if !is_large_symbol_candidate(symbol) {
                continue;
            }

            let span = symbol.line_end.saturating_sub(symbol.line_start) + 1;
            let severity = if span >= LARGE_SYMBOL_HIGH_LINES {
                AuditSeverity::High
            } else if span >= LARGE_SYMBOL_WARNING_LINES {
                AuditSeverity::Warning
            } else {
                continue;
            };

            findings.push(AuditFinding {
                id: format!("symbol.large:{path}:{}:{}", symbol.line_start, symbol.name),
                rule: "symbol.large".to_string(),
                severity,
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
            });
        }
    }
}

fn audit_dependency_hotspots(engine: &Engine, findings: &mut Vec<AuditFinding>) {
    for (path, _) in engine.file_map() {
        let fan_in = engine.get_imported_by(&path).len();
        let fan_out = engine.get_depends_on(&path).len();
        let severity = hotspot_severity(fan_in, fan_out);
        let Some(severity) = severity else {
            continue;
        };

        findings.push(AuditFinding {
            id: format!("dependency.hotspot:{path}"),
            rule: "dependency.hotspot".to_string(),
            severity,
            title: "Dependency hotspot".to_string(),
            path,
            line_start: None,
            line_end: None,
            message: "This file has a high number of direct dependency edges.".to_string(),
            evidence: vec![format!("fan-in: {fan_in}"), format!("fan-out: {fan_out}")],
            related_paths: Vec::new(),
            suggestion:
                "Treat changes here as higher-risk and consider reducing coupling over time."
                    .to_string(),
        });
    }
}

fn hotspot_severity(fan_in: usize, fan_out: usize) -> Option<AuditSeverity> {
    if fan_in >= HOTSPOT_FAN_IN_HIGH || fan_out >= HOTSPOT_FAN_OUT_HIGH {
        Some(AuditSeverity::High)
    } else if fan_in >= HOTSPOT_FAN_IN_WARNING || fan_out >= HOTSPOT_FAN_OUT_WARNING {
        Some(AuditSeverity::Warning)
    } else {
        None
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

fn find_cycles(engine: &Engine) -> Vec<Vec<String>> {
    let paths = engine
        .file_map()
        .into_iter()
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    let indexed = paths.iter().cloned().collect::<HashSet<_>>();
    let mut adjacency = HashMap::new();

    for path in &paths {
        let mut deps = engine
            .get_depends_on(path)
            .into_iter()
            .filter(|dep| indexed.contains(dep))
            .collect::<Vec<_>>();
        deps.sort();
        adjacency.insert(path.clone(), deps);
    }

    let mut cycles = Vec::new();
    let mut seen = HashSet::new();

    for start in &paths {
        let mut stack = vec![start.clone()];
        dfs_cycles(start, start, &adjacency, &mut stack, &mut seen, &mut cycles);
        if cycles.len() >= MAX_INTERNAL_CYCLES {
            break;
        }
    }

    cycles
}

fn dfs_cycles(
    start: &str,
    current: &str,
    adjacency: &HashMap<String, Vec<String>>,
    stack: &mut Vec<String>,
    seen: &mut HashSet<String>,
    cycles: &mut Vec<Vec<String>>,
) {
    if stack.len() > MAX_CYCLE_DEPTH || cycles.len() >= MAX_INTERNAL_CYCLES {
        return;
    }

    let Some(neighbors) = adjacency.get(current) else {
        return;
    };

    for neighbor in neighbors {
        if neighbor == start && stack.len() > 1 {
            let key = canonical_cycle_key(stack);
            if seen.insert(key) {
                let mut cycle = stack.clone();
                cycle.push(start.to_string());
                cycles.push(cycle);
            }
            continue;
        }

        if stack.iter().any(|path| path == neighbor) {
            continue;
        }

        stack.push(neighbor.clone());
        dfs_cycles(start, neighbor, adjacency, stack, seen, cycles);
        stack.pop();
    }
}

fn canonical_cycle_key(cycle: &[String]) -> String {
    let mut rotations = Vec::new();
    for index in 0..cycle.len() {
        let mut rotated = Vec::with_capacity(cycle.len());
        rotated.extend_from_slice(&cycle[index..]);
        rotated.extend_from_slice(&cycle[..index]);
        rotations.push(rotated.join("\u{1f}"));
    }
    rotations.sort();
    rotations.remove(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Engine;

    #[test]
    fn audit_detects_import_cycles() {
        let mut engine = Engine::new(4);
        engine.index_file("src/a.rs", "use crate::b;\nfn a() {}\n");
        engine.index_file("src/b.rs", "use crate::a;\nfn b() {}\n");

        let report = run_audit(&engine, AuditOptions::default());

        assert_eq!(report.verdict, AuditVerdict::Warn);
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.rule == "architecture.cycle"));
    }

    #[test]
    fn audit_flags_large_files_at_threshold() {
        let mut engine = Engine::new(4);
        let content = "line\n".repeat(LARGE_FILE_WARNING_LINES as usize);
        engine.index_file("src/large.rs", &content);

        let report = run_audit(&engine, AuditOptions::default());

        assert!(report.findings.iter().any(|finding| {
            finding.rule == "file.large" && finding.severity == AuditSeverity::Warning
        }));
    }

    #[test]
    fn audit_flags_large_symbols_at_threshold() {
        let mut engine = Engine::new(4);
        let mut content = String::from("fn large() {\n");
        for _ in 0..LARGE_SYMBOL_WARNING_LINES {
            content.push_str("    let value = 1;\n");
        }
        content.push_str("}\n");
        engine.index_file("src/large.rs", &content);

        let report = run_audit(&engine, AuditOptions::default());

        assert!(report.findings.iter().any(|finding| {
            finding.rule == "symbol.large" && finding.severity == AuditSeverity::Warning
        }));
    }

    #[test]
    fn audit_flags_dependency_hotspots_at_threshold() {
        let mut engine = Engine::new(4);
        engine.index_file("src/core.rs", "pub fn core() {}\n");
        for index in 0..HOTSPOT_FAN_IN_WARNING {
            engine.index_file(
                &format!("src/user_{index}.rs"),
                "use crate::core;\nfn user() { core::core(); }\n",
            );
        }

        let report = run_audit(&engine, AuditOptions::default());

        assert!(report.findings.iter().any(|finding| {
            finding.rule == "dependency.hotspot" && finding.path == "src/core.rs"
        }));
    }

    #[test]
    fn audit_truncates_returned_findings() {
        let mut engine = Engine::new(4);
        for index in 0..3 {
            let content = "line\n".repeat(LARGE_FILE_WARNING_LINES as usize);
            engine.index_file(&format!("src/large_{index}.rs"), &content);
        }

        let report = run_audit(&engine, AuditOptions { max_results: 2 });

        assert_eq!(report.summary.total_findings, 3);
        assert_eq!(report.summary.returned_findings, 2);
        assert!(report.summary.truncated);
    }
}
