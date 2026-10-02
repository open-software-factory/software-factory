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
cat "$OSF_FAKE_ANSWER"
