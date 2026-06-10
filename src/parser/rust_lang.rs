use super::{byte_offset_to_line, count_lines, get_node_text, Parser};
use crate::types::{FileOutline, Language, Symbol, SymbolKind};

pub struct RustParser;

impl Parser for RustParser {
    fn parse(&self, path: &str, source: &str) -> FileOutline {
        let mut outline = FileOutline::new(path.to_string(), Language::Rust);
        outline.line_count = count_lines(source);
        outline.byte_size = source.len() as u64;

        let mut parser = tree_sitter::Parser::new();
        if parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .is_err()
        {
            return outline;
        }

        let tree = match parser.parse(source, None) {
            Some(tree) => tree,
            None => return outline,
        };

        let root = tree.root_node();
        parse_rust_node(root, source, &mut outline, false);

        outline
    }
}

fn parse_rust_node(
    node: tree_sitter::Node,
    source: &str,
    outline: &mut FileOutline,
    in_impl: bool,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_item" | "function_signature_item" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    let kind = if in_impl {
                        SymbolKind::Method
                    } else {
                        SymbolKind::Function
                    };
                    let detail = child
                        .child_by_field_name("parameters")
                        .map(|n| get_node_text(n, source).to_string());
                    outline.symbols.push(Symbol {
                        name,
                        kind,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail,
                    });
                }
            }
            "struct_item" => {
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
            "enum_item" => {
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
            "trait_item" => {
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
            "impl_item" => {
                let line = byte_offset_to_line(source, child.start_byte());
                let name = if let Some(type_node) = child.child_by_field_name("type") {
                    get_node_text(type_node, source).to_string()
                } else {
                    format!("impl_{}", line)
                };
                outline.symbols.push(Symbol {
                    name,
                    kind: SymbolKind::ImplBlock,
                    line_start: line,
                    line_end: byte_offset_to_line(source, child.end_byte()),
                    detail: None,
                });
                parse_rust_node(child, source, outline, true);
            }
            "type_item" => {
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
            "macro_definition" => {
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
            "use_declaration" => {
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
            "const_item" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Constant,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "static_item" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = get_node_text(name_node, source).to_string();
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Variable,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "attribute_item" => {
                let text = get_node_text(child, source);
                if text.contains("cfg(test)") {}
            }
            "mod_item" => {
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
            _ => {
                if child.child_count() > 0 {
                    parse_rust_node(child, source, outline, in_impl);
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
    fn indexes_rust_items_imports_and_impl_methods() {
        let outline = RustParser.parse(
            "lib.rs",
            r#"
use std::fmt;

pub const LIMIT: usize = 10;
static NAME: &str = "lexa";

pub struct Engine;
enum Mode { Fast }
trait Run { fn run(&self); }
type Id = String;

macro_rules! make_id { () => {}; }

mod nested {}

impl Engine {
    pub fn new() -> Self { Self }
}

fn helper(value: usize) -> usize { value }
"#,
        );

        assert_eq!(outline.language, Language::Rust);
        assert!(outline
            .imports
            .iter()
            .any(|import| import.contains("std::fmt")));
        assert!(names(&outline, SymbolKind::Constant).contains(&"LIMIT".to_string()));
        assert!(names(&outline, SymbolKind::Variable).contains(&"NAME".to_string()));
        assert!(names(&outline, SymbolKind::StructDef).contains(&"Engine".to_string()));
        assert!(names(&outline, SymbolKind::EnumDef).contains(&"Mode".to_string()));
        assert!(names(&outline, SymbolKind::TraitDef).contains(&"Run".to_string()));
        assert!(names(&outline, SymbolKind::TypeAlias).contains(&"Id".to_string()));
        assert!(names(&outline, SymbolKind::MacroDef).contains(&"make_id".to_string()));
        assert!(names(&outline, SymbolKind::Module).contains(&"nested".to_string()));
        assert!(names(&outline, SymbolKind::ImplBlock).contains(&"Engine".to_string()));
        assert!(names(&outline, SymbolKind::Method).contains(&"new".to_string()));
        assert!(names(&outline, SymbolKind::Function).contains(&"helper".to_string()));
    }
}
