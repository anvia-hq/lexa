use crate::glob::match_glob;
use crate::store::Store;
use crate::types::*;
use hashbrown::HashSet;

use super::core::*;
use super::hash_content;
use super::shared::*;

impl Engine {
    pub fn file_map(&self) -> Vec<(String, FileMeta)> {
        let mut entries: Vec<(&String, &FileMeta)> = self.file_meta.iter().collect();
        entries.sort_by_key(|(path, _)| path.as_str());
        entries
            .into_iter()
            .map(|(path, meta)| (path.clone(), meta.clone()))
            .collect()
    }

    pub fn filtered_files(
        &self,
        options: &FileFilterOptions,
    ) -> (Vec<(String, FileMeta)>, usize, bool) {
        let language = options.language.as_ref().map(|value| value.to_lowercase());
        let path_prefix = options
            .path_prefix
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(normalize_filter_prefix);

        let mut entries =
            self.file_map()
                .into_iter()
                .filter(|(path, meta)| {
                    path_prefix.as_ref().is_none_or(|prefix| {
                        path == prefix || path.starts_with(&format!("{prefix}/"))
                    }) && options
                        .path_glob
                        .as_deref()
                        .is_none_or(|glob| match_glob(glob, path))
                        && language
                            .as_deref()
                            .is_none_or(|language| meta.language.as_str() == language)
                        && options
                            .min_lines
                            .is_none_or(|min_lines| meta.line_count >= min_lines)
                        && options
                            .max_lines
                            .is_none_or(|max_lines| meta.line_count <= max_lines)
                })
                .collect::<Vec<_>>();
        let total = entries.len();
        if let Some(max_results) = options.max_results {
            entries.truncate(max_results);
        }
        let truncated = entries.len() < total;
        (entries, total, truncated)
    }

    pub fn get_imported_by(&self, path: &str) -> Vec<String> {
        self.dep_graph.get_imported_by(path)
    }

    pub fn get_depends_on(&self, path: &str) -> Vec<String> {
        self.dep_graph.get_depends_on(path)
    }

    pub fn get_unresolved_imports(&self, path: &str) -> Vec<UnresolvedImport> {
        self.dep_graph.get_unresolved_imports(path)
    }

    pub fn unresolved_imports(&self) -> Vec<UnresolvedImport> {
        let mut imports = self.dep_graph.unresolved_imports();
        imports.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| a.line_start.cmp(&b.line_start))
                .then_with(|| a.import.cmp(&b.import))
        });
        imports
    }

    pub fn get_transitive_imported_by(&self, path: &str) -> Vec<String> {
        self.dep_graph.get_transitive(path, true)
    }

    pub fn get_transitive_depends_on(&self, path: &str) -> Vec<String> {
        self.dep_graph.get_transitive(path, false)
    }

    pub fn get_hot_files(&self, limit: usize) -> Vec<(String, FileMeta)> {
        let mut entries: Vec<(&String, &FileMeta)> = self.file_meta.iter().collect();
        entries.sort_by(|a, b| {
            b.1.modified_ms
                .cmp(&a.1.modified_ms)
                .then_with(|| b.1.byte_size.cmp(&a.1.byte_size))
                .then_with(|| a.0.cmp(b.0))
        });
        entries
            .into_iter()
            .take(limit)
            .map(|(p, m)| (p.clone(), m.clone()))
            .collect()
    }

    pub fn find_callers(&self, symbol_name: &str, max_results: usize) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let mut seen: HashSet<(String, u32)> = HashSet::new();

        let definitions = self.symbol_index.find_all(symbol_name);
        let def_locations: HashSet<(String, u32)> = definitions
            .iter()
            .map(|loc| (loc.path.clone(), loc.line_start))
            .collect();

        let occurrences = self.word_index.search(symbol_name);

        for (path, line_num) in occurrences {
            if results.len() >= max_results {
                break;
            }

            if def_locations.contains(&(path.clone(), line_num)) {
                continue;
            }

            if seen.insert((path.clone(), line_num)) {
                if let Some(line_text) = self.get_line(&path, line_num) {
                    results.push(SearchResult {
                        path,
                        line_num,
                        line_text,
                    });
                }
            }
        }

        results
    }
}

