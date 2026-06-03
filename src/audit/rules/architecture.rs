use crate::engine::Engine;
use hashbrown::HashSet;
use std::collections::HashMap;

use crate::audit::config::AuditConfig;
use crate::audit::report::{AuditActionability, AuditFinding, AuditNextStep, AuditSeverity};
use serde_json::json;

const MAX_CYCLE_DEPTH: usize = 32;
const MAX_INTERNAL_CYCLES: usize = 1000;

pub(crate) fn audit_cycles(
    engine: &Engine,
    config: &AuditConfig,
    findings: &mut Vec<AuditFinding>,
) {
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
        let next_path = path.clone();
        let trace_path = path.clone();
        findings.push(AuditFinding {
            id: format!("architecture.cycle:{path}"),
            rule: "architecture.cycle".to_string(),
            severity,
            actionability: AuditActionability::Actionable,
            secondary: false,
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
            next_steps: vec![
                AuditNextStep::new("outline", json!({ "path": next_path })),
                AuditNextStep::new(
                    "trace_deps",
                    json!({ "path": trace_path, "direction": "depends_on" }),
                ),
            ],
        });
    }
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
