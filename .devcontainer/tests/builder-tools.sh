#!/bin/sh
# Exercises every tool the builder role needs, run as the dev user (see the
# Dockerfile). One image serves both the builder and the reviewer role;
# this script covers the builder-only tools the coding-agent CLIs do not.
# Run by hand, inside a container, with:
# sh .devcontainer/tests/builder-tools.sh
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

assert_ok() {
  name="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    pass "$name"
  else
    fail "$name"
  fi
}

assert_ok "git --version" git --version
assert_ok "git-town --version" git-town --version
assert_ok "gh --version" gh --version
assert_ok "cargo --version" cargo --version
assert_ok "rustfmt --version" rustfmt --version
assert_ok "cargo-clippy --version" cargo-clippy --version
assert_ok "osf --version" osf --version
assert_ok "moon --version" moon --version
assert_ok "node --version" node --version
assert_ok "pnpm --version" pnpm --version
assert_ok "actionlint -version" actionlint -version
assert_ok "uv --version" uv --version
assert_ok "bun --version" bun --version
assert_ok "skillspector --help" skillspector --help
assert_ok "skillevaluator --help" skillevaluator --help

# A login shell resets PATH before sourcing /etc/profile.d/*.sh (see
# .devcontainer/profile.d/osf-path.sh). Every tool above must still be on
# PATH from a login shell, since a harness or a human may well start one.
TOOLS="osf moon dsh omp opencode codex claude actionlint gh git-town cargo node pnpm uv bun"
for tool in $TOOLS; do
  if bash -lc "command -v $tool" >/dev/null 2>&1; then
    pass "$tool is on PATH in a login shell"
  else
    fail "$tool is on PATH in a login shell"
  fi
  if bash -c "command -v $tool" >/dev/null 2>&1; then
    pass "$tool is on PATH in a non-login shell"
  else
    fail "$tool is on PATH in a non-login shell"
  fi
done

echo ""
echo "$((TOTAL - FAILURES))/$TOTAL passed"
[ "$FAILURES" -eq 0 ]
