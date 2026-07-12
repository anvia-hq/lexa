use super::value::{array, base, cols, get, row, s};
use crate::types::SymbolKind;
use serde_json::{Map, Value};

#[derive(Clone, Copy)]
pub(super) enum PathTarget<'a> {
    Rows {
        cols_key: &'a str,
        rows_key: &'a str,
        col: &'a str,
    },
    Array {
        key: &'a str,
    },
}

pub(super) fn with_rows(tool: &str, summary: Value, columns: &[&str], rows: Vec<Value>) -> Value {
    Value::Object(with_rows_map(tool, summary, columns, rows))
}

pub(super) fn with_rows_map(
    tool: &str,
    summary: Value,
    columns: &[&str],
    rows: Vec<Value>,
) -> Map<String, Value> {
    let mut map = base(tool, summary);
    insert_rows(&mut map, columns, rows);
    map
}

pub(super) fn insert_rows(map: &mut Map<String, Value>, columns: &[&str], rows: Vec<Value>) {
    if rows.is_empty() {
        return;
    }
    map.insert("cols".to_string(), cols(columns));
    map.insert("rows".to_string(), array(rows));
}

pub(super) fn apply_path_compression(map: &mut Map<String, Value>, targets: &[PathTarget<'_>]) {
    if map.contains_key("root") {
        return;
    }

    let mut paths = Vec::new();
    for target in targets {
        collect_target_paths(map, *target, &mut paths);
    }
    let Some(root) = common_path_root(&paths) else {
        return;
    };
    if !path_root_saves_tokens(&root, &paths) {
        return;
    }

    for target in targets {
        strip_target_paths(map, *target, &root);
    }
    map.insert("root".to_string(), s(root));
}

fn collect_target_paths(map: &Map<String, Value>, target: PathTarget<'_>, out: &mut Vec<String>) {
    match target {
        PathTarget::Rows {
            cols_key,
            rows_key,
            col,
        } => {
            let Some(index) = column_index(map, cols_key, col) else {
                return;
            };
            for row in map
                .get(rows_key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(path) = row
                    .as_array()
                    .and_then(|values| values.get(index))
                    .and_then(Value::as_str)
                    .filter(|path| path_is_compressible(path))
                {
                    out.push(path.to_string());
                }
            }
        }
        PathTarget::Array { key } => {
            for item in map.get(key).and_then(Value::as_array).into_iter().flatten() {
                if let Some(path) = item.as_str().filter(|path| path_is_compressible(path)) {
                    out.push(path.to_string());
                }
            }
        }
    }
}

fn strip_target_paths(map: &mut Map<String, Value>, target: PathTarget<'_>, root: &str) {
    match target {
        PathTarget::Rows {
            cols_key,
            rows_key,
            col,
        } => {
            let Some(index) = column_index(map, cols_key, col) else {
                return;
            };
            for row in map
                .get_mut(rows_key)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                let Some(values) = row.as_array_mut() else {
                    continue;
                };
                let Some(value) = values.get_mut(index) else {
                    continue;
                };
                if let Some(path) = value.as_str().and_then(|path| path.strip_prefix(root)) {
                    *value = s(path);
                }
            }
        }
        PathTarget::Array { key } => {
            for item in map
                .get_mut(key)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                if let Some(path) = item.as_str().and_then(|path| path.strip_prefix(root)) {
                    *item = s(path);
                }
            }
        }
    }
}

fn column_index(map: &Map<String, Value>, cols_key: &str, col: &str) -> Option<usize> {
    map.get(cols_key)
        .and_then(Value::as_array)?
        .iter()
        .position(|value| value.as_str() == Some(col))
}

fn common_path_root(paths: &[String]) -> Option<String> {
    if paths.len() < 2 {
        return None;
    }
    let mut prefix = paths.first()?.as_str();
    for path in &paths[1..] {
        let common_len = prefix
            .char_indices()
            .zip(path.char_indices())
            .take_while(|((_, left), (_, right))| left == right)
            .last()
            .map(|((index, ch), _)| index + ch.len_utf8())
            .unwrap_or(0);
        prefix = &prefix[..common_len];
        if prefix.is_empty() {
            return None;
        }
    }
    let slash = prefix.rfind('/')?;
    let root = &prefix[..=slash];
    if root.is_empty() || paths.iter().any(|path| path == root) {
        None
    } else {
        Some(root.to_string())
    }
}

fn path_root_saves_tokens(root: &str, paths: &[String]) -> bool {
    root.len().saturating_mul(paths.len()) > root.len().saturating_add(6)
}

fn path_is_compressible(path: &str) -> bool {
    !path.is_empty() && !path.starts_with('/') && !path.contains("://") && path.contains('/')
}

pub(super) fn file_row(file: &Value) -> Value {
    row([
        get(file, "path"),
        get(file, "language"),
        get(file, "line_count"),
        get(file, "symbol_count"),
    ])
}

pub(super) fn search_row(result: &Value) -> Value {
    row([get(result, "path"), line_value(result), text_value(result)])
}

pub(super) fn word_ref_row(result: &Value) -> Value {
    row([
        get(result, "kind"),
        get(result, "path"),
        line_value(result),
        text_value(result),
    ])
}

pub(super) fn line_value(value: &Value) -> Value {
    value
        .get("line")
        .or_else(|| value.get("line_num"))
        .cloned()
        .unwrap_or(Value::Null)
}

pub(super) fn text_value(value: &Value) -> Value {
    let raw = value
        .get("text")
        .or_else(|| value.get("line_text"))
        .cloned()
        .unwrap_or(Value::Null);
    let Some(text) = raw.as_str() else {
        return raw;
    };
    s(compact_text(text, 120))
}

fn compact_text(text: &str, max_chars: usize) -> String {
    let compacted = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compacted.chars().count() <= max_chars {
        return compacted;
    }
    let keep = max_chars.saturating_sub(3);
    format!("{}...", compacted.chars().take(keep).collect::<String>())
}

pub(super) fn kind_value(value: &Value) -> Value {
    let Some(kind) = value.get("kind").and_then(Value::as_str) else {
        return Value::Null;
    };
    if let Some(canonical) = canonical_symbol_kind(kind) {
        s(canonical)
    } else {
        s(kind.to_ascii_lowercase())
    }
}

fn canonical_symbol_kind(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "Function" => SymbolKind::Function.as_str(),
        "StructDef" => SymbolKind::StructDef.as_str(),
        "EnumDef" => SymbolKind::EnumDef.as_str(),
        "UnionDef" => SymbolKind::UnionDef.as_str(),
        "Constant" => SymbolKind::Constant.as_str(),
        "Variable" => SymbolKind::Variable.as_str(),
        "Import" => SymbolKind::Import.as_str(),
        "TestDecl" => SymbolKind::TestDecl.as_str(),
        "CommentBlock" => SymbolKind::CommentBlock.as_str(),
        "TraitDef" => SymbolKind::TraitDef.as_str(),
        "ImplBlock" => SymbolKind::ImplBlock.as_str(),
        "TypeAlias" => SymbolKind::TypeAlias.as_str(),
        "MacroDef" => SymbolKind::MacroDef.as_str(),
        "Method" => SymbolKind::Method.as_str(),
        "ClassDef" => SymbolKind::ClassDef.as_str(),
        "InterfaceDef" => SymbolKind::InterfaceDef.as_str(),
        "Module" => SymbolKind::Module.as_str(),
        _ => return None,
    })
}
