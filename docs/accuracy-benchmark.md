# Real-Repository Accuracy Benchmark

Lexa's accuracy benchmark measures retrieval relevance, graph semantics, audit
false alarms, and mutation recall on pinned real repositories. It is an internal
development workflow, not a public Lexa command.

The benchmark deliberately does not publish one blended accuracy number.
Retrieval, audit precision, and mutation recall answer different questions and
are reported separately.

## Set up a local corpus

Copy the example manifest into the ignored local data directory and replace the
repository paths:

```bash
mkdir -p .lexa-bench
cp benchmarks/accuracy/manifest.example.json .lexa-bench/manifest.json
cargo run -p xtask -- accuracy-bench prepare --config .lexa-bench/manifest.json
```

`prepare` resolves each configured ref to a commit, selects and freezes the
configured number of eligible historical tasks, generates deterministic path
cases, runs the current Lexa audit, and writes:

```text
.lexa-bench/
├── manifest.json
├── dataset.json
├── historical-tasks.jsonl
├── tool-cases.jsonl
├── audit-labels.jsonl
└── mutations.jsonl
```

All paths, labels, tasks, mutations, and reports under `.lexa-bench/` are
gitignored. The committed files in `benchmarks/accuracy/` document the portable
schema without exposing private corpus metadata.

## Review audit findings

Edit `audit-labels.jsonl` and set each `verdict` to one of:

- `correct_actionable`: the structural claim is correct and worth presenting as
  an action.
- `correct_not_actionable`: the claim is factually correct but should be a risk
  note, expected condition, or suppressed alert.
- `false_positive`: Lexa's structural claim is wrong.
- `uncertain`: dynamic or framework behavior prevents a reliable judgment.

Leave `verdict` as `null` until a finding has actually been reviewed. Uncertain
findings are reported but excluded from precision denominators.

Running `prepare` again preserves verdicts and notes for findings with the same
repository, commit, and finding ID.

## Define mutations

Mutation recipes use one exact replacement or insertion precondition and one or
more expected audit findings or tool items. Copy and adapt the records in
`benchmarks/accuracy/mutation.example.jsonl`. Every recipe runs in a fresh
detached worktree, and a stale or non-unique anchor fails the case without
touching the original repository.

## Run and compare

```bash
cargo run -p xtask -- accuracy-bench run --config .lexa-bench/manifest.json
```

To benchmark another binary or compare with a prior local run:

```bash
cargo run -p xtask -- accuracy-bench run \
  --config .lexa-bench/manifest.json \
  --lexa-bin /path/to/lexa \
  --baseline .lexa-bench/runs/PRIOR_RUN/metrics.json
```

During runner development, `--history-limit 0` skips the expensive historical
portion while still exercising pinned tool cases, audit collection, mutations,
and report generation. Release baselines must use the complete frozen task set.

Each run writes `summary.md`, `metrics.json`, `cases.jsonl`,
`audit-findings.jsonl`, `mutation-results.jsonl`, and `regressions.md` beneath
`.lexa-bench/runs/<run-id>/`. Baselines are observational in v1; only execution,
schema, stale-data, or cleanup failures make the command fail.

Historical commit diffs are used as recall targets only. They are not assumed
to be the complete set of useful context, so automatic commit tasks do not
claim precision. Reviewed semantic tool cases can be appended to
`tool-cases.jsonl` with `reviewed: true`.
