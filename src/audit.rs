pub(crate) mod config;
mod report;
mod rules;
mod scope;

pub use config::{
    load_audit_config, AuditConfig, AuditIgnore, AuditRules, AuditThresholds, DeadCodeConfig,
    RuleSetting,
};
pub use report::{
    render_audit_report, AuditActionability, AuditFinding, AuditGroups, AuditNextStep, AuditReport,
    AuditSeverity, AuditSummary, AuditVerdict,
};
pub use scope::{changed_files_since, AuditScope, AuditScopeReport};

use crate::engine::Engine;
use hashbrown::HashMap;

use rules::{collect_findings, filter_findings_by_scope, filter_ignored_findings};

#[derive(Debug, Clone)]
pub struct AuditOptions {
    pub max_results: Option<usize>,
    pub scope: AuditScope,
    pub config: AuditConfig,
    pub includes: AuditIncludes,
}

impl Default for AuditOptions {
    fn default() -> Self {
        Self {
            max_results: None,
            scope: AuditScope::Project,
            config: AuditConfig::default(),
            includes: AuditIncludes::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AuditIncludes {
    pub dead_code: bool,
}

pub fn run_audit(engine: &Engine, options: AuditOptions) -> AuditReport {
    let max_results = options.max_results.unwrap_or(options.config.max_findings);

    let mut findings = collect_findings(engine, &options.config, options.includes);
    findings = filter_findings_by_scope(engine, findings, &options.scope);
    findings = filter_ignored_findings(findings, &options.config.ignore);
    mark_secondary_findings(&mut findings);

    findings.sort_by(|a, b| {
        a.secondary
            .cmp(&b.secondary)
            .then_with(|| {
                actionability_rank(a.actionability).cmp(&actionability_rank(b.actionability))
            })
            .then_with(|| {
                b.severity
                    .cmp(&a.severity)
                    .then_with(|| a.path.cmp(&b.path))
                    .then_with(|| a.line_start.cmp(&b.line_start))
                    .then_with(|| a.rule.cmp(&b.rule))
                    .then_with(|| a.id.cmp(&b.id))
            })
    });

    let total_findings = findings.len();
    let high = findings
        .iter()
        .filter(|finding| finding.severity == AuditSeverity::High)
        .count();
    let warnings = total_findings.saturating_sub(high);
    let truncated = total_findings > max_results;
    findings.truncate(max_results);
    let groups = AuditGroups::from_findings(&findings);

    AuditReport {
        scope: options.scope.report(),
        verdict: if total_findings == 0 {
            AuditVerdict::Pass
        } else {
            AuditVerdict::Warn
        },
        summary: AuditSummary {
            total_findings,
            returned_findings: findings.len(),
            high,
            warnings,
            actionable: groups.actionable.len(),
            candidates: groups.candidates.len(),
            risk_notes: groups.risk_notes.len(),
            expected: groups.expected.len(),
            secondary: groups.secondary.len(),
            truncated,
        },
        groups,
        findings,
    }
}

fn mark_secondary_findings(findings: &mut [AuditFinding]) {
    let mut strongest_by_path: HashMap<String, AuditActionability> = HashMap::new();
    for finding in findings.iter() {
        strongest_by_path
            .entry(finding.path.clone())
            .and_modify(|actionability| {
                if actionability_rank(finding.actionability) < actionability_rank(*actionability) {
                    *actionability = finding.actionability;
                }
            })
            .or_insert(finding.actionability);
    }

    for finding in findings {
        if strongest_by_path
            .get(finding.path.as_str())
            .is_some_and(|strongest| {
                actionability_rank(*strongest) < actionability_rank(finding.actionability)
                    && matches!(*strongest, AuditActionability::Actionable)
            })
        {
            finding.secondary = true;
        }
    }
}

fn actionability_rank(actionability: AuditActionability) -> u8 {
    match actionability {
        AuditActionability::Actionable => 0,
        AuditActionability::Candidate => 1,
        AuditActionability::RiskNote => 2,
        AuditActionability::Expected => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::config::AuditConfigFile;
    use crate::audit::scope::normalize_git_changed_path;
    use crate::engine::Engine;

    const LARGE_FILE_WARNING_LINES: u32 = 800;
    const LARGE_SYMBOL_WARNING_LINES: u32 = 120;
    const HOTSPOT_FAN_IN_WARNING: usize = 15;
    const HOTSPOT_FAN_IN_HIGH: usize = 40;
    const HOTSPOT_FAN_OUT_WARNING: usize = 20;

    #[test]
    fn audit_detects_import_cycles() {
        let mut engine = Engine::new(4);
        engine.index_file("src/a.rs", "use crate::b;\nfn a() {}\n");
        engine.index_file("src/b.rs", "use crate::a;\nfn b() {}\n");

        let report = run_audit(&engine, AuditOptions::default());

        assert_eq!(report.verdict, AuditVerdict::Warn);
        assert!(report
            .findings
            .iter()
            .any(|finding| finding.rule == "architecture.cycle"));
    }

    #[test]
    fn normalize_git_changed_path_strips_worktree_prefix() {
        assert_eq!(
            normalize_git_changed_path("crates/app/src/main.rs", "crates/app/"),
            Some("src/main.rs".to_string())
        );
        assert_eq!(
            normalize_git_changed_path("crates/core/src/lib.rs", "crates/app/"),
            None
        );
    }

    #[test]
    fn audit_git_scope_keeps_findings_for_changed_paths() {
        let mut engine = Engine::new(4);
        let content = "line\n".repeat(LARGE_FILE_WARNING_LINES as usize);
        engine.index_file("src/large.rs", &content);
        engine.index_file("src/other.rs", &content);

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: Some(100),
                scope: AuditScope::GitSince {
                    base: "main".to_string(),
                    changed_files: vec!["src/large.rs".to_string()],
                },
                config: AuditConfig::default(),
                includes: AuditIncludes::default(),
            },
        );

        assert_eq!(report.summary.total_findings, 1);
        assert_eq!(report.findings[0].path, "src/large.rs");
    }

    #[test]
    fn audit_git_scope_keeps_direct_dependency_context() {
        let mut engine = Engine::new(4);
        engine.index_file("src/core.rs", "pub fn core() {}\n");
        for index in 0..HOTSPOT_FAN_IN_WARNING {
            engine.index_file(
                &format!("src/user_{index}.rs"),
                "use crate::core;\nfn user() { core::core(); }\n",
            );
        }

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: Some(100),
                scope: AuditScope::GitSince {
                    base: "main".to_string(),
                    changed_files: vec!["src/user_0.rs".to_string()],
                },
                config: AuditConfig::default(),
                includes: AuditIncludes::default(),
            },
        );

        assert!(report.findings.iter().any(|finding| {
            finding.rule == "dependency.hotspot" && finding.path == "src/core.rs"
        }));
    }

