#!/bin/sh
# Proves dsh starts on the headless and factory profiles. With no
# credentials, each run must exit non-zero with MISSING_CREDENTIAL and
# without a plugin that failed to load. Run by hand, inside a container with
# no ~/.dsh/.credentials.yaml and no DEEPSEEK_API_KEY, with:
# sh .devcontainer/tests/dsh-starts.sh
set -eu

TOTAL=0
FAILURES=0

pass() {
  TOTAL=$((TOTAL + 1))
  echo "ok - $1"
}

fail() {
  TOTAL=$((TOTAL + 1))
  FAILURES=$((FAILURES + 1))
  echo "not ok - $1" >&2
}

# check_profile PROFILE  Runs dsh once on PROFILE and checks the output.
check_profile() {
  profile="$1"
  status=0
  output=$(dsh --profile "$profile" "reply ok" 2>&1) || status=$?
  echo "$output"
  if [ "$status" -ne 0 ]; then
    pass "$profile exits non-zero without credentials"
  else
    fail "$profile exits non-zero without credentials"
  fi
  if echo "$output" | grep -q "MISSING_CREDENTIAL"; then
    pass "$profile stops at MISSING_CREDENTIAL"
  else
    fail "$profile stops at MISSING_CREDENTIAL"
  fi
  if echo "$output" | grep -Eq "failed to load|failed to import|did not activate"; then
    fail "$profile loads every plugin"
  else
    pass "$profile loads every plugin"
  fi
}

check_profile headless
check_profile factory

echo ""
echo "$((TOTAL - FAILURES))/$TOTAL passed"
[ "$FAILURES" -eq 0 ]
