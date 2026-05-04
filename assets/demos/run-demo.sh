#!/usr/bin/env bash
# run-demo.sh — execute a demo script as plain bash so you can preview the
# output before recording the GIF. Each tape has a sibling shell function
# below that runs the same commands, exactly the same way.
#
# Usage:
#   ./assets/demos/run-demo.sh                # runs the headline demo
#   ./assets/demos/run-demo.sh headline       # explicit
#   ./assets/demos/run-demo.sh doctor
#   ./assets/demos/run-demo.sh scan-polyglot
#   ./assets/demos/run-demo.sh hook
#   ./assets/demos/run-demo.sh replay
#   ./assets/demos/run-demo.sh all            # runs every demo in sequence

set -euo pipefail

# Resolve repo root (this script lives at assets/demos/run-demo.sh).
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
cd "$REPO_ROOT"

BIN="./target/release/phantomdep"
if [ ! -x "$BIN" ]; then
    echo "error: $BIN not found. Run 'cargo build --release --workspace' first." >&2
    exit 1
fi

phantomdep() { "$BIN" "$@"; }

banner() {
    printf '\n\033[1;35m=== %s ===\033[0m\n\n' "$*"
}

# `set +e` around each demo so we can capture the real $? from phantomdep
# itself, not from the trailing `|| true`.

demo_headline() {
    banner "phantomdep wrap pip install ..."
    set +e
    phantomdep wrap --dry-run pip install requests fastapi huggingface-cli phantom-pkg-12345
    local rc=$?
    set -e
    echo
    echo "exit code: $rc"
}

demo_doctor() {
    banner "phantomdep doctor"
    set +e
    phantomdep doctor
    set -e
}

demo_scan_polyglot() {
    banner "ls assets/demos/fixtures/"
    ls assets/demos/fixtures/
    echo
    banner "phantomdep scan assets/demos/fixtures/"
    set +e
    phantomdep scan assets/demos/fixtures/
    set -e
}

demo_hook() {
    banner "Claude Code PreToolUse hook"
    set +e
    echo '{"tool_name":"Bash","tool_input":{"command":"pip install requests huggingface-cli"}}' \
        | phantomdep hook check
    local rc=$?
    set -e
    echo "exit: $rc"
}

demo_replay() {
    banner "phantomdep replay"
    phantomdep replay
    echo
    banner "phantomdep benchmark --iterations 50"
    phantomdep benchmark --iterations 50
}

case "${1:-headline}" in
    headline)        demo_headline ;;
    doctor)          demo_doctor ;;
    scan-polyglot)   demo_scan_polyglot ;;
    hook)            demo_hook ;;
    replay)          demo_replay ;;
    all)
        demo_headline
        demo_doctor
        demo_scan_polyglot
        demo_hook
        demo_replay
        ;;
    *)
        echo "unknown demo: $1" >&2
        echo "Usage: $0 [headline|doctor|scan-polyglot|hook|replay|all]" >&2
        exit 2
        ;;
esac
