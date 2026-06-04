use super::{count_lines, Parser};
use crate::types::{FileOutline, Language, Symbol, SymbolKind};

macro_rules! lightweight_parser {
    ($name:ident, $language:expr) => {
        pub struct $name;

        impl Parser for $name {
            fn parse(&self, path: &str, source: &str) -> FileOutline {
                parse_lightweight(path, source, $language)
            }
        }
    };
}

lightweight_parser!(HclParser, Language::Hcl);
lightweight_parser!(RParser, Language::R);
lightweight_parser!(MarkdownParser, Language::Markdown);
lightweight_parser!(JsonParser, Language::Json);
lightweight_parser!(TomlParser, Language::Toml);
lightweight_parser!(YamlParser, Language::Yaml);
lightweight_parser!(DartParser, Language::Dart);
lightweight_parser!(KotlinParser, Language::Kotlin);
lightweight_parser!(SwiftParser, Language::Swift);
lightweight_parser!(SvelteParser, Language::Svelte);
lightweight_parser!(VueParser, Language::Vue);
lightweight_parser!(AstroParser, Language::Astro);
lightweight_parser!(ShellParser, Language::Shell);
lightweight_parser!(CssParser, Language::Css);
lightweight_parser!(ScssParser, Language::Scss);
lightweight_parser!(SqlParser, Language::Sql);
lightweight_parser!(ProtobufParser, Language::Protobuf);
lightweight_parser!(FortranParser, Language::Fortran);
lightweight_parser!(LlvmIrParser, Language::LlvmIr);
lightweight_parser!(MlirParser, Language::Mlir);
lightweight_parser!(TablegenParser, Language::Tablegen);

fn parse_lightweight(path: &str, source: &str, language: Language) -> FileOutline {
    if language == Language::Json && path.ends_with("package.json") {
        return parse_package_json(path, source);
    }
    if language == Language::Toml {
        return parse_toml(path, source);
    }

    let mut outline = FileOutline::new(path.to_string(), language);
    outline.line_count = count_lines(source);
    outline.byte_size = source.len() as u64;
    let mut yaml_stack: Vec<(usize, String)> = Vec::new();

    for (idx, raw_line) in source.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        match language {
            Language::Hcl => parse_hcl(line, line_num, &mut outline),
            Language::R => parse_r(line, line_num, &mut outline),
            Language::Markdown => parse_markdown(line, line_num, &mut outline),
            Language::Json => parse_json(line, line_num, &mut outline),
            Language::Toml => {}
            Language::Yaml => parse_yaml(raw_line, line_num, &mut outline, &mut yaml_stack),
            Language::Dart => parse_dart(line, line_num, &mut outline),
            Language::Kotlin => parse_kotlin(line, line_num, &mut outline),
            Language::Swift => parse_swift(line, line_num, &mut outline),
            Language::Svelte | Language::Vue | Language::Astro => {
                parse_web_component(line, line_num, &mut outline)
            }
            Language::Shell => parse_shell(line, line_num, &mut outline),
            Language::Css | Language::Scss => {
                parse_css_like(line, line_num, language, &mut outline)
            }
            Language::Sql => parse_sql(line, line_num, &mut outline),
            Language::Protobuf => parse_protobuf(line, line_num, &mut outline),
            Language::Fortran => parse_fortran(line, line_num, &mut outline),
            Language::LlvmIr => parse_llvm_ir(line, line_num, &mut outline),
            Language::Mlir => parse_mlir(line, line_num, &mut outline),
            Language::Tablegen => parse_tablegen(line, line_num, &mut outline),
            _ => {}
        }
    }

    infer_symbol_ranges(&mut outline);
    outline
}

fn parse_hcl(line: &str, line_num: u32, outline: &mut FileOutline) {
    if starts_comment(line) {
        return;
    }
    let Some(keyword) = first_token(line) else {
        return;
    };
    let quoted = quoted_strings(line);
    match keyword {
        "resource" | "data" => {
            if let [kind, name, ..] = quoted.as_slice() {
                push(
                    outline,
                    name.clone(),
                    SymbolKind::StructDef,
                    line_num,
                    Some(format!("{keyword} {kind}")),
                );
            }
        }
        "variable" => {
            if let Some(name) = quoted.first() {
                push(outline, name.clone(), SymbolKind::Variable, line_num, None);
            }
        }
        "output" => {
            if let Some(name) = quoted.first() {
                push(outline, name.clone(), SymbolKind::Constant, line_num, None);
            }
        }
        "module" | "provider" => {
            if let Some(name) = quoted.first() {
                push(
                    outline,
                    name.clone(),
                    SymbolKind::Module,
                    line_num,
                    Some(keyword.to_string()),
                );
            }
        }
        "locals" => {
            push(
                outline,
                "locals".to_string(),
                SymbolKind::Module,
                line_num,
                None,
            );
        }
        _ => {
            if let Some((name, _)) = line.split_once('=') {
                let name = clean_ident(name);
                if !name.is_empty() {
                    push(outline, name, SymbolKind::Constant, line_num, None);
                }
            }
        }
    }
}

