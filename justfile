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

verify: fmt lint test build

gen-skill:
	cargo run -p xtask -- gen-skill

gen-skill-check:
	cargo run -p xtask -- gen-skill --check
