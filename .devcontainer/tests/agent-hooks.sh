#!/bin/sh
# Proves the hook wiring from the [agents] table of osf.toml, selected from
# crates/osf/src/agents.rs: every enabled agent's hook file exists, names
# osf, is owned by root, and the current user cannot write to it; a disabled
# agent has none. osf hook itself answers a sample stop and a sample prompt
# payload. Names no agent directly; everything it checks comes from
# `osf agents list --json`, run from the folder that holds osf.toml. Run by
# hand, inside a container, from that folder, with: sh tests/agent-hooks.sh
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

# check_enabled_hook AGENT FILE: FILE exists, names osf, is root-owned, and
# the current user cannot write to it.
check_enabled_hook() {
  agent="$1"
  path="$2"
  if [ ! -e "$path" ]; then
    fail "$agent: hook file $path exists"
    return
  fi
  pass "$agent: hook file $path exists"

  if grep -Eq 'osf hook|@open-software-factory/osf-' "$path"; then
    pass "$agent: $path names osf"
  else
    fail "$agent: $path names osf"
  fi

  owner="$(stat -c '%U' "$path")"
  if [ "$owner" = "root" ]; then
    pass "$agent: $path is root-owned"
  else
    fail "$agent: $path is root-owned (owner is $owner)"
  fi

  if [ -w "$path" ]; then
    fail "$agent: $path is not writable by the current user"
  else
    pass "$agent: $path is not writable by the current user"
  fi
}

# Every row `osf agents list --json` prints for the osf.toml in this folder.
# The run happens here, in the folder that holds that file.
rows="$(osf agents list --json)"

builders="$(printf '%s' "$rows" | jq '[.[] | select(.builder == true)] | length')"
if [ "$builders" -eq 1 ]; then
  pass "exactly one agent is the builder"
else
  fail "exactly one agent is the builder (found $builders)"
fi

count="$(printf '%s' "$rows" | jq 'length')"
i=0
while [ "$i" -lt "$count" ]; do
  agent="$(printf '%s' "$rows" | jq -r ".[$i].name")"
  hooks="$(printf '%s' "$rows" | jq -r ".[$i].hooks")"
  enabled="$(printf '%s' "$rows" | jq -r ".[$i].enabled")"
  paths="$HOME/$hooks"
  # The image carries the dsh patch in both its factory and headless profiles.
  case "$hooks" in
    .dsh/cordis.patch.yml)
      paths="$HOME/.dsh/profiles/factory/cordis.patch.yml $HOME/.dsh/profiles/headless/cordis.patch.yml"
      ;;
  esac
  for path in $paths; do
    if [ "$enabled" = "true" ]; then
      check_enabled_hook "$agent" "$path"
    else
      if [ -e "$path" ]; then
        fail "$agent: disabled agent has no hook file at $path"
      else
        pass "$agent: disabled agent has no hook file"
      fi
    fi
  done
  i=$((i + 1))
done

# An agent left out of osf.toml gets no hook file: select only the first
# agent in a scratch folder and install there.
first_agent="$(printf '%s' "$rows" | jq -r '.[0].name')"
first_hooks="$(printf '%s' "$rows" | jq -r '.[0].hooks')"
scratch="$(mktemp -d)"
scratch_root="$scratch/root"
mkdir -p "$scratch_root"
printf '[agents]\nenabled = ["%s"]\n' "$first_agent" > "$scratch/osf.toml"
install_out="$(cd "$scratch" && osf hooks install --agents --root "$scratch_root")"

written="$(printf '%s\n' "$install_out" | grep '^osf hooks install: wrote ' | wc -l | tr -d ' ')"
if [ "$written" -eq 1 ]; then
  pass "only $first_agent enabled: the install wrote one file"
else
  fail "only $first_agent enabled: the install wrote one file (wrote $written)"
fi

written_path="$(printf '%s\n' "$install_out" | sed -n 's/^osf hooks install: wrote //p')"
expected="$scratch_root/$first_hooks"
if [ "$written_path" = "$expected" ]; then
  pass "the one file is $first_agent's hook path"
else
  fail "the one file is $first_agent's hook path (got $written_path)"
fi

rm -rf "$scratch"

# osf hook itself, against one sample payload per event it supports. These
# payloads are shaped like a real event; osf hook reads every harness's own
# spelling (see crates/osf/src/hook.rs).
STOP_PAYLOAD='{"session_id":"agent-hooks-test","hook_event_name":"Stop","last_assistant_message":"Done."}'
if printf '%s' "$STOP_PAYLOAD" | osf hook stop >/dev/null; then
  pass "osf hook stop runs for a sample stop payload"
else
  fail "osf hook stop runs for a sample stop payload"
fi

PROMPT_PAYLOAD='{"session_id":"agent-hooks-test"}'
if printf '%s' "$PROMPT_PAYLOAD" | osf hook prompt >/dev/null; then
  pass "osf hook prompt runs for a sample prompt payload"
else
  fail "osf hook prompt runs for a sample prompt payload"
fi

echo ""
echo "$((TOTAL - FAILURES))/$TOTAL passed"
[ "$FAILURES" -eq 0 ]
