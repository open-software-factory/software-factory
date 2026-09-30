#!/usr/bin/env bash
# Publishes rendered pr-lens SVGs to the orphan pr-lens branch, the way
# pr-lens's own action publishes them (coldteadotai/pr-lens, MIT,
# packages/action/scripts/publish.sh): one directory per pull request per
# commit, so a changed diagram arrives as a new URL rather than new bytes
# at an old one that GitHub's comment-image cache would never revalidate.
set -euo pipefail

DATA_BRANCH="pr-lens"
DIRECTORY="pr/${PR_NUMBER}/${HEAD_SHA}"
WORKSPACE="${RUNNER_TEMP}/pr-lens-publish"

for ATTEMPT in 1 2 3 4 5; do
  rm -rf "${WORKSPACE}"
  mkdir -p "${WORKSPACE}"
  cd "${WORKSPACE}"

  git init --quiet
  git config user.name "github-actions[bot]"
  git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
  git remote add origin "https://x-access-token:${GITHUB_TOKEN}@github.com/${GITHUB_REPOSITORY}.git"

  if git fetch --quiet --depth=1 origin "${DATA_BRANCH}" 2>/dev/null; then
    git checkout --quiet -b "${DATA_BRANCH}" FETCH_HEAD
  else
    git checkout --quiet --orphan "${DATA_BRANCH}"
  fi

  mkdir -p "${DIRECTORY}"
  cp "${RUNNER_TEMP}"/pr-lens/assets/*.svg "${DIRECTORY}/"
  git add "${DIRECTORY}"

  if git diff --quiet --cached; then
    echo "this render is already published."
    break
  fi

  git commit --quiet -m "pr-lens: pull request #${PR_NUMBER} at ${HEAD_SHA}"

  if git push --quiet origin "${DATA_BRANCH}"; then
    break
  fi

  if [ "${ATTEMPT}" -eq 5 ]; then
    echo "::error::could not publish the diagrams: ${DATA_BRANCH} kept moving."
    exit 1
  fi
  echo "${DATA_BRANCH} moved under this run; retrying (${ATTEMPT}/5)."
  sleep "$(( ATTEMPT * 3 ))"
done

echo "assets-url=https://raw.githubusercontent.com/${GITHUB_REPOSITORY}/${DATA_BRANCH}/${DIRECTORY}" >> "${GITHUB_OUTPUT}"
