use super::{byte_offset_to_line, count_lines, get_node_text, Parser};
use crate::types::{FileOutline, Language, Symbol, SymbolKind};

pub struct ZigParser;

impl Parser for ZigParser {
    fn parse(&self, path: &str, source: &str) -> FileOutline {
        let mut outline = FileOutline::new(path.to_string(), Language::Zig);
        outline.line_count = count_lines(source);
        outline.byte_size = source.len() as u64;

        let mut parser = tree_sitter::Parser::new();
        if parser
            .set_language(&tree_sitter_zig::LANGUAGE.into())
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
                "variable_declaration" | "global_variable_declaration" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = get_node_text(name_node, source).to_string();
                        let line = byte_offset_to_line(source, child.start_byte());

                        if let Some(value) = child.child_by_field_name("value") {
                            if value.kind() == "function_declaration" {
                                let detail = get_function_signature(value, source);
                                outline.symbols.push(Symbol {
                                    name,
                                    kind: SymbolKind::Function,
                                    line_start: line,
                                    line_end: byte_offset_to_line(source, child.end_byte()),
                                    detail: Some(detail),
                                });
                                continue;
                            }
                        }

                        let first_child = child.child(0);
                        let kind = if let Some(fc) = first_child {
                            match fc.kind() {
                                "keyword_const" => SymbolKind::Constant,
                                "keyword_var" => SymbolKind::Variable,
                                "keyword_threadlocal" => SymbolKind::Variable,
                                _ => SymbolKind::Variable,
                            }
                        } else {
                            SymbolKind::Variable
                        };

                        outline.symbols.push(Symbol {
                            name,
                            kind,
                            line_start: line,
                            line_end: byte_offset_to_line(source, child.end_byte()),
                            detail: None,
                        });
                    }
                }
                "function_declaration" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = get_node_text(name_node, source).to_string();
                        let line = byte_offset_to_line(source, child.start_byte());
                        let detail = get_function_signature(child, source);
                        outline.symbols.push(Symbol {
                            name,
                            kind: SymbolKind::Function,
                            line_start: line,
                            line_end: byte_offset_to_line(source, child.end_byte()),
                            detail: Some(detail),
                        });
                    }
                }
                "test_declaration" => {
                    let line = byte_offset_to_line(source, child.start_byte());
                    let name = if let Some(name_node) = child.child_by_field_name("name") {
                        get_node_text(name_node, source).to_string()
                    } else {
                        format!("test_{}", line)
                    };
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::TestDecl,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
                "struct_declaration" | "enum_declaration" | "union_declaration"
                | "opaque_declaration" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = get_node_text(name_node, source).to_string();
                        let line = byte_offset_to_line(source, child.start_byte());
                        let kind = match child.kind() {
                            "struct_declaration" => SymbolKind::StructDef,
                            "enum_declaration" => SymbolKind::EnumDef,
                            "union_declaration" => SymbolKind::UnionDef,
                            _ => SymbolKind::StructDef,
                        };
                        outline.symbols.push(Symbol {
                            name,
                            kind,
                            line_start: line,
                            line_end: byte_offset_to_line(source, child.end_byte()),
                            detail: None,
                        });
                    }
                }
                "container_declaration" => {
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
                "import_declaration" => {
                    let line = byte_offset_to_line(source, child.start_byte());
                    let text = get_node_text(child, source);
                    if let Some(start) = text.find('"') {
                        if let Some(end) = text[start + 1..].find('"') {
                            outline
                                .imports
                                .push(text[start + 1..start + 1 + end].to_string());
                        }
                    }
                    outline.symbols.push(Symbol {
                        name: text.to_string(),
                        kind: SymbolKind::Import,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
                _ => {}
            }
        }

        outline
    }
}

fn get_function_signature(node: tree_sitter::Node, source: &str) -> String {
    let return_type = node
        .child_by_field_name("return_type")
        .map(|n| get_node_text(n, source).to_string())
        .unwrap_or_default();

    let params = node
        .child_by_field_name("parameters")
        .map(|n| get_node_text(n, source).to_string())
        .unwrap_or_else(|| "()".to_string());

    format!("fn {params} -> {return_type}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(outline: &FileOutline, kind: SymbolKind) -> Vec<String> {
        outline
            .symbols
            .iter()
            .filter(|symbol| symbol.kind == kind)
            .map(|symbol| symbol.name.clone())
            .collect()
    }

    #[test]
    fn indexes_zig_declarations_functions_and_tests() {
        let outline = ZigParser.parse(
            "main.zig",
            r#"
const std = @import("std");
var counter: usize = 0;
const Handler = struct {};
const Mode = enum { fast, slow };
const Payload = union { id: u32 };

pub fn run(value: usize) usize {
    return value + counter;
}

test "run returns value" {
    try std.testing.expect(run(1) == 1);
}
"#,
        );

        assert_eq!(outline.language, Language::Zig);
        assert!(names(&outline, SymbolKind::Function).contains(&"run".to_string()));
        assert!(outline
            .symbols
            .iter()
            .any(|symbol| symbol.kind == SymbolKind::TestDecl));
    }
}