impl Engine {
    pub fn get_changes(&self, since_seq: u64) -> Vec<(String, u64, String)> {
        let changes = self.store.changes_since_detailed(since_seq);
        changes
            .into_iter()
            .map(|c| {
                let op_str = format!("{:?}", c.op);
                (c.path, c.seq, op_str)
            })
            .collect()
    }

    pub fn read_file(
        &self,
        path: &str,
        line_start: Option<u32>,
        line_end: Option<u32>,
    ) -> Option<String> {
        let stub;
        let content = if let Some(content) = self.content_for(path) {
            content
        } else {
            let meta = self.file_meta.get(path)?;
            if meta.indexed {
                return None;
            }
            stub = unindexed_file_stub(path, meta);
            &stub
        };
        let lines: Vec<&str> = content.lines().collect();

        match (line_start, line_end) {
            (Some(start), Some(end)) => {
                let start_idx = (start.saturating_sub(1)) as usize;
                let end_idx = (end as usize).min(lines.len());
                if start_idx >= lines.len() || start_idx >= end_idx {
                    return Some(String::new());
                }
                Some(lines[start_idx..end_idx].join("\n"))
            }
            (Some(start), None) => {
                let start_idx = (start.saturating_sub(1)) as usize;
                if start_idx >= lines.len() {
                    return Some(String::new());
                }
                Some(lines[start_idx..].join("\n"))
            }
            (None, Some(end)) => {
                let end_idx = (end as usize).min(lines.len());
                Some(lines[..end_idx].join("\n"))
            }
            (None, None) => Some(content.to_string()),
        }
    }

    pub fn read_file_rich(
        &self,
        path: &str,
        line_start: Option<u32>,
        line_end: Option<u32>,
        compact: bool,
        if_hash: Option<&str>,
    ) -> Option<ReadFileResult> {
        let stub;
        let content = if let Some(content) = self.content_for(path) {
            content
        } else {
            let meta = self.file_meta.get(path)?;
            if meta.indexed {
                return None;
            }
            stub = unindexed_file_stub(path, meta);
            &stub
        };
        let hash = hash_content(content);
        let hash_hex = format!("{hash:x}");
        if if_hash.is_some_and(|expected| expected.eq_ignore_ascii_case(&hash_hex)) {
            return Some(ReadFileResult {
                content: String::new(),
                hash,
                unchanged: true,
            });
        }

        let mut selected = self.read_file(path, line_start, line_end)?;
        if compact {
            let language = self
                .file_meta
                .get(path)
                .map(|meta| meta.language)
                .unwrap_or_else(|| detect_language(path));
            selected = selected
                .lines()
                .filter(|line| !is_comment_or_blank(line, language))
                .collect::<Vec<_>>()
                .join("\n");
        }

        Some(ReadFileResult {
            content: selected,
            hash,
            unchanged: false,
        })
    }
}

impl Engine {
    pub fn glob_files(&self, pattern: &str) -> Vec<String> {
        let mut results: Vec<String> = self
            .file_meta
            .keys()
            .filter(|path| match_glob(pattern, path))
            .cloned()
            .collect();
        results.sort();
        results
    }

