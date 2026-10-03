#!/bin/sh
# Proves the hook wiring from .devcontainer/agents.json: every agent's hook
# file or directory exists, is owned by root, and dev cannot write to it;
# osf hook itself answers a sample stop and a sample prompt payload. Names
# no agent directly; everything it checks comes from agents.json. Run by
# hand, inside a container, with: sh .devcontainer/tests/agent-hooks.sh
set -eu

AGENTS_JSON="$(dirname "$0")/../agents.json"

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

check_hook_path() {
  agent="$1"
  path="$2"
  if [ ! -e "$path" ]; then
    fail "$agent: hook path $path exists"
    return
  fi
  pass "$agent: hook path $path exists"

  owner="$(stat -c '%U' "$path")"
  if [ "$owner" = "root" ]; then
    pass "$agent: $path is root-owned"
  else
    fail "$agent: $path is root-owned (owner is $owner)"
  fi

  if [ -w "$path" ]; then
    fail "$agent: $path is not writable by dev"
  else
    pass "$agent: $path is not writable by dev"
  fi
}

default_builder="$(jq -r '.default_builder' "$AGENTS_JSON")"
if [ -n "$default_builder" ] && [ "$default_builder" != "null" ]; then
  pass "a default builder is configured ($default_builder)"
else
  fail "a default builder is configured"
fi

count="$(jq -r '.agents | length' "$AGENTS_JSON")"
i=0
while [ "$i" -lt "$count" ]; do
  name="$(jq -r ".agents[$i].name" "$AGENTS_JSON")"
  path="$(jq -r ".agents[$i].hook_path" "$AGENTS_JSON")"
  # agents.json spells a home-relative path with "~", the shorthand the
  # writing lint accepts in place of a literal /home/<user> path; expand
  # it here, the one place this script turns documentation into a real
  # path to stat.
  case "$path" in
    "~/"*) path="$HOME/${path#\~/}" ;;
  esac
  check_hook_path "$name" "$path"
  i=$((i + 1))
done

# osf hook itself, against one sample payload per event it supports. These
# payloads are shaped like a real Claude Code event; osf hook reads every
# harness's own spelling (see crates/osf/src/hook.rs).
STOP_PAYLOAD='{"session_id":"agent-hooks-test","hook_event_name":"Stop","last_assistant_message":"Done."}'
if printf '%s' "$STOP_PAYLOAD" | osf hook stop >/dev/null 2>&1; then
  pass "osf hook stop runs for a sample stop payload"
else
  fail "osf hook stop runs for a sample stop payload"
fi

PROMPT_PAYLOAD='{"session_id":"agent-hooks-test"}'
if printf '%s' "$PROMPT_PAYLOAD" | osf hook prompt >/dev/null 2>&1; then
  pass "osf hook prompt runs for a sample prompt payload"
else
  fail "osf hook prompt runs for a sample prompt payload"
fi

echo ""
echo "$((TOTAL - FAILURES))/$TOTAL passed"
[ "$FAILURES" -eq 0 ]
