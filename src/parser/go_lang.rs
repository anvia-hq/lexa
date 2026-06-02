use super::{byte_offset_to_line, count_lines, get_node_text, Parser};
use crate::types::{FileOutline, Language, Symbol, SymbolKind};

pub struct GoParser;

impl Parser for GoParser {
    fn parse(&self, path: &str, source: &str) -> FileOutline {
        let mut outline = FileOutline::new(path.to_string(), Language::Go);
        outline.line_count = count_lines(source);
        outline.byte_size = source.len() as u64;

        let mut parser = tree_sitter::Parser::new();
        if parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .is_err()
        {
            return outline;
        }

        let tree = match parser.parse(source, None) {
            Some(tree) => tree,
            None => return outline,
        };

        let root = tree.root_node();
        let mut cursor = root.walk();

        for child in root.children(&mut cursor) {
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
                "method_declaration" => {
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
                "type_declaration" => {
                    let mut type_cursor = child.walk();
                    for type_child in child.children(&mut type_cursor) {
                        if type_child.kind() == "type_spec" {
                            if let Some(name_node) = type_child.child_by_field_name("name") {
                                let name = get_node_text(name_node, source).to_string();
                                let line = byte_offset_to_line(source, type_child.start_byte());
                                let kind = if let Some(type_node) =
                                    type_child.child_by_field_name("type")
                                {
                                    match type_node.kind() {
                                        "struct_type" => SymbolKind::StructDef,
                                        "interface_type" => SymbolKind::InterfaceDef,
                                        _ => SymbolKind::TypeAlias,
                                    }
                                } else {
                                    SymbolKind::TypeAlias
                                };
                                outline.symbols.push(Symbol {
                                    name,
                                    kind,
                                    line_start: line,
                                    line_end: byte_offset_to_line(source, type_child.end_byte()),
                                    detail: None,
                                });
                            }
                        }
                    }
                }
                "import_declaration" => {
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
                "var_declaration" => {
                    let mut var_cursor = child.walk();
                    for var_child in child.children(&mut var_cursor) {
                        if var_child.kind() == "var_spec" {
                            if let Some(name_node) = var_child.child_by_field_name("name") {
                                let name = get_node_text(name_node, source).to_string();
                                let line = byte_offset_to_line(source, var_child.start_byte());
                                outline.symbols.push(Symbol {
                                    name,
                                    kind: SymbolKind::Variable,
                                    line_start: line,
                                    line_end: byte_offset_to_line(source, var_child.end_byte()),
                                    detail: None,
                                });
                            }
                        }
                    }
                }
                "const_declaration" => {
                    let mut const_cursor = child.walk();
                    for const_child in child.children(&mut const_cursor) {
                        if const_child.kind() == "const_spec" {
                            if let Some(name_node) = const_child.child_by_field_name("name") {
                                let name = get_node_text(name_node, source).to_string();
                                let line = byte_offset_to_line(source, const_child.start_byte());
                                outline.symbols.push(Symbol {
                                    name,
                                    kind: SymbolKind::Constant,
                                    line_start: line,
                                    line_end: byte_offset_to_line(source, const_child.end_byte()),
                                    detail: None,
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        outline
    }
}
