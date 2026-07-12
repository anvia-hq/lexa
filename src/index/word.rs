use hashbrown::{HashMap, HashSet};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordHit {
    pub doc_id: u32,
    pub line_num: u32,
}

pub struct WordIndex {
    index: HashMap<String, Vec<WordHit>>,
    file_words: HashMap<String, Vec<String>>,
    path_to_id: HashMap<String, u32>,
    id_to_path: Vec<String>,
    free_ids: Vec<u32>,
}

impl WordIndex {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
            file_words: HashMap::new(),
            path_to_id: HashMap::new(),
            id_to_path: Vec::new(),
            free_ids: Vec::new(),
        }
    }

    fn get_or_create_id(&mut self, path: &str) -> u32 {
        if let Some(&id) = self.path_to_id.get(path) {
            return id;
        }
        let id = if let Some(id) = self.free_ids.pop() {
            self.id_to_path[id as usize] = path.to_string();
            id
        } else {
            let id = self.id_to_path.len() as u32;
            self.id_to_path.push(path.to_string());
            id
        };
        self.path_to_id.insert(path.to_string(), id);
        id
    }

    pub fn index_file(&mut self, path: &str, content: &str) {
        self.remove_file(path);

        let doc_id = self.get_or_create_id(path);
        let mut words_set = HashSet::new();

        for (line_idx, line) in content.lines().enumerate() {
            let line_num = (line_idx + 1) as u32;
            for token in tokenize(line) {
                let hit = WordHit { doc_id, line_num };

                let entry = self.index.entry(token.clone()).or_default();
                let should_add = entry
                    .last()
                    .is_none_or(|last| last.doc_id != doc_id || last.line_num != line_num);
                if should_add {
                    entry.push(hit);
                }

                words_set.insert(token);
            }
        }

        let mut words = words_set.into_iter().collect::<Vec<_>>();
        words.sort();
        self.file_words.insert(path.to_string(), words);
    }

    pub fn remove_file(&mut self, path: &str) {
        if let Some(words) = self.file_words.remove(path) {
            if let Some(&doc_id) = self.path_to_id.get(path) {
                for word in &words {
                    if let Some(hits) = self.index.get_mut(word) {
                        hits.retain(|h| h.doc_id != doc_id);
                        if hits.is_empty() {
                            self.index.remove(word);
                        }
                    }
                }
                self.path_to_id.remove(path);
                if (doc_id as usize) < self.id_to_path.len() {
                    self.id_to_path[doc_id as usize] = String::new();
                }
                self.free_ids.push(doc_id);
            }
        }
    }

    #[cfg(test)]
    pub fn allocated_id_count(&self) -> usize {
        self.id_to_path.len()
    }

    pub fn search(&self, word: &str) -> Vec<(String, u32)> {
        let word_lower = word.to_lowercase();
        self.index
            .get(&word_lower)
            .map(|hits| {
                hits.iter()
                    .filter_map(|h| {
                        self.id_to_path
                            .get(h.doc_id as usize)
                            .filter(|p| !p.is_empty())
                            .map(|p| (p.clone(), h.line_num))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn search_prefix(&self, prefix: &str) -> Vec<(String, u32, String)> {
        let prefix_lower = prefix.to_lowercase();
        let mut results = Vec::new();
        let mut seen: hashbrown::HashSet<(u32, u32)> = hashbrown::HashSet::new();

        for (word, hits) in &self.index {
            if word.starts_with(&prefix_lower) {
                for hit in hits {
                    if seen.insert((hit.doc_id, hit.line_num)) {
                        if let Some(path) = self.id_to_path.get(hit.doc_id as usize) {
                            if !path.is_empty() {
                                results.push((path.clone(), hit.line_num, word.clone()));
                            }
                        }
                    }
                }
            }
        }

        results
    }

    pub fn file_count(&self) -> usize {
        self.path_to_id.len()
    }

    pub fn unique_word_count(&self) -> usize {
        self.index.len()
    }
}

impl Default for WordIndex {
    fn default() -> Self {
        Self::new()
    }
}

pub fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in line.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_word_search() {
        let mut idx = WordIndex::new();
        idx.index_file("a.rs", "fn hello() {}\nlet world = hello();");
        idx.index_file("b.rs", "fn other() {}");

        let results = idx.search("hello");
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|(p, _)| p == "a.rs"));
    }

    #[test]
    fn prefix_search() {
        let mut idx = WordIndex::new();
        idx.index_file("a.rs", "fn handle_request() {}\nfn handle_response() {}");

        let results = idx.search_prefix("handle");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn remove_file() {
        let mut idx = WordIndex::new();
        idx.index_file("a.rs", "fn hello() {}");
        idx.remove_file("a.rs");
        assert!(idx.search("hello").is_empty());
    }

    #[test]
    fn reindex_reuses_document_id() {
        let mut idx = WordIndex::new();
        idx.index_file("a.rs", "fn hello() {}");
        idx.index_file("a.rs", "fn goodbye() {}");

        assert_eq!(idx.allocated_id_count(), 1);
        assert!(idx.search("hello").is_empty());
        assert_eq!(idx.search("goodbye"), vec![("a.rs".to_string(), 1)]);
    }
}
