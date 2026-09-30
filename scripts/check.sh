#!/bin/sh
# themelios's single check entry point. Each mode is one part of the standing
# gate every change is held to; `full` runs them all. Run from anywhere in the
# repository. Requires Rust 1.97 or newer with clippy and rustfmt (CI pins
# 1.97.1); `coverage` needs cargo-llvm-cov, and `differential` needs pixi,
# which supplies clingo 5.8.2 and the tree-sitter CLI (pixi.toml).
#
# The hosted workflow, .github/workflows/checks.yml, runs these same checks on
# every push and pull request; run `full` green before you push.
#
#   scripts/check.sh portable      fmt, clippy (all features), test, the syntax examples, doc (-D warnings)
#   scripts/check.sh coverage      line coverage, floor 90 (cargo-llvm-cov)
#   scripts/check.sh differential  the clingo differentials and the tree-sitter cross-check (via pixi)
#   scripts/check.sh full          portable, coverage, then differential (only when pixi is present)
set -eu

# Run from the repository root regardless of the caller's directory.
cd "$(dirname "$0")/.."

usage() {
	echo "usage: scripts/check.sh <portable|coverage|differential|full>" >&2
	exit 2
}

portable() {
	cargo fmt --all --check
	# --all-features lints the feature-gated differential harnesses too, so they
	# are held to -D warnings like the default build.
	cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
	cargo test --workspace --locked
	# The syntax tier's example witnesses are executed, not merely compiled.
	for example in comments_as_data diagnostics_quality hostile_input asp_core_2; do
		cargo run -q -p themelios-syntax --example "$example" --locked >/dev/null
	done
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
}

coverage() {
	# The fuzz crate is a harness, not a tested surface; it is excluded from the
	# population, as the hosted workflow excludes it.
	cargo llvm-cov --workspace --exclude themelios-syntax-fuzz --locked --fail-under-lines 90
}

differential() {
	if ! command -v pixi >/dev/null 2>&1; then
		echo "differential mode needs pixi, which supplies clingo 5.8.2 (pixi.toml)" >&2
		echo "install: https://pixi.sh, then rerun; pixi resolves the committed pixi.lock" >&2
		exit 3
	fi
	pixi run differential
	pixi run differential-program
	pixi run differential-analysis
	pixi run cross-check
}

[ $# -eq 1 ] || usage
case "$1" in
	portable) portable ;;
	coverage) coverage ;;
	differential) differential ;;
	full)
		portable
		coverage
		# The differentials are out of band: run them when pixi is present, else
		# note the skip and let the rest of the gate stand.
		if command -v pixi >/dev/null 2>&1; then
			differential
		else
			echo "note: differential skipped (pixi is not on PATH)" >&2
		fi
		;;
	*) usage ;;
esac
