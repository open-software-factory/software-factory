#!/bin/sh
# Proves the pinned third-party skills are installed for the container user
# and are not empty. They are not linted here; see docs/development.md. Run
# by hand, inside the container, with: sh .devcontainer/tests/skills-installed.sh
set -eu

TOTAL=0
FAILURES=0

LOCK="$HOME/.agents/.skill-lock.json"

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
  if [ -d "$HOME/.agents/skills/$skill" ] && [ -s "$HOME/.agents/skills/$skill/SKILL.md" ]; then
    pass "$skill is installed and not empty"
  else
    fail "$skill is installed and not empty"
  fi

  listed="no"
  ref=""
  if [ -f "$LOCK" ]; then
    # The entry key is the exact skill name, so archify cannot match archify-review.
    if grep -q "\"$skill\"[[:space:]]*:[[:space:]]*{" "$LOCK"; then
      listed="yes"
      ref=$(sed -n "/\"$skill\"[[:space:]]*:[[:space:]]*{/,/^[[:space:]]*}/p" "$LOCK" \
            | sed -n 's/.*"ref"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
    fi
  fi
  if [ "$listed" = "yes" ] && printf '%s\n' "$ref" | grep -q '^[0-9a-f]\{40\}$'; then
    pass "$skill is listed in the lock file with a full commit ref"
  else
    fail "$skill is listed in the lock file with a full commit ref"
  fi
done

echo ""
echo "$((TOTAL - FAILURES))/$TOTAL passed"
[ "$FAILURES" -eq 0 ]
