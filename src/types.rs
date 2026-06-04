use serde::{Deserialize, Serialize};
use std::fmt;

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

impl Language {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Zig => "zig",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Php => "php",
            Self::Ruby => "ruby",
            Self::Hcl => "hcl",
            Self::R => "r",
            Self::Markdown => "markdown",
            Self::Json => "json",
            Self::Toml => "toml",
            Self::Yaml => "yaml",
            Self::Dart => "dart",
            Self::Java => "java",
            Self::Kotlin => "kotlin",
            Self::Swift => "swift",
            Self::Svelte => "svelte",
            Self::Vue => "vue",
            Self::Astro => "astro",
            Self::Shell => "shell",
            Self::Css => "css",
            Self::Scss => "scss",
            Self::Sql => "sql",
            Self::Protobuf => "protobuf",
            Self::Fortran => "fortran",
            Self::LlvmIr => "llvm_ir",
            Self::Mlir => "mlir",
            Self::Tablegen => "tablegen",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn detect_language(path: &str) -> Language {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "zig" => Language::Zig,
        "c" | "h" => Language::C,
        "cpp" | "hpp" | "cc" | "hh" | "cxx" | "hxx" | "mm" => Language::Cpp,
        "py" => Language::Python,
        "js" | "jsx" | "mjs" => Language::JavaScript,
        "ts" | "tsx" | "mts" => Language::TypeScript,
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

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::StructDef => "struct",
            Self::EnumDef => "enum",
            Self::UnionDef => "union",
            Self::Constant => "constant",
            Self::Variable => "variable",
            Self::Import => "import",
            Self::TestDecl => "test",
            Self::CommentBlock => "comment",
            Self::TraitDef => "trait",
            Self::ImplBlock => "impl",
            Self::TypeAlias => "type_alias",
            Self::MacroDef => "macro",
            Self::Method => "method",
            Self::ClassDef => "class",
            Self::InterfaceDef => "interface",
            Self::Module => "module",
        }
    }
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

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

fn default_indexed_file() -> bool {
    true
}
