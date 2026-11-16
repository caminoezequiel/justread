#!/usr/bin/env bash
# PostToolUse (Edit|Write|MultiEdit): rustfmt the one file that was just written.
# Silent for anything that isn't a .rs file inside this project.
# On a rustfmt error (usually a syntax error) exits 2 so Claude sees the message.
set -uo pipefail

input=$(cat)
file=$(jq -r '.tool_input.file_path // empty' <<<"$input" 2>/dev/null) || exit 0
project=${CLAUDE_PROJECT_DIR:-$(pwd)}

[[ $file == *.rs ]] || exit 0
[[ $file == "$project"/* ]] || exit 0
[[ $file == "$project"/target/* ]] && exit 0
[[ -f $file ]] || exit 0

# Same edition as the workspace (Cargo.toml: edition = "2024"), so the result
# matches `cargo fmt`.
if ! out=$(rustfmt --edition 2024 "$file" 2>&1); then
    echo "rustfmt failed on ${file#"$project"/} (the edit was kept, fix it):" >&2
    echo "$out" | head -40 >&2
    exit 2
fi
exit 0
