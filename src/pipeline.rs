use crate::engine::Engine;
use crate::types::SearchResult;

enum PipelineState {
    Files(Vec<String>),
    Results(Vec<SearchResult>),
}

pub fn run(engine: &Engine, pipeline: &str) -> String {
    let steps: Vec<&str> = pipeline
        .split('|')
        .map(str::trim)
        .filter(|step| !step.is_empty())
        .collect();

    if steps.is_empty() {
        return "Usage: lexa pipeline 'glob *.rs | search main | limit 5'".to_string();
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
                    return "Error: find/glob requires a pattern".to_string();
                }
                state = PipelineState::Files(engine.glob_files(&pattern));
            }
            "fuzzy" | "find_path" => {
                let pattern = args.join(" ");
                if pattern.is_empty() {
                    return "Error: fuzzy requires a pattern".to_string();
                }
                state = PipelineState::Files(
                    engine
                        .fuzzy_find(&pattern, 100)
                        .into_iter()
                        .map(|(path, _)| path)
                        .collect(),
                );
            }
            "search" => {
                let query = args.join(" ");
                if query.is_empty() {
                    return "Error: search requires a query".to_string();
                }
                state = PipelineState::Results(search_pipeline(engine, state, &query));
            }
            "filter" => {
                let pattern = args.join(" ");
                if pattern.is_empty() {
                    return "Error: filter requires a pattern".to_string();
                }
                filter_state(&mut state, &pattern);
            }
            "outline" => return render_outlines(engine, &state),
            "deps" => return render_deps(engine, &state),
            "read" => return render_reads(engine, &state),
            "sort" => sort_state(&mut state),
            "limit" => {
                let limit = args
                    .first()
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(10);
                truncate_state(&mut state, limit);
            }
            "count" => return render_count(&state),
            _ => {
                return format!(
                    "Unknown pipeline command: {cmd}\nAvailable commands: find, glob, fuzzy, search, filter, outline, deps, read, sort, limit, count"
                );
            }
        }
    }

    render_state(state)
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

fn render_state(state: PipelineState) -> String {
    match state {
        PipelineState::Files(paths) => paths.join("\n"),
        PipelineState::Results(results) => results
            .into_iter()
            .map(|result| format!("{}:{}: {}", result.path, result.line_num, result.line_text))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn render_count(state: &PipelineState) -> String {
    match state {
        PipelineState::Files(paths) => format!("{} files", paths.len()),
        PipelineState::Results(results) => format!("{} results", results.len()),
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

fn render_outlines(engine: &Engine, state: &PipelineState) -> String {
    let mut output = String::new();
    for path in paths_for_state(state) {
        if let Some(outline) = engine.get_outline(path) {
            output.push_str(&format!("{} ({} symbols):\n", path, outline.symbols.len()));
            for sym in &outline.symbols {
                output.push_str(&format!(
                    "  L{:<5} {:<12} {}\n",
                    sym.line_start, sym.kind, sym.name
                ));
            }
        }
    }
    output
}

fn render_deps(engine: &Engine, state: &PipelineState) -> String {
    let mut output = String::new();
    for path in paths_for_state(state) {
        let deps = engine.get_depends_on(path);
        let imported_by = engine.get_imported_by(path);
        output.push_str(&format!("{path}:\n"));
        if !deps.is_empty() {
            output.push_str(&format!("  depends on: {}\n", deps.join(", ")));
        }
        if !imported_by.is_empty() {
            output.push_str(&format!("  imported by: {}\n", imported_by.join(", ")));
        }
    }
    output
}

fn render_reads(engine: &Engine, state: &PipelineState) -> String {
    let mut output = String::new();
    for path in paths_for_state(state) {
        if let Some(content) = engine.read_file(path, None, None) {
            output.push_str(&format!("=== {path} ===\n{content}\n"));
        }
    }
    output
}
