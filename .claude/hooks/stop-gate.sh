#!/usr/bin/env bash
# Stop: run the fast gate (scripts/gate.sh) before Claude ends its turn.
# - green: silent, Claude may stop.
# - red: blocks the stop once and hands Claude the end of the gate's output.
# - already continuing because of a stop hook (`stop_hook_active`): never
#   blocks again, so a gate Claude can't fix doesn't loop forever.
# - gate takes longer than GATE_TIMEOUT seconds: lets Claude stop and only
#   shows a warning (a slow cold build is not a reason to block).
set -uo pipefail

input=$(cat)
active=$(jq -r '.stop_hook_active // false' <<<"$input" 2>/dev/null || echo false)
project=${CLAUDE_PROJECT_DIR:-$(pwd)}
timeout_s=${GATE_TIMEOUT:-240}

if [[ $active == true ]]; then
    exit 0
fi

out=$(timeout "$timeout_s" "$project/scripts/gate.sh" 2>&1)
status=$?

if [[ $status -eq 0 ]]; then
    exit 0
fi

if [[ $status -eq 124 ]]; then
    jq -n --arg t "$timeout_s" \
        '{systemMessage: ("Stop hook: scripts/gate.sh did not finish in " + $t + " s (cold build?). Not blocking; run it by hand before committing.")}'
    exit 0
fi

# Strip ANSI colors and keep the tail, where cargo puts the errors and the
# failing tests summary.
tail_out=$(sed 's/\x1b\[[0-9;]*m//g' <<<"$out" | tail -n 80)
jq -n --arg out "$tail_out" '{
  decision: "block",
  reason: ("The fast gate (scripts/gate.sh) is red, so the task is not done. Fix the cause (do not disable or delete tests), then finish. Last lines of its output:\n\n" + $out)
}'
exit 0
