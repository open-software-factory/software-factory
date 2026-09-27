#!/bin/sh
if [ -n "${OSF_FAKE_HARNESS_LOG:-}" ]; then
    echo run >> "$OSF_FAKE_HARNESS_LOG"
fi
if [ -n "${OSF_FAKE_ENV_CAPTURE:-}" ]; then
    {
        echo "GH_TOKEN=${GH_TOKEN:-}"
        echo "GITHUB_TOKEN=${GITHUB_TOKEN:-}"
        echo "GH_ENTERPRISE_TOKEN=${GH_ENTERPRISE_TOKEN:-}"
    } > "$OSF_FAKE_ENV_CAPTURE"
fi
if [ -n "${OSF_FAKE_SLEEP_SECS:-}" ]; then
    sleep "$OSF_FAKE_SLEEP_SECS"
    if [ -n "${OSF_FAKE_MARKER:-}" ]; then
        touch "$OSF_FAKE_MARKER"
    fi
fi
cat "$OSF_FAKE_ANSWER"
