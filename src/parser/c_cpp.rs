use super::{byte_offset_to_line, count_lines, get_node_text, Parser};
use crate::types::{FileOutline, Language, Symbol, SymbolKind};

pub struct CParser;

impl Parser for CParser {
    fn parse(&self, path: &str, source: &str) -> FileOutline {
        parse_c_cpp(path, source, Language::C, &tree_sitter_c::LANGUAGE.into())
    }
}

pub struct CppParser;

impl Parser for CppParser {
    fn parse(&self, path: &str, source: &str) -> FileOutline {
        parse_c_cpp(
            path,
            source,
            Language::Cpp,
            &tree_sitter_cpp::LANGUAGE.into(),
        )
    }
}

fn parse_c_cpp(
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
            "function_definition" => {
                if let Some(declarator) = child.child_by_field_name("declarator") {
                    let name = extract_c_function_name(declarator, source);
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "struct_specifier" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::StructDef,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "enum_specifier" => {
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
            "union_specifier" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::UnionDef,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "type_definition" => {
                if let Some(declarator) = child.child_by_field_name("declarator") {
                    let name = extract_c_type_name(declarator, source);
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
            "preproc_include" => {
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
            "preproc_def" | "preproc_function_def" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::MacroDef,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "class_specifier" => {
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
                }
            }
            "namespace_definition" => {
                let line = byte_offset_to_line(source, child.start_byte());
                let name = if let Some(name_node) = child.child_by_field_name("name") {
                    get_node_text(name_node, source).to_string()
                } else {
                    format!("namespace_{}", line)
                };
                outline.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::Module,
                    line_start: line,
                    line_end: byte_offset_to_line(source, child.end_byte()),
                    detail: None,
                });
            }
            _ => {
                if child.child_count() > 0 {
                    parse_node(child, source, outline);
                }
            }
        }
    }
}

fn extract_c_function_name(node: tree_sitter::Node, source: &str) -> String {
    match node.kind() {
        "function_declarator" => {
            if let Some(declarator) = node.child_by_field_name("declarator") {
                return get_node_text(declarator, source).to_string();
            }
        }
        "identifier" => {
            return get_node_text(node, source).to_string();
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let name = extract_c_function_name(child, source);
        if !name.is_empty() {
            return name;
        }
    }
    String::new()
}

fn extract_c_type_name(node: tree_sitter::Node, source: &str) -> String {
    match node.kind() {
        "type_identifier" => get_node_text(node, source).to_string(),
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                let name = extract_c_type_name(child, source);
                if !name.is_empty() {
                    return name;
                }
            }
            String::new()
        }
    }
}