fn parse_r(line: &str, line_num: u32, outline: &mut FileOutline) {
    if starts_comment(line) {
        return;
    }
    if let Some(import) = call_arg(line, "library").or_else(|| call_arg(line, "require")) {
        outline.imports.push(import.clone());
        push(outline, import, SymbolKind::Import, line_num, None);
        return;
    }
    if let Some((name, _)) = line
        .split_once("<-")
        .or_else(|| line.split_once('='))
        .filter(|(_, rhs)| rhs.trim_start().starts_with("function"))
    {
        push(
            outline,
            clean_ident(name),
            SymbolKind::Function,
            line_num,
            None,
        );
    } else if let Some(name) = quoted_call_arg(line, "setClass") {
        push(outline, name, SymbolKind::ClassDef, line_num, None);
    }
}

fn parse_markdown(line: &str, line_num: u32, outline: &mut FileOutline) {
    let level = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&level) && line.as_bytes().get(level) == Some(&b' ') {
        let name = line[level..].trim().trim_matches('#').trim();
        if !name.is_empty() {
            push(
                outline,
                name.to_string(),
                SymbolKind::Module,
                line_num,
                Some(format!("h{level}")),
            );
        }
    }
}

fn parse_json(line: &str, line_num: u32, outline: &mut FileOutline) {
    if !line.starts_with('"') {
        return;
    }
    if let Some(end) = line[1..].find('"') {
        let rest = line[1 + end + 1..].trim_start();
        if rest.starts_with(':') {
            push(
                outline,
                line[1..1 + end].to_string(),
                SymbolKind::Constant,
                line_num,
                None,
            );
        }
    }
}

fn parse_package_json(path: &str, source: &str) -> FileOutline {
    let mut outline = FileOutline::new(path.to_string(), Language::Json);
    outline.line_count = count_lines(source);
    outline.byte_size = source.len() as u64;
    let manifest_sections = [
        "scripts",
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "workspaces",
        "exports",
        "bin",
    ];
    let mut active_section: Option<String> = None;

    for (idx, raw_line) in source.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        let line = raw_line.trim();
        if line.starts_with('}') || line.starts_with(']') {
            active_section = None;
            continue;
        }

        let Some(name) = json_key(line) else {
            if let Some(section) = active_section.as_deref() {
                if section == "workspaces" {
                    if let Some(value) = json_string_value(line) {
                        push(
                            &mut outline,
                            value,
                            SymbolKind::Module,
                            line_num,
                            Some("workspace".to_string()),
                        );
                    }
                }
            }
            continue;
        };

        if manifest_sections.contains(&name.as_str()) {
            push(
                &mut outline,
                name.clone(),
                SymbolKind::Module,
                line_num,
                Some("package manifest section".to_string()),
            );
            active_section = Some(name);
            continue;
        }

        match active_section.as_deref() {
            Some("scripts") => push(
                &mut outline,
                name,
                SymbolKind::Constant,
                line_num,
                Some("npm script".to_string()),
            ),
            Some(section @ ("dependencies" | "devDependencies" | "peerDependencies")) => push(
                &mut outline,
                name,
                SymbolKind::Constant,
                line_num,
                Some(manifest_dependency_detail(section).to_string()),
            ),
            Some("exports") | Some("bin") => push(
                &mut outline,
                name,
                SymbolKind::Constant,
                line_num,
                active_section.clone(),
            ),
            Some("workspaces") => push(
                &mut outline,
                name,
                SymbolKind::Module,
                line_num,
                Some("workspace".to_string()),
            ),
            _ => {}
        }
    }

    infer_symbol_ranges(&mut outline);
    outline
}

fn manifest_dependency_detail(section: &str) -> &'static str {
    match section {
        "devDependencies" => "dev dependency",
        "peerDependencies" => "peer dependency",
        _ => "dependency",
    }
}

