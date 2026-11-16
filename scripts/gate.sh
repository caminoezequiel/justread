#!/usr/bin/env bash
# The fast gate: the same three checks CI's `test` job runs (.github/workflows/ci.yml).
# Used by the Claude Code Stop hook, the git pre-push hook, and by hand.
# Integration tests (epub-core/tests/*.rs) are part of `cargo test --workspace`;
# they read local fixture EPUBs and take well under a second.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> cargo fmt --all --check"
cargo fmt --all --check
echo "==> cargo clippy --workspace --all-targets --locked -- -D warnings"
cargo clippy --workspace --all-targets --locked -- -D warnings
echo "==> cargo test --workspace --locked"
cargo test --workspace --locked
echo "==> gate OK"
