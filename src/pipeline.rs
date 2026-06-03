use crate::engine::Engine;
use crate::types::{SearchResult, Symbol};
use serde::Serialize;
use serde_json::{json, Value};

enum PipelineState {
    Files(Vec<String>),
    Results(Vec<SearchResult>),
}

#[derive(Debug, Clone, Serialize)]
pub struct OutlineItem {
    pub path: String,
    pub symbols: Vec<Symbol>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DependencyItem {
    pub path: String,
    pub depends_on: Vec<String>,
    pub imported_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadItem {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "result_type", rename_all = "snake_case")]
pub enum PipelineOutput {
    Files { items: Vec<String> },
    Results { items: Vec<SearchResult> },
    Outlines { items: Vec<OutlineItem> },
    Deps { items: Vec<DependencyItem> },
    Reads { items: Vec<ReadItem> },
    Count { kind: String, count: usize },
    Error { message: String },
}

impl PipelineOutput {
    pub fn render(&self) -> String {
        match self {
            Self::Files { items } => items.join("\n"),
            Self::Results { items } => items
                .iter()
                .map(|result| format!("{}:{}: {}", result.path, result.line_num, result.line_text))
                .collect::<Vec<_>>()
                .join("\n"),
            Self::Outlines { items } => {
                let mut output = String::new();
                for item in items {
                    output.push_str(&format!(
                        "{} ({} symbols):\n",
                        item.path,
                        item.symbols.len()
                    ));
                    for sym in &item.symbols {
                        output.push_str(&format!(
                            "  L{:<5} {:<12} {}\n",
                            sym.line_start, sym.kind, sym.name
                        ));
                    }
                }
                output
            }
            Self::Deps { items } => {
                let mut output = String::new();
                for item in items {
                    output.push_str(&format!("{}:\n", item.path));
                    if !item.depends_on.is_empty() {
                        output.push_str(&format!("  depends on: {}\n", item.depends_on.join(", ")));
                    }
                    if !item.imported_by.is_empty() {
                        output
                            .push_str(&format!("  imported by: {}\n", item.imported_by.join(", ")));
                    }
                }
                output
            }
            Self::Reads { items } => {
                let mut output = String::new();
                for item in items {
                    output.push_str(&format!("=== {} ===\n{}\n", item.path, item.content));
                }
                output
            }
            Self::Count { kind, count } => format!("{count} {kind}"),
            Self::Error { message } => message.clone(),
        }
    }