    #[test]
    fn audit_flags_large_files_at_threshold() {
        let mut engine = Engine::new(4);
        let content = "line\n".repeat(LARGE_FILE_WARNING_LINES as usize);
        engine.index_file("src/large.rs", &content);

        let report = run_audit(&engine, AuditOptions::default());

        assert!(report.findings.iter().any(|finding| {
            finding.rule == "file.large" && finding.severity == AuditSeverity::Warning
        }));
    }

    #[test]
    fn audit_flags_large_symbols_at_threshold() {
        let mut engine = Engine::new(4);
        let mut content = String::from("fn large() {\n");
        for _ in 0..LARGE_SYMBOL_WARNING_LINES {
            content.push_str("    let value = 1;\n");
        }
        content.push_str("}\n");
        engine.index_file("src/large.rs", &content);

        let report = run_audit(&engine, AuditOptions::default());

        assert!(report.findings.iter().any(|finding| {
            finding.rule == "symbol.large" && finding.severity == AuditSeverity::Warning
        }));
    }

    #[test]
    fn audit_flags_dependency_hotspots_at_threshold() {
        let mut engine = Engine::new(4);
        engine.index_file("src/core.rs", "pub fn core() {}\n");
        for index in 0..HOTSPOT_FAN_IN_WARNING {
            engine.index_file(
                &format!("src/user_{index}.rs"),
                "use crate::core;\nfn user() { core::core(); }\n",
            );
        }

        let report = run_audit(&engine, AuditOptions::default());

        assert!(report.findings.iter().any(|finding| {
            finding.rule == "dependency.hotspot" && finding.path == "src/core.rs"
        }));
    }

