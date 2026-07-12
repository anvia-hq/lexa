use crate::engine::Engine;
use hashbrown::{HashMap, HashSet};

use crate::audit::config::AuditConfig;
use crate::audit::report::{AuditActionability, AuditFinding, AuditNextStep, AuditSeverity};
use serde_json::json;

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
        let module_internal = is_internal_rust_module_cycle(&cycle);
        let finding_severity = if module_internal && severity == AuditSeverity::High {
            AuditSeverity::Warning
        } else {
            severity
        };
        let next_path = path.clone();
        let trace_path = path.clone();
        let evidence_cycle = ordered_cycle(engine, &cycle).unwrap_or_else(|| {
            let mut fallback = cycle.clone();
            fallback.push(path.clone());
            fallback
        });
        findings.push(AuditFinding {
            id: format!("architecture.cycle:{path}"),
            rule: "architecture.cycle".to_string(),
            severity: finding_severity,
            actionability: AuditActionability::Actionable,
            secondary: false,
            title: if module_internal {
                "Rust module dependency cycle detected".to_string()
            } else {
                "Import cycle detected".to_string()
            },
            path,
            line_start: None,
            line_end: None,
            message: "Files in this cycle depend on each other through parsed imports.".to_string(),
            evidence: vec![evidence_cycle.join(" -> ")],
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

fn ordered_cycle(engine: &Engine, component: &[String]) -> Option<Vec<String>> {
    let start = component.first()?;
    let members = component.iter().cloned().collect::<HashSet<_>>();
    let mut path = vec![start.clone()];
    if find_cycle_path(engine, start, start, &members, &mut path) {
        Some(path)
    } else {
        None
    }
}

fn find_cycle_path(
    engine: &Engine,
    start: &str,
    current: &str,
    members: &HashSet<String>,
    path: &mut Vec<String>,
) -> bool {
    for neighbor in engine.get_depends_on(current) {
        if !members.contains(&neighbor) {
            continue;
        }
        if neighbor == start && path.len() > 1 {
            path.push(neighbor);
            return true;
        }
        if path.contains(&neighbor) {
            continue;
        }
        path.push(neighbor.clone());
        if find_cycle_path(engine, start, &neighbor, members, path) {
            return true;
        }
        path.pop();
    }
    false
}

fn find_cycles(engine: &Engine) -> Vec<Vec<String>> {
    let mut paths = engine
        .file_map()
        .into_iter()
        .map(|(path, _)| path)
        .collect::<Vec<_>>();
    paths.sort();
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

    StronglyConnectedComponents::new(&adjacency)
        .run(&paths)
        .into_iter()
        .filter(|component| component.len() > 1)
        .take(MAX_INTERNAL_CYCLES)
        .collect()
}

struct StronglyConnectedComponents<'a> {
    adjacency: &'a HashMap<String, Vec<String>>,
    next_index: usize,
    indices: HashMap<String, usize>,
    lowlinks: HashMap<String, usize>,
    stack: Vec<String>,
    on_stack: HashSet<String>,
    components: Vec<Vec<String>>,
}

impl<'a> StronglyConnectedComponents<'a> {
    fn new(adjacency: &'a HashMap<String, Vec<String>>) -> Self {
        Self {
            adjacency,
            next_index: 0,
            indices: HashMap::new(),
            lowlinks: HashMap::new(),
            stack: Vec::new(),
            on_stack: HashSet::new(),
            components: Vec::new(),
        }
    }

    fn run(mut self, paths: &[String]) -> Vec<Vec<String>> {
        for path in paths {
            if !self.indices.contains_key(path) {
                self.visit(path);
            }
        }
        self.components.sort();
        self.components
    }

    fn visit(&mut self, path: &str) {
        let index = self.next_index;
        self.next_index += 1;
        self.indices.insert(path.to_string(), index);
        self.lowlinks.insert(path.to_string(), index);
        self.stack.push(path.to_string());
        self.on_stack.insert(path.to_string());

        for neighbor in self.adjacency.get(path).cloned().unwrap_or_default() {
            if !self.indices.contains_key(&neighbor) {
                self.visit(&neighbor);
                let neighbor_lowlink = self.lowlinks[&neighbor];
                let path_lowlink = self.lowlinks[path];
                self.lowlinks
                    .insert(path.to_string(), path_lowlink.min(neighbor_lowlink));
            } else if self.on_stack.contains(&neighbor) {
                let neighbor_index = self.indices[&neighbor];
                let path_lowlink = self.lowlinks[path];
                self.lowlinks
                    .insert(path.to_string(), path_lowlink.min(neighbor_index));
            }
        }

        if self.lowlinks[path] != self.indices[path] {
            return;
        }

        let mut component = Vec::new();
        while let Some(member) = self.stack.pop() {
            self.on_stack.remove(&member);
            let finished = member == path;
            component.push(member);
            if finished {
                break;
            }
        }
        component.sort();
        self.components.push(component);
    }
}

fn is_internal_rust_module_cycle(cycle: &[String]) -> bool {
    if cycle.iter().any(|path| !path.ends_with(".rs")) {
        return false;
    }

    cycle.iter().any(|candidate| {
        let family = candidate
            .strip_suffix("/mod.rs")
            .or_else(|| candidate.strip_suffix(".rs"))
            .unwrap_or(candidate);
        cycle.iter().all(|path| {
            path == &format!("{family}.rs")
                || path == &format!("{family}/mod.rs")
                || path.starts_with(&format!("{family}/"))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_parent_and_child_files_are_one_module_family() {
        assert!(is_internal_rust_module_cycle(&[
            "src/audit.rs".to_string(),
            "src/audit/rules.rs".to_string(),
            "src/audit/rules/size.rs".to_string(),
        ]));
        assert!(!is_internal_rust_module_cycle(&[
            "src/engine/mod.rs".to_string(),
            "src/snapshot.rs".to_string(),
        ]));
    }

    #[test]
    fn cycle_search_returns_one_component_for_overlapping_cycles() {
        let mut engine = Engine::new(4);
        engine.index_file("src/a.rs", "use crate::b;\nuse crate::c;\n");
        engine.index_file("src/b.rs", "use crate::a;\nuse crate::c;\n");
        engine.index_file("src/c.rs", "use crate::a;\n");

        let cycles = find_cycles(&engine);

        assert_eq!(cycles.len(), 1);
        assert_eq!(
            cycles[0],
            vec![
                "src/a.rs".to_string(),
                "src/b.rs".to_string(),
                "src/c.rs".to_string()
            ]
        );
    }
}
