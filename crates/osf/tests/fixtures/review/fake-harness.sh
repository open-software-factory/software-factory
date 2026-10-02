#!/bin/sh
if [ -n "${OSF_FAKE_REQUIRE_ENV:-}" ]; then
    eval "osf_fake_required_value=\${${OSF_FAKE_REQUIRE_ENV}:-}"
    if [ -z "$osf_fake_required_value" ]; then
        echo "fake harness: ${OSF_FAKE_REQUIRE_ENV} is not set" 1>&2
        exit 1
    fi
fi
if [ -n "${OSF_FAKE_HARNESS_LOG:-}" ]; then
    echo run >> "$OSF_FAKE_HARNESS_LOG"
fi
if [ -n "${OSF_FAKE_ARGS_CAPTURE:-}" ]; then
    printf '%s\n' "$@" > "$OSF_FAKE_ARGS_CAPTURE"
fi
if [ -n "${OSF_FAKE_PROMPT_CAPTURE:-}" ]; then
    cat > "$OSF_FAKE_PROMPT_CAPTURE"
fi
if [ -n "${OSF_FAKE_CHANGE_CAPTURE:-}" ] && [ -n "${OSF_FAKE_PROMPT_CAPTURE:-}" ]; then
    change=$(grep -o '/[^ ]*change\.diff' "$OSF_FAKE_PROMPT_CAPTURE" | head -1)
    if [ -n "$change" ]; then
        cp "$change" "$OSF_FAKE_CHANGE_CAPTURE"
        stat -c '%a' "$(dirname "$change")" "$change" > "$OSF_FAKE_CHANGE_CAPTURE.modes"
        echo "$change" > "$OSF_FAKE_CHANGE_CAPTURE.path"
    fi
fi
if [ -n "${OSF_FAKE_CWD_CAPTURE:-}" ]; then
    {
        pwd -P
        find . -mindepth 1 \( -type f -o -type l \) | sort
    } > "$OSF_FAKE_CWD_CAPTURE"
fi
if [ -n "${OSF_FAKE_ENV_CAPTURE:-}" ]; then
    {
        echo "GH_TOKEN=${GH_TOKEN:-}"
        echo "GITHUB_TOKEN=${GITHUB_TOKEN:-}"
        echo "GH_ENTERPRISE_TOKEN=${GH_ENTERPRISE_TOKEN:-}"
        echo "OPENAI_API_KEY=${OPENAI_API_KEY:-}"
        echo "ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY:-}"
        echo "DEEPSEEK_API_KEY=${DEEPSEEK_API_KEY:-}"
        echo "CLAUDE_CODE_OAUTH_TOKEN=${CLAUDE_CODE_OAUTH_TOKEN:-}"
        echo "OPENROUTER_API_KEY=${OPENROUTER_API_KEY:-}"
    } > "$OSF_FAKE_ENV_CAPTURE"
fi
if [ -n "${OSF_FAKE_SLEEP_SECS:-}" ]; then
    sleep "$OSF_FAKE_SLEEP_SECS"
    if [ -n "${OSF_FAKE_MARKER:-}" ]; then
        touch "$OSF_FAKE_MARKER"
    fi
fi
if [ -n "${OSF_FAKE_ECHO_ENV:-}" ]; then
    eval "osf_fake_echo_value=\${${OSF_FAKE_ECHO_ENV}:-}"
    osf_fake_b64=$(printf '%s' "$osf_fake_echo_value" | base64 | tr -d '\n')
    osf_fake_hex=$(printf '%s' "$osf_fake_echo_value" | od -An -tx1 | tr -d ' \n')
    sed -e "s|@@KEY@@|$osf_fake_echo_value|g" -e "s|@@KEY_B64@@|$osf_fake_b64|g" -e "s|@@KEY_HEX@@|$osf_fake_hex|g" "$OSF_FAKE_ANSWER"
else
    cat "$OSF_FAKE_ANSWER"
fi
