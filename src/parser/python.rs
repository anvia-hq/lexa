use super::{byte_offset_to_line, count_lines, get_node_text, Parser};
use crate::types::{FileOutline, Language, Symbol, SymbolKind};

pub struct PythonParser;

impl Parser for PythonParser {
    fn parse(&self, path: &str, source: &str) -> FileOutline {
        let mut outline = FileOutline::new(path.to_string(), Language::Python);
        outline.line_count = count_lines(source);
        outline.byte_size = source.len() as u64;

        let mut parser = tree_sitter::Parser::new();
        if parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .is_err()
        {
            return outline;
        }

        let tree = match parser.parse(source, None) {
            Some(tree) => tree,
            None => return outline,
        };

        parse_python_node(tree.root_node(), source, &mut outline, false);

        outline
    }
}

fn parse_python_node(
    node: tree_sitter::Node,
    source: &str,
    outline: &mut FileOutline,
    in_class: bool,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    let detail = child
                        .child_by_field_name("parameters")
                        .map(|n| get_node_text(n, source).to_string());
                    outline.symbols.push(Symbol {
                        name,
                        kind: if in_class {
                            SymbolKind::Method
                        } else {
                            SymbolKind::Function
                        },
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail,
                    });
                }
            }
            "class_definition" => {
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
                parse_python_node(child, source, outline, true);
            }
            "import_statement" | "import_from_statement" => {
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
            "decorated_definition" => {
                parse_python_node(child, source, outline, in_class);
            }
            "assignment" if !in_class => {
                if let Some(left) = child.child_by_field_name("left") {
                    if left.kind() == "identifier" {
                        let name = get_node_text(left, source).to_string();
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
            _ => {
                if child.child_count() > 0 && child.kind() != "class_definition" {
                    parse_python_node(child, source, outline, in_class);
                }
            }
        }
    }
}