    pub fn fuzzy_find(&self, pattern: &str, max_results: usize) -> Vec<(String, f32)> {
        let pattern_lower = pattern.to_lowercase();
        let pattern_chars: Vec<char> = pattern_lower.chars().collect();
        let mut results: Vec<(String, f32)> = Vec::new();

        for path in self.file_meta.keys() {
            let path_lower = path.to_lowercase();
            let filename = path_lower.rsplit('/').next().unwrap_or(&path_lower);

            let (score, matched) = if let Some(s) = fuzzy_match(&pattern_chars, filename) {
                (s + 10.0, true)
            } else if let Some(s) = fuzzy_match(&pattern_chars, &path_lower) {
                (s, true)
            } else {
                (0.0, false)
            };

            if matched && score > 0.0 {
                results.push((path.clone(), score));
            }
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(max_results);
        results
    }

    pub fn list_dir(&self, dir: &str) -> Vec<(String, Option<&FileMeta>)> {
        let prefix = if dir.is_empty() || dir == "." {
            String::new()
        } else {
            format!("{}/", dir.trim_end_matches('/'))
        };

        let mut dirs: HashSet<String> = HashSet::new();
        let mut files: Vec<(String, Option<&FileMeta>)> = Vec::new();

        for (path, meta) in &self.file_meta {
            if let Some(rest) = path.strip_prefix(&prefix) {
                if let Some(slash_pos) = rest.find('/') {
                    let dir_name = &rest[..slash_pos];
                    dirs.insert(dir_name.to_string());
                } else {
                    files.push((rest.to_string(), Some(meta)));
                }
            }
        }

        let mut result: Vec<(String, Option<&FileMeta>)> = Vec::new();
        let mut sorted_dirs: Vec<String> = dirs.into_iter().collect();
        sorted_dirs.sort();
        for d in sorted_dirs {
            result.push((format!("{}/", d), None));
        }
        files.sort_by(|a, b| a.0.cmp(&b.0));
        result.extend(files);

        result
    }
}

impl Engine {
    pub fn file_count(&self) -> usize {
        self.file_meta.len()
    }

    pub fn symbol_index_count(&self) -> usize {
        self.symbol_index.symbol_count()
    }

    pub fn word_index_count(&self) -> usize {
        self.word_index.unique_word_count()
    }

    pub fn word_index_file_count(&self) -> usize {
        self.word_index.file_count()
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn content(&self, path: &str) -> Option<&str> {
        self.content_for(path)
    }

    pub fn enclosing_symbol(&self, path: &str, line_num: u32) -> Option<&Symbol> {
        self.outlines.get(path).and_then(|outline| {
            outline
                .symbols
                .iter()
                .filter(|sym| {
                    sym.kind != SymbolKind::Import
                        && sym.line_start <= line_num
                        && sym.line_end >= line_num
                })
                .max_by_key(|sym| sym.line_start)
        })
    }

    pub(super) fn symbol_source_bounded(
        &self,
        path: &str,
        symbol: &Symbol,
        max_lines: u32,
    ) -> Option<(u32, u32, String)> {
        let outline = self.outlines.get(path)?;
        let start = symbol.line_start.max(1);
        let natural_end = symbol.line_end.min(outline.line_count);
        let capped_end = start
            .saturating_add(max_lines.saturating_sub(1))
            .min(natural_end)
            .max(start);
        self.read_file(path, Some(start), Some(capped_end))
            .map(|content| (start, capped_end, content))
    }

    pub(super) fn get_line(&self, path: &str, line_num: u32) -> Option<String> {
        let content = self.content_for(path)?;
        content
            .lines()
            .nth((line_num - 1) as usize)
            .map(|s| s.to_string())
    }

    pub(super) fn content_for(&self, path: &str) -> Option<&str> {
        self.contents.get(path).map(String::as_str)
    }
}

fn unindexed_file_stub(path: &str, meta: &FileMeta) -> String {
    let kind = path
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .filter(|ext| !ext.is_empty())
        .unwrap_or("unknown");
    format!(
        "unindexed {kind} file: {} bytes\npath: {path}\nmodified_ms: {}\n",
        meta.byte_size, meta.modified_ms
    )
}

pub(super) fn matches_path_glob(pattern: &str, path: &str) -> bool {
    if match_glob(pattern, path) {
        return true;
    }
    if !pattern.contains('/') {
        return match_glob(&format!("**/{pattern}"), path);
    }
    false
}
