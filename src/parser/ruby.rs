use super::{byte_offset_to_line, count_lines, get_node_text, Parser};
use crate::types::{FileOutline, Language, Symbol, SymbolKind};

pub struct RubyParser;

impl Parser for RubyParser {
    fn parse(&self, path: &str, source: &str) -> FileOutline {
        let mut outline = FileOutline::new(path.to_string(), Language::Ruby);
        outline.line_count = count_lines(source);
        outline.byte_size = source.len() as u64;

        let mut parser = tree_sitter::Parser::new();
        if parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
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
            "method" => {
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
            "singleton_method" => {
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
            "class" => {
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
            "module" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Module,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "call" => {
                if let Some(method) = child.child_by_field_name("method") {
                    let method_name = get_node_text(method, source);
                    if method_name == "require" || method_name == "require_relative" {
                        if let Some(args) = child.child_by_field_name("arguments") {
                            let text = get_node_text(args, source).to_string();
                            outline.imports.push(text);
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
    fn indexes_ruby_classes_modules_methods_and_requires() {
        let outline = RubyParser.parse(
            "app.rb",
            r#"
require "json"
require_relative "support/tool"

module Billing
end

class Invoice
  def total
    42
  end

  def self.build
    new
  end
end
"#,
        );

        assert_eq!(outline.language, Language::Ruby);
        assert!(outline.imports.iter().any(|import| import.contains("json")));
        assert!(outline
            .imports
            .iter()
            .any(|import| import.contains("support/tool")));
        assert!(names(&outline, SymbolKind::Module).contains(&"Billing".to_string()));
        assert!(names(&outline, SymbolKind::ClassDef).contains(&"Invoice".to_string()));
        assert!(names(&outline, SymbolKind::Method).contains(&"total".to_string()));
        assert!(names(&outline, SymbolKind::Method).contains(&"build".to_string()));
    }
}
