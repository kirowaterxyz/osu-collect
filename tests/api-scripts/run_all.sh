#!/usr/bin/env bash
# run all api test scripts and report summary

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

for dep in curl jq xxd; do
    if ! command -v "$dep" &>/dev/null; then
        echo "error: missing dependency: $dep"
        exit 1
    fi
done

chmod +x "$SCRIPT_DIR"/*.sh

PASS=0
FAIL=0
FAILED_SCRIPTS=()

run_test() {
    local script="$1"
    local name
    name=$(basename "$script")
    echo ""
    echo "========================================"
    if bash "$script"; then
        PASS=$((PASS + 1))
    else
        FAIL=$((FAIL + 1))
        FAILED_SCRIPTS+=("$name")
    fi
}

run_test "$SCRIPT_DIR/test_osucollector.sh"
run_test "$SCRIPT_DIR/test_github_releases.sh"
run_test "$SCRIPT_DIR/test_nekoha_size.sh"
run_test "$SCRIPT_DIR/test_mirrors_download.sh"
run_test "$SCRIPT_DIR/test_osu_official.sh"

echo ""
echo "========================================"
echo "summary: $PASS passed, $FAIL failed"
if [[ $FAIL -gt 0 ]]; then
    echo "failed scripts:"
    for s in "${FAILED_SCRIPTS[@]}"; do
        echo "  - $s"
    done
    exit 1
fi
