use crate::types::{FileMeta, Language};
use hashbrown::{HashMap, HashSet};

pub(crate) struct ImportResolution {
    pub(crate) deps: Vec<String>,
    pub(crate) unresolved: Vec<String>,
}

pub(crate) fn resolve_imports(
    path: &str,
    imports: &[String],
    language: Language,
    file_meta: &HashMap<String, FileMeta>,
) -> ImportResolution {
    let mut deps = Vec::new();
    let mut unresolved = Vec::new();
    for import in imports {
        if language == Language::Rust {
            deps.extend(resolve_rust_import(file_meta, path, import));
        } else {
            let terms = import_terms(import);
            if let Some(candidate) = resolve_generic_import_terms(file_meta, path, &terms) {
                deps.push(candidate);
            } else if is_local_generic_import(&terms) {
                unresolved.push(import.clone());
            }
        }
    }
    deps.sort();
    deps.dedup();
    unresolved.sort();
    unresolved.dedup();
    ImportResolution { deps, unresolved }
}

fn resolve_generic_import(
    file_meta: &HashMap<String, FileMeta>,
    importer_path: &str,
    import: &str,
) -> Option<String> {
    let terms = import_terms(import);
    resolve_generic_import_terms(file_meta, importer_path, &terms)
}

fn resolve_generic_import_terms(
    file_meta: &HashMap<String, FileMeta>,
    importer_path: &str,
    terms: &[String],
) -> Option<String> {
    if terms.is_empty() {
        return None;
    }

    if let Some(candidate) = exact_import_match(file_meta, importer_path, terms) {
        return Some(candidate);
    }

    if is_local_generic_import(terms) {
        return None;
    }

    let mut best_match: Option<(i32, &str)> = None;
    for candidate in file_meta.keys() {
        if candidate == importer_path {
            continue;
        }

        for term in terms {
            let Some(score) = import_match_score(term, candidate) else {
                continue;
            };
            let should_replace = best_match.is_none_or(|(best_score, best_path)| {
                score > best_score || (score == best_score && candidate.as_str() < best_path)
            });
            if should_replace {
                best_match = Some((score, candidate));
            }
        }
    }

    best_match.map(|(_, candidate)| candidate.to_string())
}

fn resolve_rust_import(
    file_meta: &HashMap<String, FileMeta>,
    importer_path: &str,
    import: &str,
) -> Vec<String> {
    let mut deps = Vec::new();
    let mut seen = HashSet::new();

    let module_groups = rust_import_module_path_groups(importer_path, import);
    for (use_path, module_paths) in &module_groups {
        let mut group_resolved = false;
        for module_path in module_paths {
            let mut found = false;
            for candidate in rust_module_file_candidates(module_path) {
                if candidate == importer_path || !file_meta.contains_key(&candidate) {
                    continue;
                }
                if seen.insert(candidate.clone()) {
                    deps.push(candidate);
                }
                found = true;
                group_resolved = true;
                break;
            }
            if found {
                break;
            }
        }

        if !group_resolved {
            let fallback_import = format!("use {use_path};");
            if let Some(candidate) =
                resolve_generic_import(file_meta, importer_path, &fallback_import)
            {
                if seen.insert(candidate.clone()) {
                    deps.push(candidate);
                }
            }
        }
    }

    if deps.is_empty() && module_groups.is_empty() {
        if let Some(candidate) = resolve_generic_import(file_meta, importer_path, import) {
            deps.push(candidate);
        }
    }

    deps
}

fn exact_import_match(
    file_meta: &HashMap<String, FileMeta>,
    importer_path: &str,
    terms: &[String],
) -> Option<String> {
    let mut best_match: Option<(i32, String)> = None;

    for term in terms {
        for (score, candidate) in exact_import_candidates(importer_path, term) {
            if candidate == importer_path || !file_meta.contains_key(&candidate) {
                continue;
            }
            let should_replace = best_match.as_ref().is_none_or(|(best_score, best_path)| {
                score > *best_score || (score == *best_score && candidate < *best_path)
            });
            if should_replace {
                best_match = Some((score, candidate));
            }
        }
    }

    best_match.map(|(_, path)| path)
}

