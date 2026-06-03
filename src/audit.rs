use crate::engine::Engine;
use crate::glob::match_glob;
use crate::types::{Symbol, SymbolKind};
use anyhow::{bail, Context, Result};
use hashbrown::HashSet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
const DEFAULT_DEAD_CODE_IGNORE_SYMBOLS: &[&str] = &[
    "main", "new", "default", "test", "setup", "handler", "render", "init",
];
const DEFAULT_DEAD_CODE_ENTRYPOINT_GLOBS: &[&str] = &[
    "src/main.*",
    "src/bin/**",
    "src/lib.*",
    "pages/**",
    "app/**",
    "tests/**",
    "benches/**",
    "examples/**",
];

#[derive(Debug, Clone)]
pub struct AuditOptions {
    pub max_results: usize,
    pub scope: AuditScope,
    pub config: AuditConfig,
    pub includes: AuditIncludes,
}

impl Default for AuditOptions {
    fn default() -> Self {
        Self {
            max_results: DEFAULT_MAX_FINDINGS,
            scope: AuditScope::Project,
            config: AuditConfig::default(),
            includes: AuditIncludes::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AuditIncludes {
    pub dead_code: bool,
}

#[derive(Debug, Clone)]
pub struct AuditConfig {
    pub max_findings: usize,
    pub thresholds: AuditThresholds,
    pub rules: AuditRules,
    pub ignore: AuditIgnore,
    pub dead_code: DeadCodeConfig,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            max_findings: DEFAULT_MAX_FINDINGS,
            thresholds: AuditThresholds::default(),
            rules: AuditRules::default(),
            ignore: AuditIgnore::default(),
            dead_code: DeadCodeConfig::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditThresholds {
    pub large_file_warning: u32,
    pub large_file_high: u32,
    pub large_symbol_warning: u32,
    pub large_symbol_high: u32,
    pub fan_in_warning: usize,
    pub fan_in_high: usize,
    pub fan_out_warning: usize,
    pub fan_out_high: usize,
}

impl Default for AuditThresholds {
    fn default() -> Self {
        Self {
            large_file_warning: LARGE_FILE_WARNING_LINES,
            large_file_high: LARGE_FILE_HIGH_LINES,
            large_symbol_warning: LARGE_SYMBOL_WARNING_LINES,
            large_symbol_high: LARGE_SYMBOL_HIGH_LINES,
            fan_in_warning: HOTSPOT_FAN_IN_WARNING,
            fan_in_high: HOTSPOT_FAN_IN_HIGH,
            fan_out_warning: HOTSPOT_FAN_OUT_WARNING,
            fan_out_high: HOTSPOT_FAN_OUT_HIGH,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditRules {
    pub architecture_cycle: RuleSetting,
    pub file_large: RuleSetting,
    pub symbol_large: RuleSetting,
    pub dependency_hotspot: RuleSetting,
    pub dead_code_candidate: RuleSetting,
}

impl Default for AuditRules {
    fn default() -> Self {
        Self {
            architecture_cycle: RuleSetting::High,
            file_large: RuleSetting::Warning,
            symbol_large: RuleSetting::Warning,
            dependency_hotspot: RuleSetting::Warning,
            dead_code_candidate: RuleSetting::Off,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AuditIgnore {
    pub paths: Vec<String>,
    pub findings: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct DeadCodeConfig {
    pub ignore_symbols: HashSet<String>,
    pub entrypoint_globs: Vec<String>,
}

impl Default for DeadCodeConfig {
    fn default() -> Self {
        Self {
            ignore_symbols: DEFAULT_DEAD_CODE_IGNORE_SYMBOLS
                .iter()
                .map(|symbol| (*symbol).to_string())
                .collect(),
            entrypoint_globs: DEFAULT_DEAD_CODE_ENTRYPOINT_GLOBS
                .iter()
                .map(|glob| (*glob).to_string())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSetting {
    Off,
    Warning,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditScopeReport {
    Project,
    GitSince {
        base: String,
        changed_files: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditScope {
    Project,
    GitSince {
        base: String,
        changed_files: Vec<String>,
    },
}

impl AuditScope {
    fn report(&self) -> AuditScopeReport {
        match self {
            Self::Project => AuditScopeReport::Project,
            Self::GitSince {
                base,
                changed_files,
            } => AuditScopeReport::GitSince {
                base: base.clone(),
                changed_files: changed_files.clone(),
            },
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
    pub scope: AuditScopeReport,
    pub verdict: AuditVerdict,
    pub summary: AuditSummary,
    pub findings: Vec<AuditFinding>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditConfigFile {
    #[serde(default)]
    audit: AuditConfigSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditConfigSection {
    max_findings: Option<usize>,
    #[serde(default)]
    thresholds: AuditThresholdSection,
    #[serde(default)]
    rules: HashMap<String, RuleSetting>,
    #[serde(default)]
    ignore: AuditIgnoreSection,
    #[serde(default)]
    dead_code: DeadCodeSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditThresholdSection {
    large_file_warning: Option<u32>,
    large_file_high: Option<u32>,
    large_symbol_warning: Option<u32>,
    large_symbol_high: Option<u32>,
    fan_in_warning: Option<usize>,
    fan_in_high: Option<usize>,
    fan_out_warning: Option<usize>,
    fan_out_high: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditIgnoreSection {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    findings: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeadCodeSection {
    #[serde(default)]
    ignore_symbols: Vec<String>,
    #[serde(default)]
    entrypoint_globs: Vec<String>,
}

pub fn load_audit_config(
    root: &Path,
    explicit_path: Option<&Path>,
    no_config: bool,
) -> Result<AuditConfig> {
    if no_config {
        return Ok(AuditConfig::default());
    }

    let Some(path) = find_audit_config_path(root, explicit_path) else {
        return Ok(AuditConfig::default());
    };

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read audit config {}", path.display()))?;
    let file = toml::from_str::<AuditConfigFile>(&content)
        .with_context(|| format!("failed to parse audit config {}", path.display()))?;
    AuditConfig::from_file(file)
}

fn find_audit_config_path(root: &Path, explicit_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit_path {
        return Some(if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        });
    }

    let candidates = [root.join("lexa.toml"), root.join(".lexa/audit.toml")];
    candidates.into_iter().find(|path| path.exists())
}

impl AuditConfig {
    fn from_file(file: AuditConfigFile) -> Result<Self> {
        let mut config = Self::default();
        let audit = file.audit;

        if let Some(max_findings) = audit.max_findings {
            config.max_findings = max_findings;
        }

        config.thresholds.apply(audit.thresholds)?;
        config.rules.apply(audit.rules)?;
        config.ignore.paths = audit.ignore.paths;
        config.ignore.findings = audit.ignore.findings.into_iter().collect();
        config.dead_code.apply(audit.dead_code);

        Ok(config)
    }
}

impl DeadCodeConfig {
    fn apply(&mut self, section: DeadCodeSection) {
        self.ignore_symbols.extend(section.ignore_symbols);
        self.entrypoint_globs.extend(section.entrypoint_globs);
    }
}

impl AuditThresholds {
    fn apply(&mut self, section: AuditThresholdSection) -> Result<()> {
        if let Some(value) = section.large_file_warning {
            self.large_file_warning = value;
        }
        if let Some(value) = section.large_file_high {
            self.large_file_high = value;
        }
        if let Some(value) = section.large_symbol_warning {
            self.large_symbol_warning = value;
        }
        if let Some(value) = section.large_symbol_high {
            self.large_symbol_high = value;
        }
        if let Some(value) = section.fan_in_warning {
            self.fan_in_warning = value;
        }
        if let Some(value) = section.fan_in_high {
            self.fan_in_high = value;
        }
        if let Some(value) = section.fan_out_warning {
            self.fan_out_warning = value;
        }
        if let Some(value) = section.fan_out_high {
            self.fan_out_high = value;
        }
        self.validate()
    }

    fn validate(&self) -> Result<()> {
        if self.large_file_warning > self.large_file_high {
            bail!("large_file_warning must be <= large_file_high");
        }
        if self.large_symbol_warning > self.large_symbol_high {
            bail!("large_symbol_warning must be <= large_symbol_high");
        }
        if self.fan_in_warning > self.fan_in_high {
            bail!("fan_in_warning must be <= fan_in_high");
        }
        if self.fan_out_warning > self.fan_out_high {
            bail!("fan_out_warning must be <= fan_out_high");
        }
        Ok(())
    }
}

impl AuditRules {
    fn apply(&mut self, rules: HashMap<String, RuleSetting>) -> Result<()> {
        for (rule, setting) in rules {
            match rule.as_str() {
                "architecture.cycle" => self.architecture_cycle = setting,
                "file.large" => self.file_large = setting,
                "symbol.large" => self.symbol_large = setting,
                "dependency.hotspot" => self.dependency_hotspot = setting,
                "dead_code.candidate" => self.dead_code_candidate = setting,
                _ => bail!("unknown audit rule '{rule}'"),
            }
        }
        Ok(())
    }
}

pub fn run_audit(engine: &Engine, options: AuditOptions) -> AuditReport {
    let max_results = if options.max_results == 0 {
        options.config.max_findings
    } else {
        options.max_results
    };
    let mut findings = Vec::new();

    audit_cycles(engine, &options.config, &mut findings);
    audit_large_files(engine, &options.config, &mut findings);
    audit_large_symbols(engine, &options.config, &mut findings);
    audit_dependency_hotspots(engine, &options.config, &mut findings);
    audit_dead_code_candidates(engine, &options.config, options.includes, &mut findings);

    findings = filter_findings_by_scope(engine, findings, &options.scope);
    findings = filter_ignored_findings(findings, &options.config.ignore);

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
        scope: options.scope.report(),
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

    if let AuditScopeReport::GitSince {
        base,
        changed_files,
    } = &report.scope
    {
        out.push_str(&format!(
            "scope: git since {base}, {} changed file(s)\n",
            changed_files.len()
        ));
    }

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

pub fn changed_files_since(root: &Path, base: &str) -> Result<Vec<String>> {
    let prefix = git_prefix(root).unwrap_or_default();
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("diff")
        .arg("--name-only")
        .arg("--diff-filter=ACMRT")
        .arg(format!("{base}...HEAD"))
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git diff failed for base '{base}': {stderr}");
    }

    let mut files = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| normalize_git_changed_path(line.trim(), &prefix))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    Ok(files)
}

fn git_prefix(root: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("--show-prefix")
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git rev-parse failed: {stderr}");
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .trim()
        .replace('\\', "/"))
}

fn normalize_git_changed_path(path: &str, prefix: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    if prefix.is_empty() {
        return Some(path);
    }
    path.strip_prefix(prefix).map(ToString::to_string)
}

fn audit_cycles(engine: &Engine, config: &AuditConfig, findings: &mut Vec<AuditFinding>) {
    let Some(severity) = config
        .rules
        .architecture_cycle
        .finding_severity(AuditSeverity::High)
    else {
        return;
    };

    let cycles = find_cycles(engine);
    for cycle in cycles {
        let Some(path) = cycle.first().cloned() else {
            continue;
        };
        findings.push(AuditFinding {
            id: format!("architecture.cycle:{path}"),
            rule: "architecture.cycle".to_string(),
            severity,
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

fn audit_large_files(engine: &Engine, config: &AuditConfig, findings: &mut Vec<AuditFinding>) {
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

fn audit_large_symbols(engine: &Engine, config: &AuditConfig, findings: &mut Vec<AuditFinding>) {
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

fn audit_dependency_hotspots(
    engine: &Engine,
    config: &AuditConfig,
    findings: &mut Vec<AuditFinding>,
) {
    for (path, _) in engine.file_map() {
        let fan_in = engine.get_imported_by(&path).len();
        let fan_out = engine.get_depends_on(&path).len();
        let Some(base_severity) = hotspot_severity(fan_in, fan_out, &config.thresholds) else {
            continue;
        };
        let Some(severity) = config
            .rules
            .dependency_hotspot
            .finding_severity(base_severity)
        else {
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

fn audit_dead_code_candidates(
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

    for (path, _) in engine.file_map() {
        if is_dead_code_entrypoint_path(&path, &config.dead_code.entrypoint_globs) {
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
            findings.push(AuditFinding {
                id: format!("dead_code.candidate:{path}:{}:{}", symbol.line_start, symbol.name),
                rule: "dead_code.candidate".to_string(),
                severity,
                title: format!("Possible unused {} `{}`", symbol.kind, symbol.name),
                path: path.clone(),
                line_start: Some(symbol.line_start),
                line_end: Some(symbol.line_end),
                message: "This internal-looking symbol has no indexed references outside its definition line.".to_string(),
                evidence: vec![
                    format!("symbol: {}", symbol.name),
                    format!("reference_count: {reference_count}"),
                    "classification: internal candidate".to_string(),
                ],
                related_paths: Vec::new(),
                suggestion:
                    "Verify external, framework, generated, or reflective usage before removing."
                        .to_string(),
            });
        }
    }
}

fn hotspot_severity(
    fan_in: usize,
    fan_out: usize,
    thresholds: &AuditThresholds,
) -> Option<AuditSeverity> {
    if fan_in >= thresholds.fan_in_high || fan_out >= thresholds.fan_out_high {
        Some(AuditSeverity::High)
    } else if fan_in >= thresholds.fan_in_warning || fan_out >= thresholds.fan_out_warning {
        Some(AuditSeverity::Warning)
    } else {
        None
    }
}

impl RuleSetting {
    fn finding_severity(self, base: AuditSeverity) -> Option<AuditSeverity> {
        match self {
            Self::Off => None,
            Self::Warning => Some(base),
            Self::High => Some(AuditSeverity::High),
        }
    }
}

fn filter_findings_by_scope(
    engine: &Engine,
    findings: Vec<AuditFinding>,
    scope: &AuditScope,
) -> Vec<AuditFinding> {
    let AuditScope::GitSince { changed_files, .. } = scope else {
        return findings;
    };
    let changed = changed_files.iter().cloned().collect::<HashSet<_>>();
    if changed.is_empty() {
        return Vec::new();
    }

    findings
        .into_iter()
        .filter(|finding| is_finding_scope_relevant(engine, finding, &changed))
        .collect()
}

fn filter_ignored_findings(findings: Vec<AuditFinding>, ignore: &AuditIgnore) -> Vec<AuditFinding> {
    findings
        .into_iter()
        .filter(|finding| !ignore.findings.contains(&finding.id))
        .filter(|finding| {
            !ignore
                .paths
                .iter()
                .any(|pattern| match_glob(pattern, &finding.path))
        })
        .collect()
}

fn is_finding_scope_relevant(
    engine: &Engine,
    finding: &AuditFinding,
    changed: &HashSet<String>,
) -> bool {
    if changed.contains(&finding.path) {
        return true;
    }
    if finding
        .related_paths
        .iter()
        .any(|path| changed.contains(path))
    {
        return true;
    }

    changed.iter().any(|changed_path| {
        engine.get_depends_on(changed_path).contains(&finding.path)
            || engine.get_imported_by(changed_path).contains(&finding.path)
            || engine.get_depends_on(&finding.path).contains(changed_path)
            || engine.get_imported_by(&finding.path).contains(changed_path)
    })
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

fn is_dead_code_entrypoint_path(path: &str, globs: &[String]) -> bool {
    globs.iter().any(|glob| match_glob(glob, path))
}

fn is_dead_code_symbol_candidate(symbol: &Symbol, config: &DeadCodeConfig) -> bool {
    if config.ignore_symbols.contains(&symbol.name) {
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
            | SymbolKind::InterfaceDef
            | SymbolKind::TypeAlias
            | SymbolKind::TraitDef
    )
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
    fn normalize_git_changed_path_strips_worktree_prefix() {
        assert_eq!(
            normalize_git_changed_path("crates/app/src/main.rs", "crates/app/"),
            Some("src/main.rs".to_string())
        );
        assert_eq!(
            normalize_git_changed_path("crates/core/src/lib.rs", "crates/app/"),
            None
        );
    }

    #[test]
    fn audit_git_scope_keeps_findings_for_changed_paths() {
        let mut engine = Engine::new(4);
        let content = "line\n".repeat(LARGE_FILE_WARNING_LINES as usize);
        engine.index_file("src/large.rs", &content);
        engine.index_file("src/other.rs", &content);

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: 100,
                scope: AuditScope::GitSince {
                    base: "main".to_string(),
                    changed_files: vec!["src/large.rs".to_string()],
                },
                config: AuditConfig::default(),
                includes: AuditIncludes::default(),
            },
        );

        assert_eq!(report.summary.total_findings, 1);
        assert_eq!(report.findings[0].path, "src/large.rs");
    }

    #[test]
    fn audit_git_scope_keeps_direct_dependency_context() {
        let mut engine = Engine::new(4);
        engine.index_file("src/core.rs", "pub fn core() {}\n");
        for index in 0..HOTSPOT_FAN_IN_WARNING {
            engine.index_file(
                &format!("src/user_{index}.rs"),
                "use crate::core;\nfn user() { core::core(); }\n",
            );
        }

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: 100,
                scope: AuditScope::GitSince {
                    base: "main".to_string(),
                    changed_files: vec!["src/user_0.rs".to_string()],
                },
                config: AuditConfig::default(),
                includes: AuditIncludes::default(),
            },
        );

        assert!(report.findings.iter().any(|finding| {
            finding.rule == "dependency.hotspot" && finding.path == "src/core.rs"
        }));
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

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: 2,
                scope: AuditScope::Project,
                config: AuditConfig::default(),
                includes: AuditIncludes::default(),
            },
        );

        assert_eq!(report.summary.total_findings, 3);
        assert_eq!(report.summary.returned_findings, 2);
        assert!(report.summary.truncated);
    }

    #[test]
    fn audit_config_thresholds_override_defaults() {
        let mut engine = Engine::new(4);
        engine.index_file("src/small.rs", "one\ntwo\nthree\n");
        let mut config = AuditConfig::default();
        config.thresholds.large_file_warning = 3;
        config.thresholds.large_file_high = 10;

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: 100,
                scope: AuditScope::Project,
                config,
                includes: AuditIncludes::default(),
            },
        );

        assert!(report
            .findings
            .iter()
            .any(|finding| finding.rule == "file.large"));
    }

    #[test]
    fn audit_config_can_disable_rules() {
        let mut engine = Engine::new(4);
        let content = "line\n".repeat(LARGE_FILE_WARNING_LINES as usize);
        engine.index_file("src/large.rs", &content);
        let mut config = AuditConfig::default();
        config.rules.file_large = RuleSetting::Off;

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: 100,
                scope: AuditScope::Project,
                config,
                includes: AuditIncludes::default(),
            },
        );

        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.rule == "file.large"));
    }

    #[test]
    fn audit_config_ignores_findings_and_paths() {
        let mut engine = Engine::new(4);
        let content = "line\n".repeat(LARGE_FILE_WARNING_LINES as usize);
        engine.index_file("src/large.rs", &content);
        engine.index_file("vendor/large.rs", &content);
        let mut config = AuditConfig::default();
        config
            .ignore
            .findings
            .insert("file.large:src/large.rs".to_string());
        config.ignore.paths.push("vendor/**".to_string());

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: 100,
                scope: AuditScope::Project,
                config,
                includes: AuditIncludes::default(),
            },
        );

        assert_eq!(report.summary.total_findings, 0);
    }

    #[test]
    fn audit_config_parses_quoted_rule_ids() {
        let parsed = toml::from_str::<AuditConfigFile>(
            r#"
            [audit]
            max_findings = 12

            [audit.thresholds]
            large_file_warning = 10

            [audit.rules]
            "file.large" = "off"
            "dead_code.candidate" = "warning"

            [audit.ignore]
            paths = ["vendor/**"]
            findings = ["dependency.hotspot:src/main.rs"]

            [audit.dead_code]
            ignore_symbols = ["handler"]
            entrypoint_globs = ["routes/**"]
            "#,
        )
        .unwrap();
        let config = AuditConfig::from_file(parsed).unwrap();

        assert_eq!(config.max_findings, 12);
        assert_eq!(config.thresholds.large_file_warning, 10);
        assert_eq!(config.rules.file_large, RuleSetting::Off);
        assert_eq!(config.rules.dead_code_candidate, RuleSetting::Warning);
        assert_eq!(config.ignore.paths, vec!["vendor/**"]);
        assert!(config
            .ignore
            .findings
            .contains("dependency.hotspot:src/main.rs"));
        assert!(config.dead_code.ignore_symbols.contains("handler"));
        assert!(config
            .dead_code
            .entrypoint_globs
            .contains(&"routes/**".to_string()));
    }

    #[test]
    fn audit_config_rejects_unknown_keys() {
        let parsed = toml::from_str::<AuditConfigFile>(
            r#"
            [audit]
            unexpected = true
            "#,
        );

        assert!(parsed.is_err());
    }

    #[test]
    fn audit_config_rejects_unknown_rules() {
        let parsed = toml::from_str::<AuditConfigFile>(
            r#"
            [audit.rules]
            "unknown.rule" = "warning"
            "#,
        )
        .unwrap();

        assert!(AuditConfig::from_file(parsed).is_err());
    }

    #[test]
    fn dead_code_candidates_are_disabled_by_default() {
        let mut engine = Engine::new(4);
        engine.index_file("src/helper.rs", "fn unused_helper() {}\n");

        let report = run_audit(&engine, AuditOptions::default());

        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.rule == "dead_code.candidate"));
    }

    #[test]
    fn include_dead_code_reports_internal_unreferenced_symbols() {
        let mut engine = Engine::new(4);
        engine.index_file("src/helper.rs", "fn unused_helper() {}\n");

        let report = run_audit(
            &engine,
            AuditOptions {
                includes: AuditIncludes { dead_code: true },
                ..AuditOptions::default()
            },
        );

        assert!(report.findings.iter().any(|finding| {
            finding.rule == "dead_code.candidate" && finding.path == "src/helper.rs"
        }));
    }

    #[test]
    fn dead_code_candidates_skip_referenced_symbols() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/helper.rs",
            "fn used_helper() {}\nfn caller() { used_helper(); }\n",
        );

        let report = run_audit(
            &engine,
            AuditOptions {
                includes: AuditIncludes { dead_code: true },
                ..AuditOptions::default()
            },
        );

        assert!(!report.findings.iter().any(|finding| {
            finding.rule == "dead_code.candidate" && finding.id.contains("used_helper")
        }));
    }

    #[test]
    fn dead_code_candidates_skip_entrypoint_paths() {
        let mut engine = Engine::new(4);
        engine.index_file("src/main.rs", "fn unused_helper() {}\n");

        let report = run_audit(
            &engine,
            AuditOptions {
                includes: AuditIncludes { dead_code: true },
                ..AuditOptions::default()
            },
        );

        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.rule == "dead_code.candidate"));
    }

    #[test]
    fn dead_code_candidates_can_be_enabled_by_config() {
        let mut engine = Engine::new(4);
        engine.index_file("src/helper.rs", "fn unused_helper() {}\n");
        let mut config = AuditConfig::default();
        config.rules.dead_code_candidate = RuleSetting::Warning;

        let report = run_audit(
            &engine,
            AuditOptions {
                config,
                ..AuditOptions::default()
            },
        );

        assert!(report
            .findings
            .iter()
            .any(|finding| finding.rule == "dead_code.candidate"));
    }
}