fn parse_toml(path: &str, source: &str) -> FileOutline {
    let mut outline = FileOutline::new(path.to_string(), Language::Toml);
    outline.line_count = count_lines(source);
    outline.byte_size = source.len() as u64;
    let mut section = String::new();

    for (idx, raw_line) in source.lines().enumerate() {
        let line_num = (idx + 1) as u32;
        let line = raw_line.trim();
        if line.is_empty() || starts_comment(line) {
            continue;
        }

        if let Some(name) = toml_section(line) {
            section = name.clone();
            push(
                &mut outline,
                name,
                SymbolKind::Module,
                line_num,
                Some("toml section".to_string()),
            );
            continue;
        }

        let Some((key, _)) = line.split_once('=') else {
            continue;
        };
        let key = clean_ident(key);
        if key.is_empty() {
            continue;
        }

        if path.ends_with("Cargo.toml") {
            parse_cargo_toml_key(&mut outline, &section, &key, line, line_num);
        } else {
            let detail = (!section.is_empty()).then(|| section.clone());
            push(&mut outline, key, SymbolKind::Constant, line_num, detail);
        }
    }

    infer_symbol_ranges(&mut outline);
    outline
}

fn parse_cargo_toml_key(
    outline: &mut FileOutline,
    section: &str,
    key: &str,
    line: &str,
    line_num: u32,
) {
    match section {
        "package" if matches!(key, "name" | "version" | "edition") => push(
            outline,
            key.to_string(),
            SymbolKind::Constant,
            line_num,
            Some("package metadata".to_string()),
        ),
        "dependencies" | "dev-dependencies" | "build-dependencies" => push(
            outline,
            key.to_string(),
            SymbolKind::Import,
            line_num,
            Some(section.to_string()),
        ),
        "features" => push(
            outline,
            key.to_string(),
            SymbolKind::Constant,
            line_num,
            Some("feature".to_string()),
        ),
        "workspace" if key == "members" => {
            for member in quoted_strings(line) {
                push(
                    outline,
                    member,
                    SymbolKind::Module,
                    line_num,
                    Some("workspace member".to_string()),
                );
            }
        }
        section
            if (section.starts_with("bin")
                || section.starts_with("example")
                || section.starts_with("test")
                || section.starts_with("bench"))
                && key == "name" =>
        {
            if let Some(name) = toml_string_value(line) {
                push(
                    outline,
                    name,
                    SymbolKind::Module,
                    line_num,
                    Some(section.to_string()),
                );
            }
        }
        _ => {}
    }
}

fn parse_yaml(
    raw_line: &str,
    line_num: u32,
    outline: &mut FileOutline,
    stack: &mut Vec<(usize, String)>,
) {
    let indent = raw_line.chars().take_while(|c| *c == ' ').count();
    let line = raw_line.trim();
    if starts_comment(line) {
        return;
    }
    if let Some((name, _)) = line.split_once(':') {
        let name = clean_ident(name);
        if !name.is_empty() {
            while stack.last().is_some_and(|(level, _)| *level >= indent) {
                stack.pop();
            }
            stack.push((indent, name));
            let full_name = stack
                .iter()
                .map(|(_, key)| key.as_str())
                .collect::<Vec<_>>()
                .join(".");
            push(outline, full_name, SymbolKind::Constant, line_num, None);
        }
    }
}

fn parse_dart(line: &str, line_num: u32, outline: &mut FileOutline) {
    if let Some(import) = quoted_import(line, &["import", "export", "part"]) {
        outline.imports.push(import.clone());
        push(outline, import, SymbolKind::Import, line_num, None);
        return;
    }
    if let Some(name) = after_keyword(line, "typedef") {
        push(outline, name, SymbolKind::TypeAlias, line_num, None);
    } else if let Some(name) = after_keyword(line, "class")
        .or_else(|| after_keyword(line, "mixin"))
        .or_else(|| after_keyword(line, "extension"))
    {
        push(outline, name, SymbolKind::ClassDef, line_num, None);
    } else if let Some(name) = after_keyword(line, "enum") {
        push(outline, name, SymbolKind::EnumDef, line_num, None);
    } else if let Some(name) = after_any_keyword(line, &["const", "final", "var"]) {
        push(outline, name, SymbolKind::Variable, line_num, None);
    } else if let Some(name) = function_name_before_paren(line) {
        push(outline, name, SymbolKind::Function, line_num, None);
    }
}

