#!/bin/sh
# Proves every root-owned toolchain folder is read-only for the dev user:
# no entry under any of them, other than a symlink, may be writable by the
# current user. Run by hand, inside a container as the dev user, with:
# sh .devcontainer/tests/toolchain-readonly.sh
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

# check_dir DIR REQUIRED: no entry under DIR, other than a symlink, is
# writable by the current user. A missing DIR fails when REQUIRED is "yes",
# and is skipped otherwise.
check_dir() {
  dir="$1"
  required="$2"
  if [ ! -d "$dir" ]; then
    if [ "$required" = "yes" ]; then
      fail "$dir exists"
    else
      echo "skip - $dir does not exist"
    fi
    return
  fi
  status=0
  writable="$(find "$dir" ! -type l -writable)" || status=$?
  if [ "$status" -ne 0 ]; then
    fail "$dir: find reports no error"
    return
  fi
  if [ -n "$writable" ]; then
    fail "$dir has no entry writable by the current user"
    printf '%s\n' "$writable" | head -n 10 >&2
  else
    pass "$dir has no entry writable by the current user"
  fi
}

check_dir /usr/local/lib/nodejs yes
check_dir /usr/local/bin yes
check_dir /usr/local/cargo no
check_dir /usr/local/rustup no
check_dir /usr/local/share/corepack no
check_dir /opt/factory yes
check_dir /opt/dsh no
check_dir /etc/codex no

echo ""
echo "$((TOTAL - FAILURES))/$TOTAL passed"
[ "$FAILURES" -eq 0 ]
