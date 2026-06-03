use crate::engine::Engine;

use crate::audit::config::{AuditConfig, AuditThresholds};
use crate::audit::report::{AuditActionability, AuditFinding, AuditNextStep, AuditSeverity};
use serde_json::json;

pub(crate) fn audit_dependency_hotspots(
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
        let Some(mut severity) = config
            .rules
            .dependency_hotspot
            .finding_severity(base_severity)
        else {
            continue;
        };
        let kind = classify_hotspot(engine, &path, fan_in, fan_out, base_severity);
        if matches!(kind.actionability, AuditActionability::Expected) {
            severity = AuditSeverity::Warning;
        }
        let next_steps = hotspot_next_steps(&path, fan_in, fan_out);

        findings.push(AuditFinding {
            id: format!("dependency.hotspot:{path}"),
            rule: "dependency.hotspot".to_string(),
            severity,
            actionability: kind.actionability,
            secondary: false,
            title: kind.title.to_string(),
            path,
            line_start: None,
            line_end: None,
            message: kind.message.to_string(),
            evidence: vec![format!("fan-in: {fan_in}"), format!("fan-out: {fan_out}")],
            related_paths: Vec::new(),
            suggestion: kind.suggestion.to_string(),
            next_steps,
        });
    }
}

struct HotspotClassification {
    actionability: AuditActionability,
    title: &'static str,
    message: &'static str,
    suggestion: &'static str,
}

fn classify_hotspot(
    engine: &Engine,
    path: &str,
    fan_in: usize,
    fan_out: usize,
    severity: AuditSeverity,
) -> HotspotClassification {
    if is_entrypoint_orchestrator(path, fan_in, fan_out) {
        return HotspotClassification {
            actionability: AuditActionability::Expected,
            title: "Entrypoint or composition hotspot",
            message: "This file imports many modules as part of route registration, app wiring, or startup composition.",
            suggestion: "Treat edits as integration-sensitive; split only if the composition flow is hard to review.",
        };
    }

    if is_expected_shared_utility(engine, path, fan_in, fan_out) {
        return HotspotClassification {
            actionability: AuditActionability::Expected,
            title: "Expected shared utility hotspot",
            message: "This small shared helper is imported broadly, which is normal for stable utility primitives.",
            suggestion: "Avoid broad refactors here; keep the API stable and add focused tests before changing behavior.",
        };
    }

    if is_expected_shared_infrastructure(path, fan_in, fan_out) {
        return HotspotClassification {
            actionability: AuditActionability::Expected,
            title: "Expected shared infrastructure hotspot",
            message: "This shared infrastructure module is imported broadly by design.",
            suggestion: "Treat edits as high-blast-radius, but do not split it unless responsibilities are clearly mixed.",
        };
    }

    HotspotClassification {
        actionability: if severity == AuditSeverity::High {
            AuditActionability::Actionable
        } else {
            AuditActionability::RiskNote
        },
        title: "Dependency hotspot",
        message: "This file has a high number of direct dependency edges.",
        suggestion: "Treat changes here as higher-risk and consider reducing coupling over time.",
    }
}

fn is_entrypoint_orchestrator(path: &str, fan_in: usize, fan_out: usize) -> bool {
    if fan_in > 3 || fan_out < 10 {
        return false;
    }

    let file_name = path.rsplit('/').next().unwrap_or(path);
    let stem = file_name.split('.').next().unwrap_or(file_name);
    matches!(
        stem,
        "index" | "main" | "app" | "server" | "router" | "routes"
    ) || path.contains("/routes/")
}

fn is_expected_shared_utility(engine: &Engine, path: &str, fan_in: usize, fan_out: usize) -> bool {
    if fan_in < 10 || fan_out > 3 {
        return false;
    }
    if !is_utility_path(path) {
        return false;
    }

    engine
        .get_outline(path)
        .map(|outline| outline.symbols.len() <= 8)
        .unwrap_or(true)
}

fn is_utility_path(path: &str) -> bool {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    let stem = file_name.split('.').next().unwrap_or(file_name);
    matches!(
        stem,
        "utils" | "util" | "helpers" | "helper" | "common" | "classnames" | "cn"
    ) || path.contains("/lib/utils")
        || path.contains("/lib/util")
}

fn is_expected_shared_infrastructure(path: &str, fan_in: usize, fan_out: usize) -> bool {
    if fan_in < 10 || fan_out > 5 {
        return false;
    }

    let file_name = path.rsplit('/').next().unwrap_or(path);
    let stem = file_name.split('.').next().unwrap_or(file_name);
    matches!(
        stem,
        "schema"
            | "client"
            | "errors"
            | "error"
            | "crypto"
            | "auth"
            | "types"
            | "constants"
            | "config"
    ) || path.contains("/db/schema")
        || path.contains("/db/client")
        || path.contains("/lib/errors")
        || path.contains("/lib/crypto")
}

fn hotspot_next_steps(path: &str, fan_in: usize, fan_out: usize) -> Vec<AuditNextStep> {
    let mut steps = vec![AuditNextStep::new("outline", json!({ "path": path }))];
    if fan_in > 0 {
        steps.push(AuditNextStep::new(
            "trace_deps",
            json!({ "path": path, "direction": "imported_by" }),
        ));
    }
    if fan_out > 0 {
        steps.push(AuditNextStep::new(
            "trace_deps",
            json!({ "path": path, "direction": "depends_on" }),
        ));
    }
    steps
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
