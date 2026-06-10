use super::{byte_offset_to_line, count_lines, get_node_text, Parser};
use crate::types::{FileOutline, Language, Symbol, SymbolKind};

pub struct PhpParser;

impl Parser for PhpParser {
    fn parse(&self, path: &str, source: &str) -> FileOutline {
        let mut outline = FileOutline::new(path.to_string(), Language::Php);
        outline.line_count = count_lines(source);
        outline.byte_size = source.len() as u64;

        let mut parser = tree_sitter::Parser::new();
        if parser
            .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
            .is_err()
        {
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
}

fn parse_node(node: tree_sitter::Node, source: &str, outline: &mut FileOutline) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
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
            "trait_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::TraitDef,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "namespace_use_declaration" => {
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
            _ => {
                if child.child_count() > 0 {
                    parse_node(child, source, outline);
                }
            }
        }
    }
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
    fn indexes_php_symbols_and_namespace_imports() {
        let outline = PhpParser.parse(
            "app.php",
            r#"<?php
use App\Service\Client;

function helper() {}

interface Contract {}
trait LogsThings {}

class Controller {
    public function handle() {}
}
"#,
        );

        assert_eq!(outline.language, Language::Php);
        assert!(outline
            .imports
            .iter()
            .any(|import| import.contains("Client")));
        assert!(names(&outline, SymbolKind::Function).contains(&"helper".to_string()));
        assert!(names(&outline, SymbolKind::InterfaceDef).contains(&"Contract".to_string()));
        assert!(names(&outline, SymbolKind::TraitDef).contains(&"LogsThings".to_string()));
        assert!(names(&outline, SymbolKind::ClassDef).contains(&"Controller".to_string()));
        assert!(names(&outline, SymbolKind::Method).contains(&"handle".to_string()));
    }
}
