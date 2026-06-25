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
        self.reverse.remove(path);
        self.unresolved.remove(path);
    }

    pub fn get_imported_by(&self, path: &str) -> Vec<String> {
        self.reverse
            .get(path)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn get_depends_on(&self, path: &str) -> Vec<String> {
        self.forward.get(path).cloned().unwrap_or_default()
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
        self.forward
            .iter()
            .map(|(path, deps)| (path.clone(), deps.clone()))
            .collect()
    }
}

impl Default for DepGraph {
    fn default() -> Self {
        Self::new()
    }
}
