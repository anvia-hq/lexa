pub mod c_cpp;
pub mod go_lang;
pub mod java;
pub mod lightweight;
pub mod php;
pub mod python;
pub mod ruby;
pub mod rust_lang;
pub mod typescript;
pub mod zig;

use crate::types::{FileOutline, Language};

pub trait Parser {
    fn parse(&self, path: &str, source: &str) -> FileOutline;
}

pub fn get_parser(language: Language) -> Option<Box<dyn Parser>> {
    match language {
        Language::Zig => Some(Box::new(zig::ZigParser)),
        Language::Python => Some(Box::new(python::PythonParser)),
        Language::Rust => Some(Box::new(rust_lang::RustParser)),
        Language::TypeScript => Some(Box::new(typescript::TypeScriptParser)),
        Language::JavaScript => Some(Box::new(typescript::JavaScriptParser)),
        Language::Go => Some(Box::new(go_lang::GoParser)),
        Language::C => Some(Box::new(c_cpp::CParser)),
        Language::Cpp => Some(Box::new(c_cpp::CppParser)),
        Language::Java => Some(Box::new(java::JavaParser)),
        Language::Ruby => Some(Box::new(ruby::RubyParser)),
        Language::Php => Some(Box::new(php::PhpParser)),
        Language::Hcl => Some(Box::new(lightweight::HclParser)),
        Language::R => Some(Box::new(lightweight::RParser)),
        Language::Markdown => Some(Box::new(lightweight::MarkdownParser)),
        Language::Json => Some(Box::new(lightweight::JsonParser)),
        Language::Yaml => Some(Box::new(lightweight::YamlParser)),
        Language::Dart => Some(Box::new(lightweight::DartParser)),
        Language::Kotlin => Some(Box::new(lightweight::KotlinParser)),
        Language::Swift => Some(Box::new(lightweight::SwiftParser)),
        Language::Svelte => Some(Box::new(lightweight::SvelteParser)),
        Language::Vue => Some(Box::new(lightweight::VueParser)),
        Language::Astro => Some(Box::new(lightweight::AstroParser)),
        Language::Shell => Some(Box::new(lightweight::ShellParser)),
        Language::Css => Some(Box::new(lightweight::CssParser)),
        Language::Scss => Some(Box::new(lightweight::ScssParser)),
        Language::Sql => Some(Box::new(lightweight::SqlParser)),
        Language::Protobuf => Some(Box::new(lightweight::ProtobufParser)),
        Language::Fortran => Some(Box::new(lightweight::FortranParser)),
        Language::LlvmIr => Some(Box::new(lightweight::LlvmIrParser)),
        Language::Mlir => Some(Box::new(lightweight::MlirParser)),
        Language::Tablegen => Some(Box::new(lightweight::TablegenParser)),
        _ => None,
    }
}

pub fn parse_file(path: &str, language: Language, source: &str) -> Option<FileOutline> {
    let parser = get_parser(language)?;
    Some(parser.parse(path, source))
}

fn count_lines(source: &str) -> u32 {
    source.lines().count().max(1) as u32
}

fn byte_offset_to_line(source: &str, byte_offset: usize) -> u32 {
    let mut line = 1u32;
    for (i, ch) in source.char_indices() {
        if i >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
        }
    }
    line
}

fn get_node_text<'a>(node: tree_sitter::Node, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}
