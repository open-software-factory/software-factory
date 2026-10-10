#!/bin/sh
# Proves the image carries no root caches and no dsh session state left by
# the build-time dsh test. It runs as root so it can read root's home. Run by
# hand, inside a container as root, with:
# sh .devcontainer/tests/image-clean.sh
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

# check_absent PATH: PATH must not exist.
check_absent() {
  path="$1"
  if [ ! -e "$path" ]; then
    pass "$path does not exist"
  else
    fail "$path does not exist"
  fi
}

# check_cache PATH: PATH must be absent or hold under 50 MB.
check_cache() {
  path="$1"
  if [ ! -e "$path" ]; then
    pass "$path is absent or under 50 MB"
    return
  fi
  size="$(du -sm "$path" | cut -f1)"
  if [ "$size" -lt 50 ]; then
    pass "$path is absent or under 50 MB"
  else
    fail "$path is absent or under 50 MB (${size} MB)"
  fi
}

dev_home="$(getent passwd dev | cut -d: -f6)"
root_home="$(getent passwd root | cut -d: -f6)"
dsh_dir="$dev_home/.dsh"

# The build-time dsh test leaves these behind in the dev home.
check_absent "$dsh_dir/sessions"
check_absent "$dsh_dir/storages"
check_absent "$dsh_dir/.anonymous-user-id"

# A BuildKit cache mount keeps these out of the image layer.
check_cache "$root_home/.npm"
check_cache "$root_home/.cache"
check_cache "$root_home/.local/share/pnpm"

echo ""
echo "$((TOTAL - FAILURES))/$TOTAL passed"
[ "$FAILURES" -eq 0 ]
