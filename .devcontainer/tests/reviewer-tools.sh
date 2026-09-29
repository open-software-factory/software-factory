#!/bin/sh
# Exercises each reviewer CLI's --version, run as the dev user (see the
# Dockerfile), so a tool missing from dev's PATH fails the build instead
# of shipping quietly. Run by hand, inside a container, with:
# sh .devcontainer/tests/reviewer-tools.sh
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

# assert_version NAME CMD...  Runs CMD, expects exit 0.
assert_version() {
  name="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    pass "$name"
  else
    fail "$name"
  fi
}

assert_version "codex --version" codex --version
assert_version "claude --version" claude --version
assert_version "dsh --version" dsh --version
assert_version "opencode --version" opencode --version

echo ""
echo "$((TOTAL - FAILURES))/$TOTAL passed"
[ "$FAILURES" -eq 0 ]
