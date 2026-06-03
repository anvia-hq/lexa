use serde::Serialize;
use serde_json::Value;

use super::scope::AuditScopeReport;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditVerdict {
    Pass,
    Warn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSeverity {
    Warning,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditActionability {
    Actionable,
    Candidate,
    Expected,
    RiskNote,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditNextStep {
    pub tool: String,
    pub args: Value,
}

impl AuditNextStep {
    pub(crate) fn new(tool: &str, args: Value) -> Self {
        Self {
            tool: tool.to_string(),
            args,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditSummary {
    pub total_findings: usize,
    pub returned_findings: usize,
    pub high: usize,
    pub warnings: usize,
    pub actionable: usize,
    pub candidates: usize,
    pub risk_notes: usize,
    pub expected: usize,
    pub secondary: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditFinding {
    pub id: String,
    pub rule: String,
    pub severity: AuditSeverity,
    pub actionability: AuditActionability,
    pub secondary: bool,
    pub title: String,
    pub path: String,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub message: String,
    pub evidence: Vec<String>,
    pub related_paths: Vec<String>,
    pub suggestion: String,
    pub next_steps: Vec<AuditNextStep>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditReport {
    pub scope: AuditScopeReport,
    pub verdict: AuditVerdict,
    pub verification_note: String,
    pub summary: AuditSummary,
    pub groups: AuditGroups,
    pub findings: Vec<AuditFinding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditGroups {
    pub primary: Vec<AuditFinding>,
    pub secondary: Vec<AuditFinding>,
    pub actionable: Vec<AuditFinding>,
    pub candidates: Vec<AuditFinding>,
    pub risk_notes: Vec<AuditFinding>,
    pub expected: Vec<AuditFinding>,
}

impl AuditGroups {
    pub(crate) fn from_findings(findings: &[AuditFinding]) -> Self {
        Self {
            primary: findings
                .iter()
                .filter(|finding| !finding.secondary)
                .cloned()
                .collect(),
            secondary: findings
                .iter()
                .filter(|finding| finding.secondary)
                .cloned()
                .collect(),
            actionable: findings
                .iter()
                .filter(|finding| {
                    finding.actionability == AuditActionability::Actionable && !finding.secondary
                })
                .cloned()
                .collect(),
            candidates: findings
                .iter()
                .filter(|finding| {
                    finding.actionability == AuditActionability::Candidate && !finding.secondary
                })
                .cloned()
                .collect(),
            risk_notes: findings
                .iter()
                .filter(|finding| {
                    finding.actionability == AuditActionability::RiskNote && !finding.secondary
                })
                .cloned()
                .collect(),
            expected: findings
                .iter()
                .filter(|finding| {
                    finding.actionability == AuditActionability::Expected && !finding.secondary
                })
                .cloned()
                .collect(),
        }
    }
}

pub fn render_audit_report(report: &AuditReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "audit verdict: {:?}\nfindings: {} total, {} high, {} warning(s)",
        report.verdict, report.summary.total_findings, report.summary.high, report.summary.warnings
    ));
    if report.summary.truncated {
        out.push_str(&format!(" (showing {})", report.summary.returned_findings));
    }
    out.push('\n');
    out.push_str(&format!("verification: {}\n", report.verification_note));

    if let AuditScopeReport::GitSince {
        base,
        changed_files,
    } = &report.scope
    {
        out.push_str(&format!(
            "scope: git since {base}, {} changed file(s)\n",
            changed_files.len()
        ));
    }

    if report.findings.is_empty() {
        out.push_str("No audit findings.\n");
        return out;
    }

    render_group(&mut out, "Actionable Findings", report, |finding| {
        finding.actionability == AuditActionability::Actionable && !finding.secondary
    });
    render_group(&mut out, "Dead-Code Candidates", report, |finding| {
        finding.actionability == AuditActionability::Candidate && !finding.secondary
    });
    render_group(&mut out, "Risk Notes", report, |finding| {
        finding.actionability == AuditActionability::RiskNote && !finding.secondary
    });
    render_group(&mut out, "Expected Hotspots", report, |finding| {
        finding.actionability == AuditActionability::Expected && !finding.secondary
    });
    render_group(&mut out, "Secondary Context", report, |finding| {
        finding.secondary
    });

    out
}

fn render_group(
    out: &mut String,
    title: &str,
    report: &AuditReport,
    predicate: impl Fn(&AuditFinding) -> bool,
) {
    let findings = report
        .findings
        .iter()
        .filter(|finding| predicate(finding))
        .collect::<Vec<_>>();
    if findings.is_empty() {
        return;
    }

    out.push_str(&format!("\n{title}\n"));
    for finding in findings {
        render_finding(out, finding);
    }
}

fn render_finding(out: &mut String, finding: &AuditFinding) {
    let location = match (finding.line_start, finding.line_end) {
        (Some(start), Some(end)) if start != end => {
            format!("{}:{}-{}", finding.path, start, end)
        }
        (Some(line), _) => format!("{}:{}", finding.path, line),
        _ => finding.path.clone(),
    };
    out.push_str(&format!(
        "\n[{:?}] {} at {}\n{}\nActionability: {:?}{}\nSuggestion: {}\n",
        finding.severity,
        finding.rule,
        location,
        finding.title,
        finding.actionability,
        if finding.secondary {
            " (secondary)"
        } else {
            ""
        },
        finding.suggestion
    ));
    if !finding.evidence.is_empty() {
        out.push_str("Evidence:\n");
        for item in &finding.evidence {
            out.push_str(&format!("  - {item}\n"));
        }
    }
    if !finding.next_steps.is_empty() {
        out.push_str("Next steps:\n");
        for step in &finding.next_steps {
            out.push_str(&format!("  - {} {}\n", step.tool, step.args));
        }
    }
}