    pub fn to_json(&self, pipeline: &str) -> Value {
        let text = self.render();
        match self {
            Self::Files { items } => json!({
                "pipeline": pipeline,
                "result_type": "files",
                "count": items.len(),
                "items": items,
                "text": text,
            }),
            Self::Results { items } => json!({
                "pipeline": pipeline,
                "result_type": "results",
                "count": items.len(),
                "items": items,
                "text": text,
            }),
            Self::Outlines { items } => json!({
                "pipeline": pipeline,
                "result_type": "outlines",
                "count": items.len(),
                "items": items,
                "text": text,
            }),
            Self::Deps { items } => json!({
                "pipeline": pipeline,
                "result_type": "deps",
                "count": items.len(),
                "items": items,
                "text": text,
            }),
            Self::Reads { items } => json!({
                "pipeline": pipeline,
                "result_type": "reads",
                "count": items.len(),
                "items": items,
                "text": text,
            }),
            Self::Count { kind, count } => json!({
                "pipeline": pipeline,
                "result_type": "count",
                "kind": kind,
                "count": count,
                "items": [],
                "text": text,
            }),
            Self::Error { message } => json!({
                "pipeline": pipeline,
                "result_type": "error",
                "count": 0,
                "items": [],
                "message": message,
                "text": text,
            }),
        }
    }
}

pub fn run_output(engine: &Engine, pipeline: &str) -> PipelineOutput {
    let steps: Vec<&str> = pipeline
        .split('|')
        .map(str::trim)
        .filter(|step| !step.is_empty())
        .collect();

    if steps.is_empty() {
        return PipelineOutput::Error {
            message: "Usage: lexa pipeline 'glob *.rs | search main | limit 5'".to_string(),
        };
    }

    let mut state = PipelineState::Files(Vec::new());

    for step in steps {
        let parts: Vec<&str> = step.split_whitespace().collect();
        let cmd = parts[0].to_lowercase();
        let args = &parts[1..];

        match cmd.as_str() {
            "find" | "glob" => {
                let pattern = args.join(" ");
                if pattern.is_empty() {
                    return error("Error: find/glob requires a pattern");
                }
                state = PipelineState::Files(engine.glob_files(&pattern));
            }
            "fuzzy" | "find_path" | "path_search" => {
                let pattern = args.join(" ");
                if pattern.is_empty() {
                    return error("Error: fuzzy requires a pattern");
                }
                state = PipelineState::Files(
                    engine
                        .fuzzy_find(&pattern, 100)
                        .into_iter()
                        .map(|(path, _)| path)
                        .collect(),
                );
            }
            "search" | "text_search" => {
                let query = args.join(" ");
                if query.is_empty() {
                    return error("Error: search requires a query");
                }
                state = PipelineState::Results(search_pipeline(engine, state, &query));
            }
            "filter" => {
                let pattern = args.join(" ");
                if pattern.is_empty() {
                    return error("Error: filter requires a pattern");
                }
                filter_state(&mut state, &pattern);
            }
            "outline" => {
                return PipelineOutput::Outlines {
                    items: collect_outlines(engine, &state),
                }
            }
            "deps" => {
                return PipelineOutput::Deps {
                    items: collect_deps(engine, &state),
                }
            }
            "read" => {
                return PipelineOutput::Reads {
                    items: collect_reads(engine, &state),
                }
            }
            "sort" => sort_state(&mut state),
            "limit" => {
                let limit = args
                    .first()
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(10);
                truncate_state(&mut state, limit);
            }
            "count" => return count_output(&state),
            _ => {
                return error(&format!(
                    "Unknown pipeline command: {cmd}\nAvailable commands: find, glob, fuzzy, path_search, search, text_search, filter, outline, deps, read, sort, limit, count"
                ));
            }
        }
    }

    state_output(state)
}

fn error(message: &str) -> PipelineOutput {
    PipelineOutput::Error {
        message: message.to_string(),
    }
}

fn search_pipeline(engine: &Engine, state: PipelineState, query: &str) -> Vec<SearchResult> {
    match state {
        PipelineState::Files(paths) if !paths.is_empty() => {
            let query_lower = query.to_lowercase();
            let mut results = Vec::new();
            for path in paths {
                if let Some(content) = engine.content(&path) {
                    for (line_idx, line) in content.lines().enumerate() {
                        if line.to_lowercase().contains(&query_lower) {
                            results.push(SearchResult {
                                path: path.clone(),
                                line_num: (line_idx + 1) as u32,
                                line_text: line.to_string(),
                            });
                        }
                    }
                }
            }
            results
        }
        _ => engine.search(query, 100),
    }
}

fn filter_state(state: &mut PipelineState, pattern: &str) {
    let pattern_lower = pattern.to_lowercase();
    match state {
        PipelineState::Files(paths) => {
            paths.retain(|path| path.to_lowercase().contains(&pattern_lower));
        }
        PipelineState::Results(results) => {
            results.retain(|result| {
                result.path.to_lowercase().contains(&pattern_lower)
                    || result.line_text.to_lowercase().contains(&pattern_lower)
            });
        }
    }
}

fn sort_state(state: &mut PipelineState) {
    match state {
        PipelineState::Files(paths) => paths.sort(),
        PipelineState::Results(results) => {
            results.sort_by(|a, b| a.path.cmp(&b.path).then(a.line_num.cmp(&b.line_num)));
        }
    }
}

fn truncate_state(state: &mut PipelineState, limit: usize) {
    match state {
        PipelineState::Files(paths) => paths.truncate(limit),
        PipelineState::Results(results) => results.truncate(limit),
    }
}

fn state_output(state: PipelineState) -> PipelineOutput {
    match state {
        PipelineState::Files(items) => PipelineOutput::Files { items },
        PipelineState::Results(items) => PipelineOutput::Results { items },
    }
}

fn count_output(state: &PipelineState) -> PipelineOutput {
    match state {
        PipelineState::Files(paths) => PipelineOutput::Count {
            kind: "files".to_string(),
            count: paths.len(),
        },
        PipelineState::Results(results) => PipelineOutput::Count {
            kind: "results".to_string(),
            count: results.len(),
        },
    }
}

fn paths_for_state(state: &PipelineState) -> Vec<&str> {
    match state {
        PipelineState::Files(paths) => paths.iter().map(String::as_str).collect(),
        PipelineState::Results(results) => {
            results.iter().map(|result| result.path.as_str()).collect()
        }
    }
}

fn collect_outlines(engine: &Engine, state: &PipelineState) -> Vec<OutlineItem> {
    paths_for_state(state)
        .into_iter()
        .filter_map(|path| {
            engine.get_outline(path).map(|outline| OutlineItem {
                path: path.to_string(),
                symbols: outline.symbols.clone(),
            })
        })
        .collect()
}

fn collect_deps(engine: &Engine, state: &PipelineState) -> Vec<DependencyItem> {
    paths_for_state(state)
        .into_iter()
        .map(|path| DependencyItem {
            path: path.to_string(),
            depends_on: engine.get_depends_on(path),
            imported_by: engine.get_imported_by(path),
        })
        .collect()
}

fn collect_reads(engine: &Engine, state: &PipelineState) -> Vec<ReadItem> {
    paths_for_state(state)
        .into_iter()
        .filter_map(|path| {
            engine.read_file(path, None, None).map(|content| ReadItem {
                path: path.to_string(),
                content,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_returns_typed_file_output() {
        let mut engine = Engine::new(4);
        engine.index_file("src/main.rs", "fn main() {}\n");

        let output = run_output(&engine, "glob src/*.rs | limit 1");

        match output {
            PipelineOutput::Files { items } => assert_eq!(items, vec!["src/main.rs"]),
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[test]
    fn pipeline_returns_typed_count_output() {
        let mut engine = Engine::new(4);
        engine.index_file("src/main.rs", "fn main() {}\n");

        let output = run_output(&engine, "glob src/*.rs | count");

        match output {
            PipelineOutput::Count { kind, count } => {
                assert_eq!(kind, "files");
                assert_eq!(count, 1);
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }
}