fn parse_kotlin(line: &str, line_num: u32, outline: &mut FileOutline) {
    if let Some(rest) = line.strip_prefix("import ") {
        let import = rest.trim_end_matches(';').to_string();
        outline.imports.push(import.clone());
        push(outline, import, SymbolKind::Import, line_num, None);
    } else if let Some(name) =
        after_keyword(line, "enum class").or_else(|| after_keyword(line, "enum"))
    {
        push(outline, name, SymbolKind::EnumDef, line_num, None);
    } else if let Some(name) = after_keyword(line, "interface") {
        push(outline, name, SymbolKind::InterfaceDef, line_num, None);
    } else if let Some(name) = after_any_keyword(line, &["class", "object"]) {
        push(outline, name, SymbolKind::ClassDef, line_num, None);
    } else if let Some(name) = after_keyword(line, "fun") {
        push(outline, name, SymbolKind::Function, line_num, None);
    } else if let Some(name) = after_any_keyword(line, &["val", "var"]) {
        push(outline, name, SymbolKind::Constant, line_num, None);
    }
}

fn parse_swift(line: &str, line_num: u32, outline: &mut FileOutline) {
    if let Some(rest) = line.strip_prefix("import ") {
        let import = rest.trim().to_string();
        outline.imports.push(import.clone());
        push(outline, import, SymbolKind::Import, line_num, None);
    } else if let Some(name) = after_keyword(line, "struct") {
        push(outline, name, SymbolKind::StructDef, line_num, None);
    } else if let Some(name) = after_keyword(line, "class") {
        push(outline, name, SymbolKind::ClassDef, line_num, None);
    } else if let Some(name) = after_keyword(line, "enum") {
        push(outline, name, SymbolKind::EnumDef, line_num, None);
    } else if let Some(name) = after_keyword(line, "protocol") {
        push(outline, name, SymbolKind::InterfaceDef, line_num, None);
    } else if let Some(name) = after_keyword(line, "func") {
        push(outline, name, SymbolKind::Function, line_num, None);
    } else if let Some(name) = after_any_keyword(line, &["let", "var"]) {
        push(outline, name, SymbolKind::Variable, line_num, None);
    }
}

fn parse_web_component(line: &str, line_num: u32, outline: &mut FileOutline) {
    if line.starts_with('<') || line.starts_with("</") || line == "---" {
        return;
    }
    if let Some(import) = quoted_import(line, &["import", "export"]) {
        outline.imports.push(import.clone());
        push(outline, import, SymbolKind::Import, line_num, None);
    } else if let Some(name) = export_binding_name(line) {
        push(outline, name, SymbolKind::Constant, line_num, None);
    } else if let Some(name) = after_keyword(line, "function")
        .or_else(|| after_keyword(line, "class"))
        .or_else(|| function_name_before_paren(line))
    {
        push(outline, name, SymbolKind::Function, line_num, None);
    } else if let Some(name) = after_any_keyword(line, &["const", "let", "var"]) {
        push(outline, name, SymbolKind::Constant, line_num, None);
    } else if let Some(selector) = css_selector_name(line) {
        push(outline, selector, SymbolKind::ClassDef, line_num, None);
    }
}

fn parse_shell(line: &str, line_num: u32, outline: &mut FileOutline) {
    if starts_comment(line) {
        return;
    }
    if let Some(rest) = line
        .strip_prefix("source ")
        .or_else(|| line.strip_prefix(". "))
    {
        let import = rest.split_whitespace().next().unwrap_or("").to_string();
        if !import.is_empty() {
            outline.imports.push(import.clone());
            push(outline, import, SymbolKind::Import, line_num, None);
        }
    } else if let Some(name) = line.strip_prefix("function ").map(clean_ident) {
        push(outline, name, SymbolKind::Function, line_num, None);
    } else if let Some((name, rest)) = line.split_once("()") {
        if rest.trim_start().starts_with('{') {
            push(
                outline,
                clean_ident(name),
                SymbolKind::Function,
                line_num,
                None,
            );
        }
    }
}

fn parse_css_like(line: &str, line_num: u32, language: Language, outline: &mut FileOutline) {
    if line.starts_with("/*") || line.starts_with('*') {
        return;
    }
    if let Some(import) = quoted_import(line, &["@import", "@use", "@forward"]) {
        outline.imports.push(import.clone());
        push(outline, import, SymbolKind::Import, line_num, None);
    } else if let Some(name) = directive_name(line, "@mixin")
        .or_else(|| directive_name(line, "@function"))
        .or_else(|| directive_name(line, "@keyframes"))
    {
        push(outline, name, SymbolKind::Function, line_num, None);
    } else if (language == Language::Scss && line.starts_with('$')) || line.starts_with("--") {
        if let Some((name, _)) = line.split_once(':') {
            push(
                outline,
                clean_ident(name),
                SymbolKind::Variable,
                line_num,
                None,
            );
        }
    } else if let Some(selector) = css_selector_name(line) {
        push(outline, selector, SymbolKind::ClassDef, line_num, None);
    }
}

