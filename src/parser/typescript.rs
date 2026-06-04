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
            "import_statement" => {
                let line = byte_offset_to_line(source, child.start_byte());
                let text = get_node_text(child, source).to_string();
                if let Some(import) = import_specifier_from_statement(&text) {
                    outline.imports.push(import.clone());
                    outline.symbols.push(Symbol {
                        name: import,
                        kind: SymbolKind::Import,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                }
            }
            "export_statement" => {
                let text = get_node_text(child, source).to_string();
                if let Some(import) = import_specifier_from_statement(&text) {
                    let line = byte_offset_to_line(source, child.start_byte());
                    outline.imports.push(import.clone());
                    outline.symbols.push(Symbol {
                        name: import,
                        kind: SymbolKind::Import,
                        line_start: line,
                        line_end: byte_offset_to_line(source, child.end_byte()),
                        detail: None,
                    });
                } else {
                    parse_node(child, source, outline);
                }
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

fn import_specifier_from_statement(statement: &str) -> Option<String> {
    let statement = statement.trim_start();
    if statement.starts_with("import") {
        if let Some(from_index) = statement.rfind(" from ") {
            return extract_quoted_literal(&statement[from_index + " from ".len()..]);
        }
        return extract_quoted_literal(statement);
    }

    if statement.starts_with("export {")
        || statement.starts_with("export *")
        || statement.starts_with("export type {")
    {
        if let Some(from_index) = statement.rfind(" from ") {
            return extract_quoted_literal(&statement[from_index + " from ".len()..]);
        }
    }

    None
}

fn extract_quoted_literal(text: &str) -> Option<String> {
    let start = text.find(['"', '\''])?;
    let quote = text.as_bytes()[start] as char;
    let rest = &text[start + 1..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
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

    #[test]
    fn imports_use_module_specifiers_not_full_statements() {
        let outline = parse_js_ts(
            "providers.ts",
            r#"
import anthropicIcon from "./assets/anthropic.svg";
import type { Provider } from './contracts';
export { provider } from "../provider";
"#,
            Language::TypeScript,
            &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        );

        assert!(outline
            .imports
            .contains(&"./assets/anthropic.svg".to_string()));
        assert!(outline.imports.contains(&"./contracts".to_string()));
        assert!(outline.imports.contains(&"../provider".to_string()));
        assert!(outline.symbols.iter().any(|symbol| {
            symbol.kind == SymbolKind::Import
                && symbol.name == "./assets/anthropic.svg"
                && symbol.detail.is_none()
        }));
    }

    #[test]
    fn exported_string_values_are_not_imports() {
        let outline = parse_js_ts(
            "labels.ts",
            r#"export const label = "not-a-module";"#,
            Language::TypeScript,
            &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        );

        assert!(outline.imports.is_empty());
    }

    #[test]
    fn exported_typed_object_values_are_not_imports() {
        let outline = parse_js_ts(
            "providers.ts",
            r#"
type Provider = "a";
type Metadata = { label: string };
export const metadata: Record<
  Provider,
  Metadata
> = {
  a: {
    label: "Provider",
  },
};
"#,
            Language::TypeScript,
            &tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        );

        assert!(outline.imports.is_empty());
    }
}
