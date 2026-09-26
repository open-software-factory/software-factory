#!/bin/sh
if [ -n "${OSF_FAKE_HARNESS_LOG:-}" ]; then
    echo run >> "$OSF_FAKE_HARNESS_LOG"
fi
if [ -n "${OSF_FAKE_SLEEP_SECS:-}" ]; then
    sleep "$OSF_FAKE_SLEEP_SECS"
    if [ -n "${OSF_FAKE_MARKER:-}" ]; then
        touch "$OSF_FAKE_MARKER"
    fi
fi
cat "$OSF_FAKE_ANSWER"
