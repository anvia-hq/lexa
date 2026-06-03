use anyhow::{bail, Context, Result};
use hashbrown::HashSet;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::report::AuditSeverity;

pub(crate) const DEFAULT_MAX_FINDINGS: usize = 100;
const LARGE_FILE_WARNING_LINES: u32 = 800;
const LARGE_FILE_HIGH_LINES: u32 = 1500;
const LARGE_SYMBOL_WARNING_LINES: u32 = 120;
const LARGE_SYMBOL_HIGH_LINES: u32 = 250;
const HOTSPOT_FAN_IN_WARNING: usize = 15;
const HOTSPOT_FAN_IN_HIGH: usize = 40;
const HOTSPOT_FAN_OUT_WARNING: usize = 20;
const HOTSPOT_FAN_OUT_HIGH: usize = 50;
const DEFAULT_DEAD_CODE_IGNORE_SYMBOLS: &[&str] = &[
    "main", "new", "default", "test", "setup", "handler", "render", "init",
];
const DEFAULT_DEAD_CODE_ENTRYPOINT_GLOBS: &[&str] = &[
    "src/main.*",
    "src/bin/**",
    "src/lib.*",
    "pages/**",
    "app/**",
    "tests/**",
    "benches/**",
    "examples/**",
];
pub(crate) const DEFAULT_GENERATED_IGNORE_GLOBS: &[&str] = &[
    "**/*.gen.*",
    "**/*.generated.*",
    "**/*.g.*",
    "**/*_generated.*",
    "**/*Generated.*",
    "**/generated.*",
    "**/generated/**",
    "**/gen/**",
    "**/autogen/**",
    "**/auto_generated/**",
    "**/generated_src/**",
    "**/generated-sources/**",
    "**/generated_sources/**",
    "**/__generated__/**",
    "**/.generated/**",
    "**/*.pb.*",
    "**/*.grpc.pb.*",
    "**/*_pb2.py",
    "**/*_pb2_grpc.py",
    "**/*.pbenum.*",
    "**/*.pbobjc.*",
    "**/*.pbjson.*",
    "**/*.pbserver.*",
    "**/*.pbgrpc.*",
    "**/*.freezed.dart",
    "**/*.gr.dart",
    "**/*.designer.*",
    "**/*.Designer.*",
    "**/TemporaryGeneratedFile_*.cs",
    "**/zz_generated.*",
    "**/R.java",
    "**/R.kt",
    "**/BuildConfig.java",
    "**/BuildConfig.kt",
    "**/moc_*.cpp",
    "**/ui_*.h",
    "**/qrc_*.cpp",
    "**/GeneratedPluginRegistrant.*",
    "**/*openapi*generated*.*",
    "**/*swagger*generated*.*",
    "**/openapi/generated/**",
    "**/graphql/generated/**",
    "**/routeTree.gen.ts",
    "**/worker-configuration.d.ts",
    "**/drizzle/meta/**",
    "**/package-lock.json",
    "**/pnpm-lock.yaml",
    "**/yarn.lock",
    "**/Cargo.lock",
    "**/go.sum",
    "**/poetry.lock",
    "**/Pipfile.lock",
    "**/Gemfile.lock",
    "**/composer.lock",
    "**/node_modules/**",
    "**/dist/**",
    "**/build/**",
    "**/out/**",
    "**/.next/**",
    "**/.nuxt/**",
    "**/.svelte-kit/**",
    "**/.angular/**",
    "**/.turbo/**",
    "**/.expo/**",
    "**/coverage/**",
    "**/target/**",
    "**/vendor/**",
];

