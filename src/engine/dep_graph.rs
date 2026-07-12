use crate::types::UnresolvedImport;
use hashbrown::{HashMap, HashSet};

pub struct DepGraph {
    forward: HashMap<String, Vec<String>>,
    reverse: HashMap<String, HashSet<String>>,
    unresolved: HashMap<String, Vec<UnresolvedImport>>,
}

impl DepGraph {
    pub fn new() -> Self {
        Self {
            forward: HashMap::new(),
            reverse: HashMap::new(),
            unresolved: HashMap::new(),
        }
    }

    pub fn set_resolution(
        &mut self,
        path: &str,
        deps: Vec<String>,
        unresolved: Vec<UnresolvedImport>,
    ) {
        if let Some(old_deps) = self.forward.get(path) {
            for dep in old_deps {
                if let Some(set) = self.reverse.get_mut(dep) {
                    set.remove(path);
                }
            }
        }

        for dep in &deps {
            self.reverse
                .entry(dep.clone())
                .or_default()
                .insert(path.to_string());
        }
        self.forward.insert(path.to_string(), deps);
        if unresolved.is_empty() {
            self.unresolved.remove(path);
        } else {
            self.unresolved.insert(path.to_string(), unresolved);
        }
    }

    pub fn clear(&mut self) {
        self.forward.clear();
        self.reverse.clear();
        self.unresolved.clear();
    }

    pub fn remove(&mut self, path: &str) {
        if let Some(deps) = self.forward.remove(path) {
            for dep in deps {
                if let Some(set) = self.reverse.get_mut(&dep) {
                    set.remove(path);
                }
            }
        }
        if let Some(importers) = self.reverse.remove(path) {
            for importer in importers {
                if let Some(deps) = self.forward.get_mut(&importer) {
                    deps.retain(|dep| dep != path);
                }
            }
        }
        self.unresolved.remove(path);
    }

    pub fn get_imported_by(&self, path: &str) -> Vec<String> {
        self.reverse
            .get(path)
            .map(|set| {
                let mut importers: Vec<_> = set.iter().cloned().collect();
                importers.sort();
                importers
            })
            .unwrap_or_default()
    }

    pub fn get_depends_on(&self, path: &str) -> Vec<String> {
        self.forward
            .get(path)
            .map(|deps| {
                let mut deps = deps.clone();
                deps.sort();
                deps
            })
            .unwrap_or_default()
    }

    pub fn get_unresolved_imports(&self, path: &str) -> Vec<UnresolvedImport> {
        self.unresolved.get(path).cloned().unwrap_or_default()
    }

    pub fn unresolved_imports(&self) -> Vec<UnresolvedImport> {
        self.unresolved
            .values()
            .flat_map(|imports| imports.iter().cloned())
            .collect()
    }

    pub fn get_transitive(&self, path: &str, reverse: bool) -> Vec<String> {
        let mut visited = HashSet::new();
        let mut stack = vec![path.to_string()];
        let mut result = Vec::new();

        while let Some(current) = stack.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if current != path {
                result.push(current.clone());
            }

            let neighbors = if reverse {
                self.get_imported_by(&current)
            } else {
                self.get_depends_on(&current)
            };

            for neighbor in neighbors {
                if !visited.contains(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }

        result
    }

    pub(crate) fn forward_deps(&self) -> Vec<(String, Vec<String>)> {
        let mut deps: Vec<_> = self
            .forward
            .iter()
            .map(|(path, deps)| (path.clone(), deps.clone()))
            .collect();
        for (_, deps) in &mut deps {
            deps.sort();
        }
        deps.sort_by(|a, b| a.0.cmp(&b.0));
        deps
    }

    pub(crate) fn unresolved_imports_by_path(&self) -> Vec<(String, Vec<UnresolvedImport>)> {
        let mut unresolved = self
            .unresolved
            .iter()
            .map(|(path, imports)| (path.clone(), imports.clone()))
            .collect::<Vec<_>>();
        unresolved.sort_by(|left, right| left.0.cmp(&right.0));
        unresolved
    }

    pub(crate) fn from_snapshot(
        forward: Vec<(String, Vec<String>)>,
        unresolved: Vec<(String, Vec<UnresolvedImport>)>,
    ) -> Option<Self> {
        let mut unresolved_by_path = HashMap::new();
        for (path, imports) in unresolved {
            if unresolved_by_path.insert(path, imports).is_some() {
                return None;
            }
        }
        let mut graph = Self::new();
        for (path, deps) in forward {
            let path_unresolved = unresolved_by_path.get(&path).cloned().unwrap_or_default();
            if graph.forward.contains_key(&path) {
                return None;
            }
            graph.set_resolution(&path, deps, path_unresolved);
        }
        if unresolved_by_path
            .keys()
            .any(|path| !graph.forward.contains_key(path))
        {
            return None;
        }
        Some(graph)
    }
}

impl Default for DepGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_keeps_forward_and_reverse_edges_consistent() {
        let mut graph = DepGraph::new();
        graph.set_resolution("src/app.rs", vec!["src/dep.rs".to_string()], Vec::new());

        graph.remove("src/dep.rs");

        assert!(graph.get_depends_on("src/app.rs").is_empty());
        assert!(graph.get_imported_by("src/dep.rs").is_empty());
    }

    #[test]
    fn dependency_accessors_return_stable_order() {
        let mut graph = DepGraph::new();
        graph.set_resolution(
            "src/app.rs",
            vec!["src/z.rs".to_string(), "src/a.rs".to_string()],
            Vec::new(),
        );
        graph.set_resolution(
            "src/z_importer.rs",
            vec!["src/a.rs".to_string()],
            Vec::new(),
        );
        graph.set_resolution(
            "src/a_importer.rs",
            vec!["src/a.rs".to_string()],
            Vec::new(),
        );

        assert_eq!(
            graph.get_depends_on("src/app.rs"),
            vec!["src/a.rs".to_string(), "src/z.rs".to_string()]
        );
        assert_eq!(
            graph.get_imported_by("src/a.rs"),
            vec![
                "src/a_importer.rs".to_string(),
                "src/app.rs".to_string(),
                "src/z_importer.rs".to_string(),
            ]
        );
        assert_eq!(
            graph.forward_deps(),
            vec![
                (
                    "src/a_importer.rs".to_string(),
                    vec!["src/a.rs".to_string()]
                ),
                (
                    "src/app.rs".to_string(),
                    vec!["src/a.rs".to_string(), "src/z.rs".to_string()]
                ),
                (
                    "src/z_importer.rs".to_string(),
                    vec!["src/a.rs".to_string()]
                ),
            ]
        );
    }
}