fn import_terms(import: &str) -> Vec<String> {
    let raw = import.trim().trim_end_matches(';').trim();
    let mut terms = Vec::new();

    if let Some(quoted) = extract_quoted(raw) {
        terms.push(quoted);
    } else if let Some(included) = extract_include(raw) {
        terms.push(included);
    } else if let Some(rest) = raw.strip_prefix("from ") {
        if let Some((module, _)) = rest.split_once(" import ") {
            terms.push(module.trim().replace('.', "/"));
        }
    } else if let Some(rest) = raw.strip_prefix("import ") {
        if let Some(module) = rest.split(|c: char| c == ',' || c.is_whitespace()).next() {
            terms.push(module.trim().replace('.', "/"));
        }
    } else if let Some(rest) = raw.strip_prefix("use ") {
        terms.extend(expand_rust_use_terms(rest));
    }

    if terms.is_empty() {
        terms.push(raw.to_string());
    }

    let mut expanded = Vec::new();
    for term in terms {
        let normalized = normalize_import_term(&term);
        if normalized.is_empty() {
            continue;
        }
        expanded.push(normalized.clone());
        if let Some(last) = normalized.rsplit('/').next() {
            expanded.push(last.to_string());
        }
    }

    expanded.sort();
    expanded.dedup();
    expanded
}

fn is_local_generic_import(terms: &[String]) -> bool {
    terms
        .iter()
        .any(|term| term.starts_with("./") || term.starts_with("../"))
}

fn rust_import_module_path_groups(importer_path: &str, import: &str) -> Vec<(String, Vec<String>)> {
    let Some(use_tree) = rust_use_tree(import) else {
        return Vec::new();
    };
    let (source_root, importer_module) = rust_source_root_and_module_path(importer_path);
    let expanded_paths = expand_rust_use_tree(use_tree);
    let mut groups = Vec::new();

    for use_path in expanded_paths {
        let module_paths =
            rust_module_paths_from_use_path(&source_root, &importer_module, &use_path);
        if !module_paths.is_empty() {
            groups.push((use_path, module_paths));
        }
    }

    groups
}

fn rust_use_tree(import: &str) -> Option<&str> {
    let raw = import.trim().trim_end_matches(';').trim();

    for (idx, _) in raw.match_indices("use") {
        let before = raw[..idx].chars().next_back();
        let after = raw[idx + "use".len()..].chars().next();
        let before_is_boundary = before.is_none_or(|ch| !is_rust_ident_char(ch));
        let after_is_boundary = after.is_some_and(char::is_whitespace);
        if before_is_boundary && after_is_boundary {
            return Some(raw[idx + "use".len()..].trim());
        }
    }

    None
}

fn is_rust_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn expand_rust_use_tree(use_tree: &str) -> Vec<String> {
    let use_tree = use_tree.trim();
    let Some((start, end)) = top_level_brace_pair(use_tree) else {
        return vec![use_tree.to_string()];
    };

    let prefix = use_tree[..start].trim();
    let suffix = use_tree[end + 1..].trim();
    let inner = &use_tree[start + 1..end];
    let mut paths = Vec::new();

    for item in split_top_level_commas(inner) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let combined = format!("{prefix}{item}{suffix}");
        paths.extend(expand_rust_use_tree(&combined));
    }

    paths
}

fn top_level_brace_pair(value: &str) -> Option<(usize, usize)> {
    let mut start = None;
    let mut depth = 0usize;

    for (idx, ch) in value.char_indices() {
        match ch {
            '{' => {
                if depth == 0 && start.is_none() {
                    start = Some(idx);
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return start.map(|start| (start, idx));
                }
            }
            _ => {}
        }
    }

    None
}

fn split_top_level_commas(value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;

    for (idx, ch) in value.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&value[start..idx]);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }

    parts.push(&value[start..]);
    parts
}

fn rust_source_root_and_module_path(path: &str) -> (String, Vec<String>) {
    let (source_root, relative) =
        if let Some((source_root, relative)) = rust_bin_source_root_and_relative(path) {
            (source_root, relative)
        } else if let Some(relative) = path.strip_prefix("src/") {
            ("src".to_string(), relative)
        } else if let Some((prefix, relative)) = path.rsplit_once("/src/") {
            (format!("{prefix}/src"), relative)
        } else if let Some((dir, filename)) = path.rsplit_once('/') {
            (dir.to_string(), filename)
        } else {
            (String::new(), path)
        };

    let module = if relative == "lib.rs" || relative == "main.rs" {
        ""
    } else if let Some(module) = relative.strip_suffix("/mod.rs") {
        module
    } else {
        strip_known_extension(relative)
    };

    let segments = module
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(ToString::to_string)
        .collect();

    (source_root, segments)
}

fn rust_bin_source_root_and_relative(path: &str) -> Option<(String, &str)> {
    let (src_prefix, bin_relative) = if let Some(relative) = path.strip_prefix("src/bin/") {
        ("src", relative)
    } else if let Some((prefix, relative)) = path.rsplit_once("/src/bin/") {
        (&path[..prefix.len() + "/src".len()], relative)
    } else {
        return None;
    };

    if let Some((target_name, target_relative)) = bin_relative.split_once('/') {
        return Some((format!("{src_prefix}/bin/{target_name}"), target_relative));
    }

    let target_name = strip_known_extension(bin_relative);
    if target_name == bin_relative {
        None
    } else {
        Some((format!("{src_prefix}/bin/{target_name}"), "main.rs"))
    }
}

