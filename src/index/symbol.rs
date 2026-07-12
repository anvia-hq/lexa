use crate::types::{FileOutline, SymbolIndexSnapshot, SymbolLocation};
use hashbrown::HashMap;

pub struct SymbolIndex {
    index: HashMap<String, Vec<SymbolLocation>>,
}

impl SymbolIndex {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
        }
    }

    pub fn index_file(&mut self, outline: &FileOutline) {
        self.remove_file(&outline.path);

        for sym in &outline.symbols {
            let loc = SymbolLocation {
                path: outline.path.clone(),
                kind: sym.kind,
                line_start: sym.line_start,
                line_end: sym.line_end,
            };
            self.index.entry(sym.name.clone()).or_default().push(loc);
        }
    }

    pub fn remove_file(&mut self, path: &str) {
        let empty_keys: Vec<String> = self
            .index
            .iter_mut()
            .filter_map(|(name, locs)| {
                locs.retain(|loc| loc.path != path);
                if locs.is_empty() {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in empty_keys {
            self.index.remove(&key);
        }
    }

    #[cfg(test)]
    pub fn find(&self, name: &str) -> Vec<&SymbolLocation> {
        self.index
            .get(name)
            .map(|locs| locs.iter().collect())
            .unwrap_or_default()
    }

    pub fn find_all(&self, name: &str) -> Vec<SymbolLocation> {
        self.index.get(name).cloned().unwrap_or_default()
    }

    pub fn symbol_count(&self) -> usize {
        self.index.values().map(|v| v.len()).sum()
    }

    pub(crate) fn snapshot(&self) -> SymbolIndexSnapshot {
        let mut entries = self
            .index
            .iter()
            .map(|(name, locations)| (name.clone(), locations.clone()))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        SymbolIndexSnapshot { entries }
    }

    pub(crate) fn from_snapshot(snapshot: SymbolIndexSnapshot) -> Self {
        Self {
            index: snapshot.entries.into_iter().collect(),
        }
    }
}

impl Default for SymbolIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileOutline, Symbol, SymbolKind};

    #[test]
    fn basic_symbol_lookup() {
        let mut idx = SymbolIndex::new();
        let outline = FileOutline {
            path: "src/main.rs".to_string(),
            language: crate::types::Language::Rust,
            line_count: 100,
            byte_size: 1000,
            symbols: vec![
                Symbol {
                    name: "main".to_string(),
                    kind: SymbolKind::Function,
                    line_start: 1,
                    line_end: 5,
                    detail: None,
                },
                Symbol {
                    name: "Config".to_string(),
                    kind: SymbolKind::StructDef,
                    line_start: 10,
                    line_end: 20,
                    detail: None,
                },
            ],
            imports: vec![],
        };

        idx.index_file(&outline);

        let results = idx.find("main");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, "src/main.rs");

        let results = idx.find("Config");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn remove_file() {
        let mut idx = SymbolIndex::new();
        let outline = FileOutline {
            path: "a.rs".to_string(),
            language: crate::types::Language::Rust,
            line_count: 10,
            byte_size: 100,
            symbols: vec![Symbol {
                name: "foo".to_string(),
                kind: SymbolKind::Function,
                line_start: 1,
                line_end: 3,
                detail: None,
            }],
            imports: vec![],
        };

        idx.index_file(&outline);
        assert_eq!(idx.find("foo").len(), 1);

        idx.remove_file("a.rs");
        assert!(idx.find("foo").is_empty());
    }
}