    #[test]
    fn audit_classifies_small_shared_utilities_as_expected_hotspots() {
        let mut engine = Engine::new(4);
        engine.index_file("packages/ui/src/lib/utils.ts", "export function cn() {}\n");
        for index in 0..HOTSPOT_FAN_IN_HIGH {
            engine.index_file(
                &format!("packages/ui/src/button_{index}.tsx"),
                "import { cn } from './lib/utils';\nexport function Button() { cn(); }\n",
            );
        }

        let report = run_audit(&engine, AuditOptions::default());
        let finding = report
            .findings
            .iter()
            .find(|finding| {
                finding.rule == "dependency.hotspot"
                    && finding.path == "packages/ui/src/lib/utils.ts"
            })
            .unwrap();

        assert_eq!(finding.severity, AuditSeverity::Warning);
        assert_eq!(finding.actionability, AuditActionability::Expected);
        assert!(finding
            .next_steps
            .iter()
            .any(|step| step.tool == "trace_deps"));
    }

    #[test]
    fn audit_classifies_entrypoint_fanout_as_expected_hotspot() {
        let mut engine = Engine::new(4);
        let mut index = String::new();
        for route in 0..HOTSPOT_FAN_OUT_WARNING {
            index.push_str(&format!("use crate::route_{route};\n"));
            engine.index_file(&format!("src/route_{route}.rs"), "pub fn route() {}\n");
        }
        engine.index_file("src/index.rs", &index);

        let report = run_audit(&engine, AuditOptions::default());
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.rule == "dependency.hotspot" && finding.path == "src/index.rs")
            .unwrap();