fn rust_module_paths_from_use_path(
    source_root: &str,
    importer_module: &[String],
    use_path: &str,
) -> Vec<String> {
    let path = use_path
        .split(" as ")
        .next()
        .unwrap_or(use_path)
        .trim()
        .trim_end_matches("::*")
        .trim_end_matches("::self");
    let segments: Vec<&str> = path
        .split("::")
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.is_empty() {
        return Vec::new();
    }

    let mut base: Vec<String> = Vec::new();
    let mut index = 0usize;
    match segments[0] {
        "crate" => index = 1,
        "self" => {
            base.extend(importer_module.iter().cloned());
            index = 1;
        }
        "super" => {
            base.extend(importer_module.iter().cloned());
            while segments
                .get(index)
                .is_some_and(|segment| *segment == "super")
            {
                base.pop();
                index += 1;
            }
        }
        _ => {}
    }

    for segment in &segments[index..] {
        if *segment == "self" || *segment == "*" {
            continue;
        }
        base.push((*segment).to_string());
    }

    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for len in (1..=base.len()).rev() {
        let module_path = base[..len].join("/");
        let full_path = if source_root.is_empty() {
            module_path
        } else {
            format!("{source_root}/{module_path}")
        };
        if seen.insert(full_path.clone()) {
            paths.push(full_path);
        }
    }

    paths
}

fn rust_module_file_candidates(module_path: &str) -> Vec<String> {
    vec![format!("{module_path}.rs"), format!("{module_path}/mod.rs")]
}

fn expand_rust_use_terms(rest: &str) -> Vec<String> {
    let rest = rest.trim().trim_end_matches(';').trim();
    let rest = rest
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .trim_start_matches("super::");

    if let Some((prefix, group)) = rest.split_once("::{") {
        let group = group.trim_end_matches('}').trim();
        let prefix = prefix.trim();
        let mut terms = vec![prefix.to_string()];
        let base = prefix.rsplit("::").next().unwrap_or(prefix);
        terms.push(base.to_string());
        for item in group
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
        {
            if item == "self" {
                continue;
            }
            terms.push(format!("{prefix}::{item}"));
            if let Some((module_path, _name)) = item.rsplit_once("::") {
                terms.push(format!("{prefix}::{module_path}"));
                terms.push(module_path.to_string());
            }
            terms.push(item.to_string());
        }
        return terms;
    }

    if rest.starts_with('{') && rest.ends_with('}') {
        return rest
            .trim_start_matches('{')
            .trim_end_matches('}')
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToString::to_string)
            .collect();
    }

    let mut terms = vec![rest.to_string()];
    if let Some((module_path, _name)) = rest.rsplit_once("::") {
        terms.push(module_path.to_string());
    }
    terms
}