fn parse_sql(line: &str, line_num: u32, outline: &mut FileOutline) {
    let lower = line.to_ascii_lowercase();
    if lower.starts_with("--") {
        return;
    }
    for prefix in [
        "create table",
        "create temporary table",
        "create view",
        "create function",
        "create or replace function",
        "create procedure",
        "create or replace procedure",
        "create index",
        "create unique index",
        "alter table",
    ] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            let offset = line.len() - rest.len();
            let name = clean_sql_ident(line[offset..].split_whitespace().next().unwrap_or(""));
            if !name.is_empty() {
                let kind = if prefix.contains("table") || prefix.contains("view") {
                    SymbolKind::StructDef
                } else if prefix.contains("index") {
                    SymbolKind::Constant
                } else {
                    SymbolKind::Function
                };
                push(outline, name, kind, line_num, Some(prefix.to_string()));
            }
            return;
        }
    }
}

fn parse_protobuf(line: &str, line_num: u32, outline: &mut FileOutline) {
    if let Some(import) = quoted_import(line, &["import"]) {
        outline.imports.push(import.clone());
        push(outline, import, SymbolKind::Import, line_num, None);
    } else if let Some(name) = after_any_keyword(line, &["message", "service"]) {
        push(outline, name, SymbolKind::StructDef, line_num, None);
    } else if let Some(name) = after_keyword(line, "enum") {
        push(outline, name, SymbolKind::EnumDef, line_num, None);
    } else if let Some(name) = after_keyword(line, "rpc") {
        push(outline, name, SymbolKind::Function, line_num, None);
    }
}

fn parse_fortran(line: &str, line_num: u32, outline: &mut FileOutline) {
    let lower = line.to_ascii_lowercase();
    if lower.starts_with('!') || lower.starts_with("end ") {
        return;
    }
    if let Some(rest) = lower.strip_prefix("use ") {
        let import = clean_ident(rest.split(',').next().unwrap_or(""));
        outline.imports.push(import.clone());
        push(outline, import, SymbolKind::Import, line_num, None);
    } else if let Some(name) = fortran_type_name(line) {
        push(outline, name, SymbolKind::StructDef, line_num, None);
    } else if let Some(name) =
        after_keyword(line, "module").or_else(|| after_keyword(line, "program"))
    {
        push(outline, name, SymbolKind::Module, line_num, None);
    } else if let Some(name) = after_any_keyword(line, &["subroutine", "function"]) {
        push(outline, name, SymbolKind::Function, line_num, None);
    }
}

fn parse_llvm_ir(line: &str, line_num: u32, outline: &mut FileOutline) {
    if line.starts_with("define ") || line.starts_with("declare ") {
        if let Some(name) = after_at_name(line) {
            push(outline, name, SymbolKind::Function, line_num, None);
        }
    } else if line.starts_with('@') {
        if let Some(name) = after_at_name(line) {
            push(outline, name, SymbolKind::Variable, line_num, None);
        }
    }
}

fn parse_mlir(line: &str, line_num: u32, outline: &mut FileOutline) {
    if line.contains("func.func") || line.starts_with("func ") {
        if let Some(name) = after_at_name(line) {
            push(outline, name, SymbolKind::Function, line_num, None);
        }
    } else if let Some(name) = after_keyword(line, "module") {
        push(outline, name, SymbolKind::Module, line_num, None);
    }
}

fn parse_tablegen(line: &str, line_num: u32, outline: &mut FileOutline) {
    if let Some(import) = quoted_import(line, &["include"]) {
        outline.imports.push(import.clone());
        push(outline, import, SymbolKind::Import, line_num, None);
    } else if let Some(name) = after_any_keyword(line, &["class", "multiclass"]) {
        push(outline, name, SymbolKind::ClassDef, line_num, None);
    } else if let Some(name) = after_any_keyword(line, &["def", "defm"]) {
        push(outline, name, SymbolKind::StructDef, line_num, None);
    } else if let Some(rest) = line.strip_prefix("let ") {
        if let Some((name, _)) = rest.split_once('=') {
            push(
                outline,
                clean_ident(name),
                SymbolKind::Constant,
                line_num,
                None,
            );
        }
    }
}

