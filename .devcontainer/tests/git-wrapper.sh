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

# --- Refusals: skipping a check, short and long spellings. ---
assert_refused "git commit --no-verify is refused" git commit --no-verify -q -m test --allow-empty
assert_refused "git commit --no-v is refused (abbreviated)" git commit --no-v -q -m test --allow-empty
assert_refused "git commit --no-ve is refused (abbreviated)" git commit --no-ve -q -m test --allow-empty
assert_refused "git commit --no-ver is refused (abbreviated)" git commit --no-ver -q -m test --allow-empty
assert_refused "git commit --no-veri is refused (abbreviated)" git commit --no-veri -q -m test --allow-empty
assert_refused "git commit --no-verif is refused (abbreviated)" git commit --no-verif -q -m test --allow-empty
assert_refused "git commit -n is refused" git commit -n -q -m test --allow-empty
assert_refused "git commit -nm msg is refused" git commit -nm msg
assert_refused "git commit -anm msg is refused" git commit -anm msg
assert_ok "git commit --allow-empty -q -m n is allowed (n is a value)" git commit --allow-empty -q -m n
assert_refused "git merge --no-verif is refused (abbreviated)" git merge --no-verif
assert_refused "git merge -n is refused" git merge -n
assert_refused "git push --no-verify is refused" git push --no-verify origin HEAD
assert_refused "git push --no-verif is refused (abbreviated)" git push --no-verif origin HEAD
# git push -n is a dry run and needs a remote, so this file does not test it.

# --- Refusals: a caller-chosen hooks path, every route. ---
assert_refused "-c core.hooksPath=... is refused" git -c core.hooksPath=/tmp/evil status
assert_refused "-c core.hookspath=... is refused (lowercase key)" git -c core.hookspath=/tmp/evil status
assert_refused "-c CORE.HOOKSPATH=... is refused (any case)" git -c CORE.HOOKSPATH=/tmp/evil status
assert_refused "--config-env core.hooksPath=... is refused" git --config-env core.hooksPath=SOME_VAR status
assert_refused "--config-env=core.hooksPath=... is refused" git --config-env=core.hooksPath=SOME_VAR status

# --- Refusals: a caller-chosen repository or hooks path as an option. ---
assert_refused "git --git-dir=<dir> is refused" git --git-dir=/tmp/x status
assert_refused "git --git-dir <dir> is refused" git --git-dir /tmp/x status
assert_refused "git --work-tree=<dir> is refused" git --work-tree=/tmp/x status
assert_refused "git --work-tree <dir> is refused" git --work-tree /tmp/x status
assert_refused "git --no-hooks is refused" git --no-hooks status
assert_refused "git --skip-hooks is refused" git --skip-hooks status
assert_refused "git --hooks-path=<dir> is refused" git --hooks-path=/tmp/x status
assert_refused "git --hooks-path <dir> is refused" git --hooks-path /tmp/x status

# --- Refusals: environment variables that move the repository. ---
# GIT_DIR here names a repository other than the one git finds.
assert_refused "GIT_DIR pointing elsewhere is refused" env GIT_DIR=/tmp/x git status
assert_refused "GIT_WORK_TREE is refused" env GIT_WORK_TREE=/tmp/x git status
assert_refused "GIT_COMMON_DIR is refused" env GIT_COMMON_DIR=/tmp/x git status

# --- The one allowed GIT_DIR case: git exports it to hooks in a worktree. ---
assert_ok "GIT_DIR naming this repository's own .git is allowed" \
  env GIT_DIR="$tmp_repo/.git" git status

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

# --- The one allowed git config environment override. ---
assert_ok "GIT_CONFIG_COUNT=1 with core.fsmonitor=false is allowed" \
  env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.fsmonitor GIT_CONFIG_VALUE_0=false git status
assert_ok "the allowed key is matched in any case" \
  env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=Core.FSMonitor GIT_CONFIG_VALUE_0=false git status
assert_refused "a second config pair is refused" \
  env GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=core.fsmonitor GIT_CONFIG_VALUE_0=false GIT_CONFIG_KEY_1=user.name GIT_CONFIG_VALUE_1=x git status
assert_refused "a different key is refused" \
  env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/tmp/evil git status
assert_refused "core.fsmonitor=true is refused" \
  env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.fsmonitor GIT_CONFIG_VALUE_0=true git status
assert_refused "a count with a missing pair is refused" \
  env GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=core.fsmonitor GIT_CONFIG_VALUE_0=false git status
assert_refused "an extra config pair past the count is refused" \
  env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.fsmonitor GIT_CONFIG_VALUE_0=false GIT_CONFIG_KEY_1=core.hooksPath GIT_CONFIG_VALUE_1=/tmp/evil git status
assert_refused "a non-numeric GIT_CONFIG_COUNT is refused" \
  env GIT_CONFIG_COUNT=x git status

# --- Still fails closed for an option this wrapper does not know. ---
assert_refused "an unrecognized option is refused" git --totally-bogus-option status

echo ""
echo "$((TOTAL - FAILURES))/$TOTAL passed"
[ "$FAILURES" -eq 0 ]
