#!/bin/sh
# Proves the git hooks: each file is executable, calls only an osf command
# and flags the installed osf accepts, and runs cleanly in a scratch
# repository. The Dockerfile runs this at build time. By hand, inside a
# container: sh .devcontainer/tests/git-hooks.sh [SOURCE_HOOKS_DIR]
# SOURCE_HOOKS_DIR is the repository's own .devcontainer/githooks; when
# given, its files must be executable too, since git stores the mode.
set -eu

HOOKS_DIR=/opt/factory/githooks
OSF=/opt/factory/bin/osf
SOURCE_DIR="${1:-}"

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

check_executable() {
  for hook in "$1"/*; do
    if [ -x "$hook" ]; then
      pass "$hook is executable"
    else
      fail "$hook is executable"
    fi
  done
}

check_executable "$HOOKS_DIR"
if [ -n "$SOURCE_DIR" ]; then
  check_executable "$SOURCE_DIR"
fi

# Each hook's one exec line names an osf command and flags. Words before the
# first flag are the command; every flag must appear in that command's help.
for hook in "$HOOKS_DIR"/*; do
  line="$(grep '^exec ' "$hook" || true)"
  if [ -z "$line" ]; then
    fail "$hook has an exec line"
    continue
  fi
  set -f
  # shellcheck disable=SC2086
  set -- $line
  set +f
  shift 2
  command_words=""
  flags=""
  for word in "$@"; do
    case "$word" in
      --*) flags="$flags $word" ;;
      -* | '"'*) ;;
      *) [ -n "$flags" ] || command_words="$command_words $word" ;;
    esac
  done
  # shellcheck disable=SC2086
  if help="$("$OSF" $command_words --help 2>&1)"; then
    pass "$hook: osf$command_words exists"
  else
    fail "$hook: osf$command_words exists"
    continue
  fi
  for flag in $flags; do
    if printf '%s\n' "$help" | grep -Eq "(^|[ ,])$flag([ =<]|\$)"; then
      pass "$hook: osf$command_words accepts $flag"
    else
      fail "$hook: osf$command_words accepts $flag"
    fi
  done
done

# Run each hook in a scratch repository outside the workspace mount, where
# the git wrapper leaves hooks alone.
SCRATCH="$(mktemp -d /tmp/git-hooks-test.XXXXXX)"
trap 'rm -rf "$SCRATCH"' EXIT
cd "$SCRATCH"
git init -q -b main
git config user.email test@example.com
git config user.name Test
echo base >base.txt
git add base.txt
git commit -q -m "Add a base file"
git checkout -q -b change
echo change >change.txt
git add change.txt

assert_hook_runs() {
  name="$1"
  shift
  if out="$("$@" 2>&1)"; then
    pass "$name runs and exits 0"
  else
    fail "$name runs and exits 0 ($out)"
  fi
}

assert_hook_runs "pre-commit" "$HOOKS_DIR/pre-commit"
printf 'Add a change file\n' >msg.txt
assert_hook_runs "commit-msg" "$HOOKS_DIR/commit-msg" msg.txt
git commit -q -m "Add a change file"
assert_hook_runs "pre-push" "$HOOKS_DIR/pre-push"

echo ""
echo "$((TOTAL - FAILURES))/$TOTAL passed"
[ "$FAILURES" -eq 0 ]