        assert_eq!(finding.severity, AuditSeverity::Warning);
        assert_eq!(finding.actionability, AuditActionability::Expected);
        assert!(finding.title.contains("Entrypoint"));
    }

    #[test]
    fn audit_marks_same_file_lower_priority_findings_as_secondary() {
        let mut engine = Engine::new(4);
        let content = "line\n".repeat(LARGE_FILE_WARNING_LINES as usize);
        engine.index_file("src/content.rs", &content);
        for index in 0..HOTSPOT_FAN_IN_WARNING {
            engine.index_file(
                &format!("src/page_{index}.rs"),
                "use crate::content;\nfn page() { content::render(); }\n",
            );
        }

        let report = run_audit(&engine, AuditOptions::default());
        let large = report
            .findings
            .iter()
            .find(|finding| finding.rule == "file.large" && finding.path == "src/content.rs")
            .unwrap();
        let hotspot = report
            .findings
            .iter()
            .find(|finding| {
                finding.rule == "dependency.hotspot" && finding.path == "src/content.rs"
            })
            .unwrap();

        assert!(!large.secondary);
        assert!(hotspot.secondary);
        assert_eq!(report.summary.secondary, 1);
        assert!(report
            .groups
            .actionable
            .iter()
            .any(|finding| finding.path == "src/content.rs"));
        assert!(report
            .groups
            .secondary
            .iter()
            .any(|finding| finding.path == "src/content.rs"));
    }

    #[test]
    fn rendered_audit_groups_findings_by_actionability() {
        let mut engine = Engine::new(4);
        let content = "line\n".repeat(LARGE_FILE_WARNING_LINES as usize);
        engine.index_file("src/content.rs", &content);
        engine.index_file("src/helper.rs", "fn unused_helper() {}\n");
        engine.index_file("packages/ui/src/lib/utils.ts", "export function cn() {}\n");
        for index in 0..HOTSPOT_FAN_IN_WARNING {
            engine.index_file(
                &format!("packages/ui/src/button_{index}.tsx"),
                "import { cn } from './lib/utils';\nexport function Button() { cn(); }\n",
            );
        }

        let report = run_audit(
            &engine,
            AuditOptions {
                includes: AuditIncludes { dead_code: true },
                ..AuditOptions::default()
            },
        );
        let rendered = render_audit_report(&report);

        let actionable = rendered.find("Actionable Findings").unwrap();
        let candidates = rendered.find("Dead-Code Candidates").unwrap();
        let expected = rendered.find("Expected Hotspots").unwrap();
        assert!(actionable < candidates);
        assert!(candidates < expected);
        assert_eq!(report.summary.actionable, report.groups.actionable.len());
        assert_eq!(report.summary.candidates, report.groups.candidates.len());
        assert_eq!(report.summary.expected, report.groups.expected.len());
        assert_eq!(
            report.summary.returned_findings,
            report.groups.primary.len() + report.groups.secondary.len()
        );
    }

    #[test]
    fn audit_truncates_returned_findings() {
        let mut engine = Engine::new(4);
        for index in 0..3 {
            let content = "line\n".repeat(LARGE_FILE_WARNING_LINES as usize);
            engine.index_file(&format!("src/large_{index}.rs"), &content);
        }

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: Some(2),
                scope: AuditScope::Project,
                config: AuditConfig::default(),
                includes: AuditIncludes::default(),
            },
        );

        assert_eq!(report.summary.total_findings, 3);
        assert_eq!(report.summary.returned_findings, 2);
        assert!(report.summary.truncated);
    }

    #[test]
    fn audit_config_thresholds_override_defaults() {
        let mut engine = Engine::new(4);
        engine.index_file("src/small.rs", "one\ntwo\nthree\n");
        let mut config = AuditConfig::default();
        config.thresholds.large_file_warning = 3;
        config.thresholds.large_file_high = 10;

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: Some(100),
                scope: AuditScope::Project,
                config,
                includes: AuditIncludes::default(),
            },
        );

        assert!(report
            .findings
            .iter()
            .any(|finding| finding.rule == "file.large"));
    }

    #[test]
    fn audit_config_can_disable_rules() {
        let mut engine = Engine::new(4);
        let content = "line\n".repeat(LARGE_FILE_WARNING_LINES as usize);
        engine.index_file("src/large.rs", &content);
        let mut config = AuditConfig::default();
        config.rules.file_large = RuleSetting::Off;

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: Some(100),
                scope: AuditScope::Project,
                config,
                includes: AuditIncludes::default(),
            },
        );

        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.rule == "file.large"));
    }

    #[test]
    fn audit_config_ignores_findings_and_paths() {
        let mut engine = Engine::new(4);
        let content = "line\n".repeat(LARGE_FILE_WARNING_LINES as usize);
        engine.index_file("src/large.rs", &content);
        engine.index_file("vendor/large.rs", &content);
        let mut config = AuditConfig::default();
        config
            .ignore
            .findings
            .insert("file.large:src/large.rs".to_string());
        config.ignore.paths.push("vendor/**".to_string());

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: Some(100),
                scope: AuditScope::Project,
                config,
                includes: AuditIncludes::default(),
            },
        );

        assert_eq!(report.summary.total_findings, 0);
    }

    #[test]
    fn audit_ignores_generated_paths_by_default() {
        let mut engine = Engine::new(4);
        let content = "line\n".repeat(20);
        engine.index_file("apps/www/src/routeTree.gen.ts", &content);
        engine.index_file("apps/api/worker-configuration.d.ts", &content);
        engine.index_file("apps/api/drizzle/meta/0000_snapshot.json", &content);
        engine.index_file("services/user.pb.go", &content);
        engine.index_file("python/app/service_pb2.py", &content);
        engine.index_file("android/app/build/generated/source/R.java", &content);
        engine.index_file("lib/models/account.freezed.dart", &content);
        engine.index_file("src/Generated/Order.Designer.cs", &content);
        engine.index_file("cpp/build/moc_window.cpp", &content);
        engine.index_file("Cargo.lock", &content);
        engine.index_file("apps/www/src/static-pages.ts", &content);

        let mut config = AuditConfig::default();
        config.thresholds.large_file_warning = 10;
        config.thresholds.large_file_high = 30;

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: Some(100),
                scope: AuditScope::Project,
                config,
                includes: AuditIncludes::default(),
            },
        );

        assert_eq!(report.summary.total_findings, 1);
        assert_eq!(report.findings[0].path, "apps/www/src/static-pages.ts");
    }

    #[test]
    fn audit_config_can_include_generated_paths() {
        let mut engine = Engine::new(4);
        let content = "line\n".repeat(20);
        engine.index_file("apps/www/src/routeTree.gen.ts", &content);

        let mut config = AuditConfig::default();
        config.ignore.generated = false;
        config.thresholds.large_file_warning = 10;
        config.thresholds.large_file_high = 30;

        let report = run_audit(
            &engine,
            AuditOptions {
                max_results: Some(100),
                scope: AuditScope::Project,
                config,
                includes: AuditIncludes::default(),
            },
        );

        assert!(report.findings.iter().any(|finding| {
            finding.rule == "file.large" && finding.path == "apps/www/src/routeTree.gen.ts"
        }));
    }

    #[test]
    fn audit_ignores_cycles_touching_generated_paths_by_default() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "apps/www/src/routeTree.gen.ts",
            "use crate::routes;\nfn route_tree() {}\n",
        );
        engine.index_file(
            "apps/www/src/routes.ts",
            "use crate::routeTree;\nfn routes() {}\n",
        );

        let report = run_audit(&engine, AuditOptions::default());

        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.rule == "architecture.cycle"));
    }

    #[test]
    fn audit_config_parses_quoted_rule_ids() {
        let parsed = toml::from_str::<AuditConfigFile>(
            r#"
            [audit]
            max_findings = 12

            [audit.thresholds]
            large_file_warning = 10

            [audit.rules]
            "file.large" = "off"
            "dead_code.candidate" = "warning"

            [audit.ignore]
            generated = false
            paths = ["vendor/**"]
            findings = ["dependency.hotspot:src/main.rs"]

            [audit.dead_code]
            ignore_symbols = ["handler"]
            entrypoint_globs = ["routes/**"]
            "#,
        )
        .unwrap();
        let config = AuditConfig::from_file(parsed).unwrap();

        assert_eq!(config.max_findings, 12);
        assert_eq!(config.thresholds.large_file_warning, 10);
        assert_eq!(config.rules.file_large, RuleSetting::Off);
        assert_eq!(config.rules.dead_code_candidate, RuleSetting::Warning);
        assert!(!config.ignore.generated);
        assert_eq!(config.ignore.paths, vec!["vendor/**"]);
        assert!(config
            .ignore
            .findings
            .contains("dependency.hotspot:src/main.rs"));
        assert!(config.dead_code.ignore_symbols.contains("handler"));
        assert!(config
            .dead_code
            .entrypoint_globs
            .contains(&"routes/**".to_string()));
    }

    #[test]
    fn audit_config_rejects_unknown_keys() {
        let parsed = toml::from_str::<AuditConfigFile>(
            r#"
            [audit]
            unexpected = true
            "#,
        );

        assert!(parsed.is_err());
    }

    #[test]
    fn audit_config_rejects_unknown_rules() {
        let parsed = toml::from_str::<AuditConfigFile>(
            r#"
            [audit.rules]
            "unknown.rule" = "warning"
            "#,
        )
        .unwrap();

        assert!(AuditConfig::from_file(parsed).is_err());
    }

    #[test]
    fn dead_code_candidates_are_disabled_by_default() {
        let mut engine = Engine::new(4);
        engine.index_file("src/helper.rs", "fn unused_helper() {}\n");

        let report = run_audit(&engine, AuditOptions::default());

        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.rule == "dead_code.candidate"));
    }

    #[test]
    fn include_dead_code_reports_internal_unreferenced_symbols() {
        let mut engine = Engine::new(4);
        engine.index_file("src/helper.rs", "fn unused_helper() {}\n");

        let report = run_audit(
            &engine,
            AuditOptions {
                includes: AuditIncludes { dead_code: true },
                ..AuditOptions::default()
            },
        );

        let finding = report
            .findings
            .iter()
            .find(|finding| {
                finding.rule == "dead_code.candidate" && finding.path == "src/helper.rs"
            })
            .unwrap();

        assert_eq!(finding.actionability, AuditActionability::Candidate);
        assert!(finding
            .evidence
            .iter()
            .any(|item| item == "confidence: high"));
        assert!(finding.next_steps.iter().any(|step| step.tool == "callers"));
    }

    #[test]
    fn dead_code_candidates_skip_referenced_symbols() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "src/helper.rs",
            "fn used_helper() {}\nfn caller() { used_helper(); }\n",
        );

        let report = run_audit(
            &engine,
            AuditOptions {
                includes: AuditIncludes { dead_code: true },
                ..AuditOptions::default()
            },
        );

        assert!(!report.findings.iter().any(|finding| {
            finding.rule == "dead_code.candidate" && finding.id.contains("used_helper")
        }));
    }

    #[test]
    fn dead_code_candidates_skip_entrypoint_paths() {
        let mut engine = Engine::new(4);
        engine.index_file("src/main.rs", "fn unused_helper() {}\n");

        let report = run_audit(
            &engine,
            AuditOptions {
                includes: AuditIncludes { dead_code: true },
                ..AuditOptions::default()
            },
        );

        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.rule == "dead_code.candidate"));
    }

    #[test]
    fn dead_code_candidates_skip_config_style_and_data_files() {
        let mut engine = Engine::new(4);
        engine.index_file(
            "apps/www/src/styles.css",
            ":root {\n  --background: white;\n}\n#app { min-height: 100%; }\n",
        );
        engine.index_file(
            "biome.json",
            "{\n  \"$schema\": \"https://biomejs.dev/schemas/schema.json\",\n  \"vcs\": { \"enabled\": true }\n}\n",
        );
        engine.index_file(
            "apps/api/package.json",
            "{\n  \"scripts\": { \"db:generate\": \"drizzle-kit generate\" }\n}\n",
        );
        engine.index_file(
            "tsconfig.json",
            "{\n  \"compilerOptions\": { \"paths\": { \"#/*\": [\"./src/*\"] } }\n}\n",
        );

        let report = run_audit(
            &engine,
            AuditOptions {
                includes: AuditIncludes { dead_code: true },
                ..AuditOptions::default()
            },
        );

        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.rule == "dead_code.candidate"));
    }

    #[test]
    fn dead_code_candidates_skip_framework_and_test_files() {
        let mut engine = Engine::new(4);
        engine.index_file("vite.config.ts", "export default { plugins: [] };\n");
        engine.index_file("src/page.test.ts", "function helper() {}\n");
        engine.index_file("src/routes/user.ts", "export function GET() {}\n");

        let report = run_audit(
            &engine,
            AuditOptions {
                includes: AuditIncludes { dead_code: true },
                ..AuditOptions::default()
            },
        );

        assert!(!report
            .findings
            .iter()
            .any(|finding| finding.rule == "dead_code.candidate"));
    }

    #[test]
    fn dead_code_candidates_can_be_enabled_by_config() {
        let mut engine = Engine::new(4);
        engine.index_file("src/helper.rs", "fn unused_helper() {}\n");
        let mut config = AuditConfig::default();
        config.rules.dead_code_candidate = RuleSetting::Warning;

        let report = run_audit(
            &engine,
            AuditOptions {
                config,
                ..AuditOptions::default()
            },
        );

        assert!(report
            .findings
            .iter()
            .any(|finding| finding.rule == "dead_code.candidate"));
    }
}
