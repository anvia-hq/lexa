mod architecture;
mod dead_code;
mod dependencies;
mod size;

use crate::engine::Engine;
use crate::glob::match_glob;
use hashbrown::HashSet;

use super::config::{AuditConfig, AuditIgnore, DEFAULT_GENERATED_IGNORE_GLOBS};
use super::report::AuditFinding;
use super::scope::AuditScope;
use super::AuditIncludes;

pub(crate) fn collect_findings(
    engine: &Engine,
    config: &AuditConfig,
    includes: AuditIncludes,
) -> Vec<AuditFinding> {
    let mut findings = Vec::new();

    architecture::audit_cycles(engine, config, &mut findings);
    dependencies::audit_unresolved_imports(engine, config, &mut findings);
    size::audit_large_files(engine, config, &mut findings);
    size::audit_large_symbols(engine, config, &mut findings);
    dependencies::audit_dependency_hotspots(engine, config, &mut findings);
    dead_code::audit_dead_code_candidates(engine, config, includes, &mut findings);

    findings
}

pub(crate) fn filter_findings_by_scope(
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

pub(crate) fn filter_ignored_findings(
    findings: Vec<AuditFinding>,
    ignore: &AuditIgnore,
) -> Vec<AuditFinding> {
    findings
        .into_iter()
        .filter(|finding| !ignore.findings.contains(&finding.id))
        .filter(|finding| {
            !is_ignored_path(ignore, &finding.path)
                && !finding
                    .related_paths
                    .iter()
                    .any(|path| is_ignored_path(ignore, path))
        })
        .collect()
}

fn is_ignored_path(ignore: &AuditIgnore, path: &str) -> bool {
    ignore.paths.iter().any(|pattern| match_glob(pattern, path))
        || (ignore.generated && is_generated_path(path))
}

fn is_generated_path(path: &str) -> bool {
    DEFAULT_GENERATED_IGNORE_GLOBS
        .iter()
        .any(|pattern| match_glob(pattern, path))
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
