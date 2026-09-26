#!/bin/sh
# Exercises the git wrapper (.devcontainer/git-wrapper) and the forced
# hooks path (.devcontainer/Dockerfile) inside the built image. The
# Dockerfile runs this at build time; a person can also run it by hand,
# inside a container, with: sh .devcontainer/tests/git-wrapper.sh
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

# assert_refused NAME CMD...  Runs CMD, expects a nonzero exit and the
# word "refused" on stderr, the wrapper's own signature for every case
# in this file (as opposed to some other, unrelated git failure).
assert_refused() {
  name="$1"
  shift
  err_file=$(mktemp)
  if "$@" >/dev/null 2>"$err_file"; then
    fail "$name (expected a refusal; git exited 0)"
  elif grep -q 'refused' "$err_file"; then
    pass "$name"
  else
    fail "$name (git failed, but not with a refusal: $(cat "$err_file"))"
  fi
  rm -f "$err_file"
}

# assert_ok NAME CMD...  Runs CMD, expects exit 0.
assert_ok() {
  name="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    pass "$name"
  else
    fail "$name (expected success; git failed)"
  fi
}

WORKSPACE_ROOT="${OSF_WORKSPACE_ROOT:-/workspace}"
SCRATCH=$(mktemp -d /tmp/git-wrapper-test.XXXXXX)
trap 'rm -rf "$SCRATCH"' EXIT

git config --global user.email test@example.com
git config --global user.name Test
git config --global init.defaultBranch main

# --- Pass-throughs that must keep working. ---
assert_ok "git --version passes through" git --version
assert_ok "git --help passes through" git --help

# --- The forced hooks path, scoped to the workspace mount. ---
# A commit under the workspace root: git's own trace shows it running
# the container's root-owned hook, not any hook the repository names
# itself.
mkdir -p "$WORKSPACE_ROOT"
ws_repo="$WORKSPACE_ROOT/git-wrapper-test-$$"
mkdir -p "$ws_repo"
(cd "$ws_repo" && git init -q)
ws_trace=$(mktemp)
(cd "$ws_repo" && GIT_TRACE=1 git commit --allow-empty -q -m test) 2>"$ws_trace" || true
if grep -q 'run_command:.*/opt/factory/githooks/pre-commit' "$ws_trace"; then
  pass "a commit under the workspace root runs /opt/factory/githooks/pre-commit"
else
  fail "a commit under the workspace root runs /opt/factory/githooks/pre-commit (trace was: $(cat "$ws_trace"))"
fi
rm -f "$ws_trace"
rm -rf "$ws_repo"

# A commit under /tmp, outside the workspace root: the same trace line
# must not appear, and the repository's own (empty) hooks directory
# lets the commit through.
tmp_repo="$SCRATCH/scratch-repo"
mkdir -p "$tmp_repo"
(cd "$tmp_repo" && git init -q)
tmp_trace=$(mktemp)
(cd "$tmp_repo" && GIT_TRACE=1 git commit --allow-empty -q -m test) 2>"$tmp_trace" || true
if grep -q 'run_command:.*/opt/factory/githooks/pre-commit' "$tmp_trace"; then
  fail "a commit under /tmp does not run the container hook (it did)"
else
  pass "a commit under /tmp does not run the container hook"
fi
rm -f "$tmp_trace"

cd "$tmp_repo"

# --- Refusals: skipping a check. ---
assert_refused "git commit --no-verify is refused" git commit --no-verify -q -m test --allow-empty
assert_refused "git commit -n is refused" git commit -n -q -m test --allow-empty
assert_refused "git push --no-verify is refused" git push --no-verify origin HEAD

# --- Refusals: a caller-chosen hooks path, every route. ---
assert_refused "-c core.hooksPath=... is refused" git -c core.hooksPath=/tmp/evil status
assert_refused "-c core.hookspath=... is refused (lowercase key)" git -c core.hookspath=/tmp/evil status
assert_refused "-c CORE.HOOKSPATH=... is refused (any case)" git -c CORE.HOOKSPATH=/tmp/evil status
assert_refused "--config-env core.hooksPath=... is refused" git --config-env core.hooksPath=SOME_VAR status
assert_refused "--config-env=core.hooksPath=... is refused" git --config-env=core.hooksPath=SOME_VAR status

# --- Refusals: the two other config keys that can run a command. ---
assert_refused "-c core.fsmonitor=... is refused" git -c core.fsmonitor=x status
assert_refused "-c core.editor=... is refused" git -c core.editor=x status

# --- An unrelated -c is still allowed. ---
assert_ok "-c user.email=... is allowed" git -c user.email=x status

# --- Refusals: the git config environment overrides. ---
assert_refused "GIT_CONFIG_COUNT is refused" env GIT_CONFIG_COUNT=0 git status
assert_refused "GIT_CONFIG_PARAMETERS is refused" env GIT_CONFIG_PARAMETERS="core.hooksPath=/tmp/evil" git status
assert_refused "GIT_CONFIG_GLOBAL is refused" env GIT_CONFIG_GLOBAL=/tmp/evil.gitconfig git status
assert_refused "GIT_CONFIG_SYSTEM is refused" env GIT_CONFIG_SYSTEM=/tmp/evil.gitconfig git status
assert_refused "GIT_CONFIG_KEY_0 alone is refused" env GIT_CONFIG_KEY_0=core.hooksPath git status
assert_refused "GIT_CONFIG_VALUE_0 alone is refused" env GIT_CONFIG_VALUE_0=/tmp/evil git status

# --- Still fails closed for an option this wrapper does not know. ---
assert_refused "an unrecognized option is refused" git --totally-bogus-option status

echo ""
echo "$((TOTAL - FAILURES))/$TOTAL passed"
[ "$FAILURES" -eq 0 ]
