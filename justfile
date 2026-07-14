fmt:
	cargo fmt -- --check

lint:
	cargo clippy --all-targets --all-features -- -D warnings

test:
	cargo test --locked

build:
	cargo build --locked

bench:
	cargo bench --bench engine

perf-gate:
	cargo run -p xtask -- perf-gate

accuracy-prepare:
	cargo run -p xtask -- accuracy-bench prepare --config .lexa-bench/manifest.json

accuracy-bench:
	cargo run -p xtask -- accuracy-bench run --config .lexa-bench/manifest.json

verify: fmt lint test build

gen-skill:
	cargo run -p xtask -- gen-skill

gen-skill-check:
	cargo run -p xtask -- gen-skill --check
