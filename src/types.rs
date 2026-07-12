use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! impl_as_str_and_display {
    ($enum:ident, $( $variant:ident => $value:literal ),+ $(,)?) => {
        impl $enum {
            pub fn as_str(&self) -> &'static str {
                match self {
                    $( Self::$variant => $value ),+
                }
            }
        }

        impl fmt::Display for $enum {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Zig,
    C,
    Cpp,
    Python,
    JavaScript,
    TypeScript,
    Rust,
    Go,
    Php,
    Ruby,
    Hcl,
    R,
    Markdown,
    Json,
    Toml,
    Yaml,
    Dart,
    Java,
    Kotlin,
    Swift,
    Svelte,
    Vue,
    Astro,
    Shell,
    Css,
    Scss,
    Sql,
    Protobuf,
    Fortran,
    LlvmIr,
    Mlir,
    Tablegen,
    Unknown,
}

impl_as_str_and_display!(Language,
    Zig => "zig",
    C => "c",
    Cpp => "cpp",
    Python => "python",
    JavaScript => "javascript",
    TypeScript => "typescript",
    Rust => "rust",
    Go => "go",
    Php => "php",
    Ruby => "ruby",
    Hcl => "hcl",
    R => "r",
    Markdown => "markdown",
    Json => "json",
    Toml => "toml",
    Yaml => "yaml",
    Dart => "dart",
    Java => "java",
    Kotlin => "kotlin",
    Swift => "swift",
    Svelte => "svelte",
    Vue => "vue",
    Astro => "astro",
    Shell => "shell",
    Css => "css",
    Scss => "scss",
    Sql => "sql",
    Protobuf => "protobuf",
    Fortran => "fortran",
    LlvmIr => "llvm_ir",
    Mlir => "mlir",
    Tablegen => "tablegen",
    Unknown => "unknown",
);

pub fn detect_language(path: &str) -> Language {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "zig" => Language::Zig,
        "c" | "h" => Language::C,
        "cpp" | "hpp" | "cc" | "hh" | "cxx" | "hxx" | "mm" => Language::Cpp,
        "py" => Language::Python,
        "js" | "jsx" | "mjs" | "cjs" => Language::JavaScript,
        "ts" | "tsx" | "mts" | "cts" => Language::TypeScript,
        "rs" => Language::Rust,
        "go" => Language::Go,
        "php" => Language::Php,
        "rb" | "rake" => Language::Ruby,
        "tf" | "tfvars" | "hcl" => Language::Hcl,
        "r" | "R" => Language::R,
        "md" => Language::Markdown,
        "json" => Language::Json,
        "toml" => Language::Toml,
        "yaml" | "yml" => Language::Yaml,
        "dart" => Language::Dart,
        "java" => Language::Java,
        "kt" => Language::Kotlin,
        "swift" => Language::Swift,
        "svelte" => Language::Svelte,
        "vue" => Language::Vue,
        "astro" => Language::Astro,
        "sh" | "bash" | "zsh" => Language::Shell,
        "css" => Language::Css,
        "scss" => Language::Scss,
        "sql" => Language::Sql,
        "proto" => Language::Protobuf,
        "f90" | "f" | "for" => Language::Fortran,
        "ll" => Language::LlvmIr,
        "mlir" => Language::Mlir,
        "td" => Language::Tablegen,
        _ => Language::Unknown,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    StructDef,
    EnumDef,
    UnionDef,
    Constant,
    Variable,
    Import,
    TestDecl,
    CommentBlock,
    TraitDef,
    ImplBlock,
    TypeAlias,
    MacroDef,
    Method,
    ClassDef,
    InterfaceDef,
    Module,
}

impl_as_str_and_display!(SymbolKind,
    Function => "function",
    StructDef => "struct",
    EnumDef => "enum",
    UnionDef => "union",
    Constant => "constant",
    Variable => "variable",
    Import => "import",
    TestDecl => "test",
    CommentBlock => "comment",
    TraitDef => "trait",
    ImplBlock => "impl",
    TypeAlias => "type_alias",
    MacroDef => "macro",
    Method => "method",
    ClassDef => "class",
    InterfaceDef => "interface",
    Module => "module",
);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line_start: u32,
    pub line_end: u32,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOutline {
    pub path: String,
    pub language: Language,
    pub line_count: u32,
    pub byte_size: u64,
    pub symbols: Vec<Symbol>,
    pub imports: Vec<String>,
}

impl FileOutline {
    pub fn new(path: String, language: Language) -> Self {
        Self {
            path,
            language,
            line_count: 0,
            byte_size: 0,
            symbols: Vec::new(),
            imports: Vec::new(),
        }
    }

    pub fn symbol_count(&self) -> usize {
        self.symbols.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnresolvedImport {
    pub path: String,
    pub import: String,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub path: String,
    pub line_num: u32,
    pub line_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolResult {
    pub path: String,
    pub symbol: Symbol,
}

#[derive(Debug, Clone)]
pub struct SymbolLocation {
    pub path: String,
    pub kind: SymbolKind,
    pub line_start: u32,
    pub line_end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub language: Language,
    pub line_count: u32,
    pub byte_size: u64,
    pub symbol_count: u32,
    #[serde(default)]
    pub modified_ms: u64,
    #[serde(default = "default_indexed_file")]
    pub indexed: bool,
}

pub struct EngineSnapshotData {
    pub outlines: Vec<(String, FileOutline)>,
    pub file_meta: Vec<(String, FileMeta)>,
    pub contents: Vec<(String, String)>,
    pub forward_deps: Vec<(String, Vec<String>)>,
}

fn default_indexed_file() -> bool {
    true
}
