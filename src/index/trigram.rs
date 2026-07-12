use hashbrown::HashMap;

pub type Trigram = u32;

pub struct TrigramIndex {
    index: HashMap<Trigram, Vec<u32>>,
    file_trigrams: HashMap<String, Vec<Trigram>>,
    path_to_id: HashMap<String, u32>,
    id_to_path: Vec<String>,
    free_ids: Vec<u32>,
}

impl TrigramIndex {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
            file_trigrams: HashMap::new(),
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
        let trigrams = extract_trigrams(content);

        for &tri in &trigrams {
            let posting = self.index.entry(tri).or_default();
            match posting.binary_search(&doc_id) {
                Ok(_) => {}
                Err(pos) => posting.insert(pos, doc_id),
            }
        }

        self.file_trigrams.insert(path.to_string(), trigrams);
    }

    pub fn remove_file(&mut self, path: &str) {
        if let Some(trigrams) = self.file_trigrams.remove(path) {
            if let Some(&doc_id) = self.path_to_id.get(path) {
                for tri in trigrams {
                    if let Some(posting) = self.index.get_mut(&tri) {
                        if let Ok(pos) = posting.binary_search(&doc_id) {
                            posting.remove(pos);
                        }
                        if posting.is_empty() {
                            self.index.remove(&tri);
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

    pub fn candidates(&self, query: &str) -> Vec<String> {
        if query.len() < 3 {
            return Vec::new();
        }

        let query_lower = query.to_lowercase();
        let query_bytes = query_lower.as_bytes();

        let mut query_trigrams: Vec<Trigram> = Vec::new();
        for window in query_bytes.windows(3) {
            query_trigrams.push(pack_trigram(window));
        }
        query_trigrams.sort_unstable();
        query_trigrams.dedup();

        if query_trigrams.is_empty() {
            return Vec::new();
        }

        let Some(mut postings) = query_trigrams
            .iter()
            .map(|tri| self.index.get(tri))
            .collect::<Option<Vec<_>>>()
        else {
            return Vec::new();
        };
        postings.sort_by_key(|posting| posting.len());
        let mut result_ids = postings.first().copied().cloned().unwrap_or_default();
        for posting in postings.iter().skip(1) {
            result_ids.retain(|id| posting.binary_search(id).is_ok());
            if result_ids.is_empty() {
                break;
            }
        }

        result_ids
            .into_iter()
            .filter_map(|id| {
                let path = &self.id_to_path[id as usize];
                if path.is_empty() {
                    None
                } else {
                    Some(path.clone())
                }
            })
            .collect()
    }
}

impl Default for TrigramIndex {
    fn default() -> Self {
        Self::new()
    }
}

fn pack_trigram(bytes: &[u8]) -> Trigram {
    ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | (bytes[2] as u32)
}

pub fn extract_trigrams(content: &str) -> Vec<Trigram> {
    let lower = content.to_lowercase();
    let bytes = lower.as_bytes();
    let mut trigrams = Vec::with_capacity(bytes.len().saturating_sub(2));

    for window in bytes.windows(3) {
        if window.iter().all(|&b| (0x20..0x7f).contains(&b)) {
            trigrams.push(pack_trigram(window));
        }
    }
    trigrams.sort_unstable();
    trigrams.dedup();
    trigrams
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_candidates() {
        let mut idx = TrigramIndex::new();
        idx.index_file("a.rs", "hello world function");
        idx.index_file("b.rs", "other stuff");
        idx.index_file("c.rs", "hello again");

        let results = idx.candidates("hello");
        assert!(results.contains(&"a.rs".to_string()));
        assert!(results.contains(&"c.rs".to_string()));
        assert!(!results.contains(&"b.rs".to_string()));
    }

    #[test]
    fn remove_file() {
        let mut idx = TrigramIndex::new();
        idx.index_file("a.rs", "hello world");
        idx.remove_file("a.rs");
        assert!(idx.candidates("hello").is_empty());
    }
}