fn push(
    outline: &mut FileOutline,
    name: String,
    kind: SymbolKind,
    line: u32,
    detail: Option<String>,
) {
    if name.is_empty() {
        return;
    }
    outline.symbols.push(Symbol {
        name,
        kind,
        line_start: line,
        line_end: line,
        detail,
    });
}

fn starts_comment(line: &str) -> bool {
    line.starts_with('#') || line.starts_with("//") || line.starts_with("/*")
}

fn first_token(line: &str) -> Option<&str> {
    line.split_whitespace().next()
}

fn clean_ident(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_matches('`')
        .trim_matches('$')
        .trim_matches('@')
        .trim_end_matches('{')
        .trim_end_matches(';')
        .trim_end_matches(',')
        .trim_end_matches(':')
        .split(['(', '<', ':', '=', ' ', '\t'])
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

fn clean_sql_ident(raw: &str) -> String {
    clean_ident(raw)
        .trim_matches('[')
        .trim_matches(']')
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_string()
}

fn after_keyword(line: &str, keyword: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let keyword_tokens: Vec<&str> = keyword.split_whitespace().collect();
    if keyword_tokens.is_empty() || tokens.len() <= keyword_tokens.len() {
        return None;
    }

    for start in 0..=tokens.len() - keyword_tokens.len() - 1 {
        if tokens[start..start + keyword_tokens.len()] == keyword_tokens {
            let name = clean_ident(tokens[start + keyword_tokens.len()]);
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn after_any_keyword(line: &str, keywords: &[&str]) -> Option<String> {
    keywords
        .iter()
        .find_map(|keyword| after_keyword(line, keyword))
}

fn quoted_strings(rest: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut input = rest;
    while let Some(start) = input.find('"') {
        let after = &input[start + 1..];
        let Some(end) = after.find('"') else {
            break;
        };
        values.push(after[..end].to_string());
        input = &after[end + 1..];
    }
    values
}

fn json_key(line: &str) -> Option<String> {
    if !line.starts_with('"') {
        return None;
    }
    let end = line[1..].find('"')?;
    let rest = line[1 + end + 1..].trim_start();
    rest.starts_with(':').then(|| line[1..1 + end].to_string())
}

fn json_string_value(line: &str) -> Option<String> {
    let line = line.trim().trim_end_matches(',');
    if !line.starts_with('"') || line.contains(':') {
        return None;
    }
    let end = line[1..].find('"')?;
    Some(line[1..1 + end].to_string())
}

fn toml_section(line: &str) -> Option<String> {
    let line = line
        .strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]"))
        .or_else(|| {
            line.strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
        })?;
    Some(line.trim().to_string())
}

fn toml_string_value(line: &str) -> Option<String> {
    let (_, value) = line.split_once('=')?;
    quoted_strings(value).into_iter().next()
}

fn quoted_import(line: &str, keywords: &[&str]) -> Option<String> {
    if !keywords
        .iter()
        .any(|keyword| line.trim_start().starts_with(keyword))
    {
        return None;
    }
    quoted_strings(line)
        .into_iter()
        .next()
        .or_else(|| single_quoted_strings(line).into_iter().next())
}

fn call_arg(line: &str, call: &str) -> Option<String> {
    let rest = line.strip_prefix(call)?.trim_start();
    let rest = rest.strip_prefix('(')?;
    Some(clean_ident(rest))
}

fn quoted_call_arg(line: &str, call: &str) -> Option<String> {
    line.strip_prefix(call)
        .and_then(|rest| quoted_strings(rest).into_iter().next())
}

fn function_name_before_paren(line: &str) -> Option<String> {
    let before = line.split_once('(')?.0.trim();
    if before.is_empty() || before.contains(['=', ':']) {
        return None;
    }
    let name = before.split_whitespace().last().map(clean_ident)?;
    let blocked = ["if", "for", "while", "switch", "catch", "return"];
    (!name.is_empty() && !blocked.contains(&name.as_str())).then_some(name)
}

fn after_at_name(line: &str) -> Option<String> {
    let start = line.find('@')? + 1;
    let rest = &line[start..];
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.' || *c == '$')
        .collect();
    (!name.is_empty()).then_some(name)
}

fn single_quoted_strings(rest: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut input = rest;
    while let Some(start) = input.find('\'') {
        let after = &input[start + 1..];
        let Some(end) = after.find('\'') else {
            break;
        };
        values.push(after[..end].to_string());
        input = &after[end + 1..];
    }
    values
}

fn export_binding_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("export ")?;
    after_any_keyword(rest, &["let", "const", "var"])
}

fn css_selector_name(line: &str) -> Option<String> {
    let selector = line.split_once('{')?.0.trim();
    if selector.is_empty() || selector.starts_with('@') {
        return None;
    }
    Some(selector.to_string())
}

fn directive_name(line: &str, directive: &str) -> Option<String> {
    let rest = line.strip_prefix(directive)?.trim_start();
    let name = clean_ident(rest);
    (!name.is_empty()).then_some(name)
}

fn fortran_type_name(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if !lower.starts_with("type") {
        return None;
    }
    let (_, rhs) = line.split_once("::")?;
    let name = clean_ident(rhs);
    (!name.is_empty()).then_some(name)
}

fn infer_symbol_ranges(outline: &mut FileOutline) {
    let mut block_symbols: Vec<usize> = outline
        .symbols
        .iter()
        .enumerate()
        .filter_map(|(idx, sym)| is_block_symbol(sym.kind).then_some(idx))
        .collect();
    block_symbols.sort_by_key(|idx| outline.symbols[*idx].line_start);

    for window in block_symbols.windows(2) {
        let current = window[0];
        let next = window[1];
        outline.symbols[current].line_end = outline.symbols[next].line_start.saturating_sub(1);
    }
    if let Some(last) = block_symbols.last().copied() {
        outline.symbols[last].line_end = outline.line_count;
    }
}

fn is_block_symbol(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function
            | SymbolKind::StructDef
            | SymbolKind::EnumDef
            | SymbolKind::UnionDef
            | SymbolKind::ClassDef
            | SymbolKind::InterfaceDef
            | SymbolKind::Module
            | SymbolKind::Method
            | SymbolKind::TraitDef
            | SymbolKind::ImplBlock
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(outline: &FileOutline, kind: SymbolKind) -> Vec<String> {
        outline
            .symbols
            .iter()
            .filter(|sym| sym.kind == kind)
            .map(|sym| sym.name.clone())
            .collect()
    }

    #[test]
    fn hcl_uses_block_specific_names_and_kinds() {
        let outline = parse_lightweight(
            "main.tf",
            r#"
resource "aws_instance" "web" {}
variable "region" {}
output "ip" {}
module "vpc" {}
"#,
            Language::Hcl,
        );

        assert!(names(&outline, SymbolKind::StructDef).contains(&"web".to_string()));
        assert!(names(&outline, SymbolKind::Variable).contains(&"region".to_string()));
        assert!(names(&outline, SymbolKind::Constant).contains(&"ip".to_string()));
        assert!(names(&outline, SymbolKind::Module).contains(&"vpc".to_string()));
    }

    #[test]
    fn kotlin_distinguishes_interfaces_enums_and_constants() {
        let outline = parse_lightweight(
            "App.kt",
            r#"
package demo
import kotlinx.coroutines.runBlocking
data class User(val name: String)
interface Repo
enum class KotlinMode { A }
private fun loadUser(): User = User("a")
val answer = 42
"#,
            Language::Kotlin,
        );

        assert!(outline
            .imports
            .contains(&"kotlinx.coroutines.runBlocking".to_string()));
        assert!(names(&outline, SymbolKind::ClassDef).contains(&"User".to_string()));
        assert!(names(&outline, SymbolKind::InterfaceDef).contains(&"Repo".to_string()));
        assert!(names(&outline, SymbolKind::EnumDef).contains(&"KotlinMode".to_string()));
        assert!(names(&outline, SymbolKind::Function).contains(&"loadUser".to_string()));
        assert!(names(&outline, SymbolKind::Constant).contains(&"answer".to_string()));
    }

    #[test]
    fn web_component_extracts_script_and_style_symbols() {
        let outline = parse_lightweight(
            "Widget.svelte",
            r#"
<script>
import Thing from './Thing.svelte';
export let title;
function renderTitle() {}
</script>
.card { color: red; }
"#,
            Language::Svelte,
        );

        assert!(outline.imports.contains(&"./Thing.svelte".to_string()));
        assert!(names(&outline, SymbolKind::Constant).contains(&"title".to_string()));
        assert!(names(&outline, SymbolKind::Function).contains(&"renderTitle".to_string()));
        assert!(names(&outline, SymbolKind::ClassDef).contains(&".card".to_string()));
    }

    #[test]
    fn package_json_outlines_manifest_sections() {
        let outline = parse_lightweight(
            "package.json",
            r#"
{
  "scripts": {
    "test": "vitest"
  },
  "dependencies": {
    "react": "^19.0.0"
  },
  "devDependencies": {
    "vite": "^6.0.0"
  },
  "workspaces": [
    "packages/*"
  ]
}
"#,
            Language::Json,
        );

        assert!(names(&outline, SymbolKind::Module).contains(&"scripts".to_string()));
        assert!(names(&outline, SymbolKind::Constant).contains(&"test".to_string()));
        assert!(names(&outline, SymbolKind::Constant).contains(&"react".to_string()));
        assert!(names(&outline, SymbolKind::Constant).contains(&"vite".to_string()));
        assert!(names(&outline, SymbolKind::Module).contains(&"packages/*".to_string()));
        let react = outline
            .symbols
            .iter()
            .find(|symbol| symbol.name == "react")
            .unwrap();
        assert_eq!(react.detail.as_deref(), Some("dependency"));
    }

    #[test]
    fn cargo_toml_outlines_manifest_sections() {
        let outline = parse_lightweight(
            "Cargo.toml",
            r#"
[package]
name = "lexa"
version = "0.1.0"

[dependencies]
serde = "1"

[features]
default = []

[workspace]
members = ["crates/*"]

[[bin]]
name = "lexa"
"#,
            Language::Toml,
        );

        assert!(names(&outline, SymbolKind::Module).contains(&"package".to_string()));
        assert!(names(&outline, SymbolKind::Constant).contains(&"name".to_string()));
        assert!(names(&outline, SymbolKind::Import).contains(&"serde".to_string()));
        assert!(names(&outline, SymbolKind::Constant).contains(&"default".to_string()));
        assert!(names(&outline, SymbolKind::Module).contains(&"crates/*".to_string()));
        assert!(names(&outline, SymbolKind::Module).contains(&"lexa".to_string()));
    }

    #[test]
    fn scss_sql_fortran_and_tablegen_common_constructs() {
        let scss = parse_lightweight(
            "app.scss",
            r#"
@use "theme";
$gap: 8px;
@mixin center {}
@function scale($x) {}
.panel {}
"#,
            Language::Scss,
        );
        assert!(scss.imports.contains(&"theme".to_string()));
        assert!(names(&scss, SymbolKind::Variable).contains(&"gap".to_string()));
        assert!(names(&scss, SymbolKind::Function).contains(&"center".to_string()));
        assert!(names(&scss, SymbolKind::Function).contains(&"scale".to_string()));
        assert!(names(&scss, SymbolKind::ClassDef).contains(&".panel".to_string()));

        let sql = parse_lightweight(
            "schema.sql",
            r#"
CREATE TABLE users (id integer);
CREATE OR REPLACE FUNCTION do_thing() RETURNS void AS $$ SELECT 1; $$ LANGUAGE sql;
CREATE INDEX idx_users_id ON users(id);
"#,
            Language::Sql,
        );
        assert!(names(&sql, SymbolKind::StructDef).contains(&"users".to_string()));
        assert!(names(&sql, SymbolKind::Function).contains(&"do_thing".to_string()));
        assert!(names(&sql, SymbolKind::Constant).contains(&"idx_users_id".to_string()));

        let fortran = parse_lightweight(
            "solver.f90",
            r#"
module solver
use mathlib
type :: Particle
subroutine step()
function energy()
"#,
            Language::Fortran,
        );
        assert!(fortran.imports.contains(&"mathlib".to_string()));
        assert!(names(&fortran, SymbolKind::Module).contains(&"solver".to_string()));
        assert!(names(&fortran, SymbolKind::StructDef).contains(&"Particle".to_string()));
        assert!(names(&fortran, SymbolKind::Function).contains(&"step".to_string()));
        assert!(names(&fortran, SymbolKind::Function).contains(&"energy".to_string()));

        let tablegen = parse_lightweight(
            "records.td",
            r#"
include "Base.td"
class Register<string name>;
multiclass Pat<string op>;
def R0 : Register<"r0">;
defm ADD : Pat<"add">;
let Namespace = "Toy";
"#,
            Language::Tablegen,
        );
        assert!(tablegen.imports.contains(&"Base.td".to_string()));
        assert!(names(&tablegen, SymbolKind::ClassDef).contains(&"Register".to_string()));
        assert!(names(&tablegen, SymbolKind::ClassDef).contains(&"Pat".to_string()));
        assert!(names(&tablegen, SymbolKind::StructDef).contains(&"R0".to_string()));
        assert!(names(&tablegen, SymbolKind::StructDef).contains(&"ADD".to_string()));
        assert!(names(&tablegen, SymbolKind::Constant).contains(&"Namespace".to_string()));
    }
}