fn extract_quoted(raw: &str) -> Option<String> {
    let start = raw.find(['"', '\''])?;
    let quote = raw.as_bytes()[start] as char;
    let rest = &raw[start + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn extract_include(raw: &str) -> Option<String> {
    let start = raw.find('<')?;
    let rest = &raw[start + 1..];
    let end = rest.find('>')?;
    Some(rest[..end].to_string())
}

fn normalize_import_term(term: &str) -> String {
    let normalized = term
        .trim()
        .trim_start_matches('#')
        .trim_start_matches("include ")
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .trim_start_matches("super::")
        .trim_matches('{')
        .trim_matches('}')
        .replace("::", "/");

    if normalized.starts_with("./") || normalized.starts_with("../") {
        strip_import_resource_suffix(&normalized).to_string()
    } else {
        normalized.replace('.', "/")
    }
}

fn strip_import_resource_suffix(term: &str) -> &str {
    let query_index = term.find('?');
    let hash_index = term.find('#');
    match (query_index, hash_index) {
        (Some(query), Some(hash)) => &term[..query.min(hash)],
        (Some(index), None) | (None, Some(index)) => &term[..index],
        (None, None) => term,
    }
}

fn exact_import_candidates(importer_path: &str, term: &str) -> Vec<(i32, String)> {
    let mut bases = Vec::new();
    let mut seen_bases = HashSet::new();

    if let Some(relative) = resolve_relative_import_base(importer_path, term) {
        push_unique(&mut bases, &mut seen_bases, relative);
    } else {
        let normalized = term.trim_matches('/').to_string();
        if !normalized.is_empty() {
            push_unique(&mut bases, &mut seen_bases, normalized.clone());

            if let Some(dir) = importer_path.rsplit_once('/').map(|(dir, _)| dir) {
                push_unique(&mut bases, &mut seen_bases, format!("{dir}/{normalized}"));
            }

            if !normalized.starts_with("src/") {
                push_unique(&mut bases, &mut seen_bases, format!("src/{normalized}"));
            }
        }
    }

    let specificity = import_term_specificity(term);
    let mut candidates = Vec::new();
    let mut seen_candidates = HashSet::new();

    for base in bases {
        push_scored_candidate(
            &mut candidates,
            &mut seen_candidates,
            1200 + specificity,
            base.clone(),
        );
        push_typescript_source_extension_candidates(
            &mut candidates,
            &mut seen_candidates,
            1190 + specificity,
            &base,
        );
        for ext in IMPORT_FILE_EXTENSIONS {
            push_scored_candidate(
                &mut candidates,
                &mut seen_candidates,
                1100 + specificity,
                format!("{base}.{ext}"),
            );
        }
        for index_file in IMPORT_INDEX_FILES {
            push_scored_candidate(
                &mut candidates,
                &mut seen_candidates,
                1000 + specificity,
                format!("{base}/{index_file}"),
            );
        }
    }

    candidates
}

fn resolve_relative_import_base(importer_path: &str, term: &str) -> Option<String> {
    if !term.starts_with("./") && !term.starts_with("../") {
        return None;
    }

    let mut parts: Vec<&str> = importer_path
        .rsplit_once('/')
        .map(|(dir, _)| dir.split('/').filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();

    for part in term.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn push_unique(values: &mut Vec<String>, seen: &mut HashSet<String>, value: String) {
    if seen.insert(value.clone()) {
        values.push(value);
    }
}

fn push_scored_candidate(
    values: &mut Vec<(i32, String)>,
    seen: &mut HashSet<String>,
    score: i32,
    value: String,
) {
    if seen.insert(value.clone()) {
        values.push((score, value));
    }
}

fn import_term_specificity(term: &str) -> i32 {
    (term.matches('/').count() as i32 * 50) + term.len().min(80) as i32
}

fn import_match_score(term: &str, path: &str) -> Option<i32> {
    let term = term.trim_matches('/');
    if term.is_empty() {
        return None;
    }

    let path_stem = strip_known_extension(path);
    let specificity = import_term_specificity(term);

    if path == term {
        return Some(1000 + specificity);
    }
    if path_stem == term {
        return Some(950 + specificity);
    }
    if path.ends_with(&format!("/{term}")) {
        return Some(800 + specificity);
    }
    if path_stem.ends_with(&format!("/{term}")) {
        return Some(750 + specificity);
    }
    for ext in IMPORT_FILE_EXTENSIONS {
        if path.ends_with(&format!("/{term}.{ext}")) || path == format!("{term}.{ext}") {
            return Some(700 + specificity);
        }
    }
    for index_file in IMPORT_INDEX_FILES {
        if path.ends_with(&format!("/{term}/{index_file}"))
            || path == format!("{term}/{index_file}")
        {
            return Some(650 + specificity);
        }
    }

    None
}

const IMPORT_FILE_EXTENSIONS: &[&str] = &[
    "rs", "py", "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "go", "java", "rb", "php",
    "zig", "c", "h", "cpp", "hpp", "cc", "hh", "cxx", "hxx",
];

const IMPORT_INDEX_FILES: &[&str] = &[
    "mod.rs",
    "index.ts",
    "index.tsx",
    "index.js",
    "index.jsx",
    "__init__.py",
];

fn strip_known_extension(path: &str) -> &str {
    for ext in IMPORT_FILE_EXTENSIONS {
        if let Some(stem) = path.strip_suffix(&format!(".{ext}")) {
            return stem;
        }
    }
    path
}

fn push_typescript_source_extension_candidates(
    candidates: &mut Vec<(i32, String)>,
    seen: &mut HashSet<String>,
    score: i32,
    base: &str,
) {
    let Some((stem, source_exts)) = typescript_source_extensions_for_runtime_import(base) else {
        return;
    };
    for (index, ext) in source_exts.iter().enumerate() {
        push_scored_candidate(
            candidates,
            seen,
            score - index as i32,
            format!("{stem}.{ext}"),
        );
    }
}

fn typescript_source_extensions_for_runtime_import(
    base: &str,
) -> Option<(&str, &'static [&'static str])> {
    if let Some(stem) = base.strip_suffix(".js") {
        return Some((stem, &["ts", "tsx"]));
    }
    if let Some(stem) = base.strip_suffix(".jsx") {
        return Some((stem, &["tsx"]));
    }
    if let Some(stem) = base.strip_suffix(".mjs") {
        return Some((stem, &["mts"]));
    }
    if let Some(stem) = base.strip_suffix(".cjs") {
        return Some((stem, &["cts"]));
    }
    None
}
