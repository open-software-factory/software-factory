#!/bin/sh
# Checks .devcontainer/worktree-container against a stub container engine
# that records its arguments, so no engine or image is needed. Run it with:
# sh .devcontainer/tests/worktree-container.sh
set -eu

HERE=$(cd "$(dirname "$0")" && pwd -P)
SCRIPT="$HERE/../worktree-container"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

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
# check NAME PATTERN FILE: the file must hold a line matching the pattern.
check() {
  if grep -q -- "$2" "$3"; then pass "$1"; else fail "$1 (no line matches '$2')"; fi
}
check_absent() {
  if grep -q -- "$2" "$3"; then fail "$1 (a line matches '$2')"; else pass "$1"; fi
}

# A stub engine: `container inspect` reports no container, every other
# call is recorded, one line per call.
cat >"$TMP/engine" <<'EOF'
#!/bin/sh
echo "$*" >>"$STUB_LOG"
[ "$1" = "container" ] && exit 1
exit 0
EOF
chmod +x "$TMP/engine"

git init -q "$TMP/main"
git -C "$TMP/main" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m init
git -C "$TMP/main" worktree add -q "$TMP/one" -b one
git -C "$TMP/main" worktree add -q "$TMP/two" -b two
mkdir "$TMP/alike"
git -C "$TMP/main" worktree add -q "$TMP/alike/one" -b alike-one

export OSF_CONTAINER_ENGINE="$TMP/engine"
export OSF_IMAGE="example-image:tag"
export STUB_LOG="$TMP/log"

"$SCRIPT" start "$TMP/one" >"$TMP/name-one"
common=$(cd "$TMP/main/.git" && pwd -P)
run=$(grep '^run ' "$STUB_LOG")
printf '%s\n' "$run" >"$TMP/run-one"

check "the worktree is mounted read-write at /workspace" ":/workspace " "$TMP/run-one"
check "the main git directory is mounted at its own host path" "-v $common:$common " "$TMP/run-one"
check "the shared config is mounted read-only over it" "-v $common/config:$common/config:ro " "$TMP/run-one"
check "the token is passed by name, not by value" "-e GH_TOKEN " "$TMP/run-one"
check "the image comes from OSF_IMAGE" " example-image:tag " "$TMP/run-one"
# Exactly three mounts: the worktree, the git directory, the config file.
if [ "$(grep -o -- ' -v ' "$TMP/run-one" | wc -l)" -eq 3 ]; then
  pass "only three mounts, so no home or login folder"
else
  fail "only three mounts, so no home or login folder"
fi
check_absent "the home folder is not named" "-v $HOME[:/]" "$TMP/run-one"

"$SCRIPT" name "$TMP/two" >"$TMP/name-two"
"$SCRIPT" name "$TMP/alike/one" >"$TMP/name-alike"
if [ "$(cat "$TMP/name-one")" != "$(cat "$TMP/name-two")" ]; then
  pass "two worktrees get two names"
else
  fail "two worktrees get two names"
fi
if [ "$(cat "$TMP/name-one")" != "$(cat "$TMP/name-alike")" ]; then
  pass "two worktrees with the same folder name get two names"
else
  fail "two worktrees with the same folder name get two names"
fi

if "$SCRIPT" start "$TMP/main" 2>"$TMP/err"; then
  fail "a main clone is refused"
else
  check "a main clone is refused" "not a linked worktree" "$TMP/err"
fi

echo "$TOTAL checks, $FAILURES failed"
[ "$FAILURES" -eq 0 ]
