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
                parse_node(child, source, outline);
            }
            "lexical_declaration" | "variable_declaration" => {
                parse_variable_declaration(child, source, outline);
            }
            _ => {
                if child.child_count() > 0 {
                    parse_node(child, source, outline);
                }
            }
        }
    }
}

fn parse_variable_declaration(node: tree_sitter::Node, source: &str, outline: &mut FileOutline) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        if name_node.kind() != "identifier" {
            continue;
        }

        let name = get_node_text(name_node, source).to_string();
        let line = byte_offset_to_line(source, node.start_byte());
        let value = child.child_by_field_name("value");
        let kind = if value.is_some_and(is_function_value) {
            SymbolKind::Function
        } else {
            SymbolKind::Constant
        };
        let detail = value.map(|value| value.kind().to_string());
        outline.symbols.push(Symbol {
            name,
            kind,
            line_start: line,
            line_end: byte_offset_to_line(source, node.end_byte()),
            detail,
        });
    }
}

fn is_function_value(node: tree_sitter::Node) -> bool {
    matches!(
        node.kind(),
        "arrow_function" | "function_expression" | "generator_function"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol_kind(outline: &FileOutline, name: &str) -> Option<SymbolKind> {
        outline
            .symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .map(|symbol| symbol.kind)
    }

    #[test]
    fn indexes_exported_const_function_symbols() {
        let outline = parse_js_ts(
            "middleware.ts",
            "export const authMiddleware = (req, res, next) => next();\n",
            Language::TypeScript,
            &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        );

        assert_eq!(
            symbol_kind(&outline, "authMiddleware"),
            Some(SymbolKind::Function)
        );
    }

    #[test]
    fn indexes_top_level_const_symbols_without_uppercase_filter() {
        let outline = parse_js_ts(
            "keys.ts",
            "const apiKeys = new Map();\n",
            Language::TypeScript,
            &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        );

        assert_eq!(symbol_kind(&outline, "apiKeys"), Some(SymbolKind::Constant));
    }
}