#[derive(Debug, Clone)]
pub struct AuditConfig {
    pub max_findings: usize,
    pub thresholds: AuditThresholds,
    pub rules: AuditRules,
    pub ignore: AuditIgnore,
    pub dead_code: DeadCodeConfig,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            max_findings: DEFAULT_MAX_FINDINGS,
            thresholds: AuditThresholds::default(),
            rules: AuditRules::default(),
            ignore: AuditIgnore::default(),
            dead_code: DeadCodeConfig::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditThresholds {
    pub large_file_warning: u32,
    pub large_file_high: u32,
    pub large_symbol_warning: u32,
    pub large_symbol_high: u32,
    pub fan_in_warning: usize,
    pub fan_in_high: usize,
    pub fan_out_warning: usize,
    pub fan_out_high: usize,
}

impl Default for AuditThresholds {
    fn default() -> Self {
        Self {
            large_file_warning: LARGE_FILE_WARNING_LINES,
            large_file_high: LARGE_FILE_HIGH_LINES,
            large_symbol_warning: LARGE_SYMBOL_WARNING_LINES,
            large_symbol_high: LARGE_SYMBOL_HIGH_LINES,
            fan_in_warning: HOTSPOT_FAN_IN_WARNING,
            fan_in_high: HOTSPOT_FAN_IN_HIGH,
            fan_out_warning: HOTSPOT_FAN_OUT_WARNING,
            fan_out_high: HOTSPOT_FAN_OUT_HIGH,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditRules {
    pub architecture_cycle: RuleSetting,
    pub file_large: RuleSetting,
    pub symbol_large: RuleSetting,
    pub dependency_hotspot: RuleSetting,
    pub dependency_unresolved_import: RuleSetting,
    pub dead_code_candidate: RuleSetting,
}

impl Default for AuditRules {
    fn default() -> Self {
        Self {
            architecture_cycle: RuleSetting::High,
            file_large: RuleSetting::Warning,
            symbol_large: RuleSetting::Warning,
            dependency_hotspot: RuleSetting::Warning,
            dependency_unresolved_import: RuleSetting::High,
            dead_code_candidate: RuleSetting::Off,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditIgnore {
    pub generated: bool,
    pub paths: Vec<String>,
    pub findings: HashSet<String>,
}

impl Default for AuditIgnore {
    fn default() -> Self {
        Self {
            generated: true,
            paths: Vec::new(),
            findings: HashSet::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DeadCodeConfig {
    pub ignore_symbols: HashSet<String>,
    pub entrypoint_globs: Vec<String>,
}

impl Default for DeadCodeConfig {
    fn default() -> Self {
        Self {
            ignore_symbols: DEFAULT_DEAD_CODE_IGNORE_SYMBOLS
                .iter()
                .map(|symbol| (*symbol).to_string())
                .collect(),
            entrypoint_globs: DEFAULT_DEAD_CODE_ENTRYPOINT_GLOBS
                .iter()
                .map(|glob| (*glob).to_string())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSetting {
    Off,
    Warning,
    High,
}

impl RuleSetting {
    pub(crate) fn finding_severity(self, base: AuditSeverity) -> Option<AuditSeverity> {
        match self {
            Self::Off => None,
            Self::Warning => Some(base),
            Self::High => Some(AuditSeverity::High),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuditConfigFile {
    #[serde(default)]
    audit: AuditConfigSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditConfigSection {
    max_findings: Option<usize>,
    #[serde(default)]
    thresholds: AuditThresholdSection,
    #[serde(default)]
    rules: HashMap<String, RuleSetting>,
    #[serde(default)]
    ignore: AuditIgnoreSection,
    #[serde(default)]
    dead_code: DeadCodeSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditThresholdSection {
    large_file_warning: Option<u32>,
    large_file_high: Option<u32>,
    large_symbol_warning: Option<u32>,
    large_symbol_high: Option<u32>,
    fan_in_warning: Option<usize>,
    fan_in_high: Option<usize>,
    fan_out_warning: Option<usize>,
    fan_out_high: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuditIgnoreSection {
    generated: Option<bool>,
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    findings: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeadCodeSection {
    #[serde(default)]
    ignore_symbols: Vec<String>,
    #[serde(default)]
    entrypoint_globs: Vec<String>,
}

pub fn load_audit_config(
    root: &Path,
    explicit_path: Option<&Path>,
    no_config: bool,
) -> Result<AuditConfig> {
    if no_config {
        return Ok(AuditConfig::default());
    }

    let Some(path) = find_audit_config_path(root, explicit_path) else {
        return Ok(AuditConfig::default());
    };

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read audit config {}", path.display()))?;
    let file = toml::from_str::<AuditConfigFile>(&content)
        .with_context(|| format!("failed to parse audit config {}", path.display()))?;
    AuditConfig::from_file(file)
}

fn find_audit_config_path(root: &Path, explicit_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit_path {
        return Some(if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        });
    }

    let candidates = [root.join("lexa.toml"), root.join(".lexa/audit.toml")];
    candidates.into_iter().find(|path| path.exists())
}

impl AuditConfig {
    pub(crate) fn from_file(file: AuditConfigFile) -> Result<Self> {
        let mut config = Self::default();
        let audit = file.audit;

        if let Some(max_findings) = audit.max_findings {
            config.max_findings = max_findings;
        }

        config.thresholds.apply(audit.thresholds)?;
        config.rules.apply(audit.rules)?;
        if let Some(generated) = audit.ignore.generated {
            config.ignore.generated = generated;
        }
        config.ignore.paths = audit.ignore.paths;
        config.ignore.findings = audit.ignore.findings.into_iter().collect();
        config.dead_code.apply(audit.dead_code);

        Ok(config)
    }
}

impl DeadCodeConfig {
    fn apply(&mut self, section: DeadCodeSection) {
        self.ignore_symbols.extend(section.ignore_symbols);
        self.entrypoint_globs.extend(section.entrypoint_globs);
    }
}

impl AuditThresholds {
    fn apply(&mut self, section: AuditThresholdSection) -> Result<()> {
        if let Some(value) = section.large_file_warning {
            self.large_file_warning = value;
        }
        if let Some(value) = section.large_file_high {
            self.large_file_high = value;
        }
        if let Some(value) = section.large_symbol_warning {
            self.large_symbol_warning = value;
        }
        if let Some(value) = section.large_symbol_high {
            self.large_symbol_high = value;
        }
        if let Some(value) = section.fan_in_warning {
            self.fan_in_warning = value;
        }
        if let Some(value) = section.fan_in_high {
            self.fan_in_high = value;
        }
        if let Some(value) = section.fan_out_warning {
            self.fan_out_warning = value;
        }
        if let Some(value) = section.fan_out_high {
            self.fan_out_high = value;
        }
        self.validate()
    }

    fn validate(&self) -> Result<()> {
        if self.large_file_warning > self.large_file_high {
            bail!("large_file_warning must be <= large_file_high");
        }
        if self.large_symbol_warning > self.large_symbol_high {
            bail!("large_symbol_warning must be <= large_symbol_high");
        }
        if self.fan_in_warning > self.fan_in_high {
            bail!("fan_in_warning must be <= fan_in_high");
        }
        if self.fan_out_warning > self.fan_out_high {
            bail!("fan_out_warning must be <= fan_out_high");
        }
        Ok(())
    }
}

impl AuditRules {
    fn apply(&mut self, rules: HashMap<String, RuleSetting>) -> Result<()> {
        for (rule, setting) in rules {
            match rule.as_str() {
                "architecture.cycle" => self.architecture_cycle = setting,
                "file.large" => self.file_large = setting,
                "symbol.large" => self.symbol_large = setting,
                "dependency.hotspot" => self.dependency_hotspot = setting,
                "dependency.unresolved_import" => self.dependency_unresolved_import = setting,
                "dead_code.candidate" => self.dead_code_candidate = setting,
                _ => bail!("unknown audit rule '{rule}'"),
            }
        }
        Ok(())
    }
}
