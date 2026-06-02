use super::{byte_offset_to_line, count_lines, get_node_text, Parser};
use crate::types::{FileOutline, Language, Symbol, SymbolKind};

pub struct TypeScriptParser;

impl Parser for TypeScriptParser {
    fn parse(&self, path: &str, source: &str) -> FileOutline {
        parse_js_ts(
            path,
            source,
            Language::TypeScript,
            &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        )
    }
}

pub struct JavaScriptParser;

impl Parser for JavaScriptParser {
    fn parse(&self, path: &str, source: &str) -> FileOutline {
        parse_js_ts(
            path,
            source,
            Language::JavaScript,
            &tree_sitter_javascript::LANGUAGE.into(),
        )
    }
}

fn parse_js_ts(
    path: &str,
    source: &str,
    language: Language,
    ts_lang: &tree_sitter::Language,
) -> FileOutline {
    let mut outline = FileOutline::new(path.to_string(), language);
    outline.line_count = count_lines(source);
    outline.byte_size = source.len() as u64;

    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(ts_lang).is_err() {
        return outline;
    }

    let tree = match parser.parse(source, None) {
        Some(tree) => tree,
        None => return outline,
    };

    let root = tree.root_node();
    parse_node(root, source, &mut outline);

    outline
}

fn parse_node(node: tree_sitter::Node, source: &str, outline: &mut FileOutline) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    let detail = child
                        .child_by_field_name("parameters")
                        .map(|n| get_node_text(n, source).to_string());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail,
                    });
                }
            }
            "class_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::ClassDef,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                    parse_node(child, source, outline);
                }
            }
            "method_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Method,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "interface_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::InterfaceDef,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "type_alias_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::TypeAlias,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "enum_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::EnumDef,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "import_statement" | "import_clause" => {
                let line = byte_offset_to_line(source, child.start_byte());
                let text = get_node_text(child, source).to_string();
                outline.imports.push(text.clone());
                outline.symbols.push(Symbol {
                    name: text,
                    kind: SymbolKind::Import,
                    line_start: line,
                    line_end: byte_offset_to_line(source, child.end_byte()),
                    detail: None,
                });
            }
            "export_statement" => {
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    match inner.kind() {
                        "function_declaration"
                        | "class_declaration"
                        | "interface_declaration"
                        | "type_alias_declaration"
                        | "enum_declaration" => {}
                        _ => {}
                    }
                }
                parse_node(child, source, outline);
            }
            "lexical_declaration" | "variable_declaration" => {
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "variable_declarator" {
                        if let Some(name_node) = inner.child_by_field_name("name") {
                            let name = get_node_text(name_node, source).to_string();
                            let line = byte_offset_to_line(source, child.start_byte());
                            if name
                                .chars()
                                .all(|c| c.is_uppercase() || c == '_' || c.is_numeric())
                            {
                                outline.symbols.push(Symbol {
                                    name,
                                    kind: SymbolKind::Constant,
                                    line_start: line,
                                    line_end: byte_offset_to_line(source, child.end_byte()),
                                    detail: None,
                                });
                            }
                        }
                    }
                }
            }
            _ => {
                if child.child_count() > 0 {
                    parse_node(child, source, outline);
                }
            }
        }
    }
}
