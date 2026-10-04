#!/bin/sh
# Proves the pinned third-party skills are installed for the container user
# and are not empty. They are not linted here; see docs/development.md. Run
# by hand, inside the container, with: sh .devcontainer/tests/skills-installed.sh
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

for skill in archify archify-review; do
  if [ -s "$HOME/.agents/skills/$skill/SKILL.md" ]; then
    pass "$skill is installed and not empty"
  else
    fail "$skill is installed and not empty"
  fi
done

echo ""
echo "$((TOTAL - FAILURES))/$TOTAL passed"
[ "$FAILURES" -eq 0 ]
