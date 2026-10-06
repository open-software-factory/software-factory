if ($env:OSF_FAKE_REQUIRE_ENV) {
    $requiredValue = [Environment]::GetEnvironmentVariable($env:OSF_FAKE_REQUIRE_ENV)
    if ([string]::IsNullOrEmpty($requiredValue)) {
        [Console]::Error.WriteLine("fake harness: $($env:OSF_FAKE_REQUIRE_ENV) is not set")
        exit 1
    }
}
if ($env:OSF_FAKE_HARNESS_LOG) {
    Add-Content -Path $env:OSF_FAKE_HARNESS_LOG -Value "run"
}
if ($env:OSF_FAKE_ENV_CAPTURE) {
    $lines = @(
        "GH_TOKEN=$($env:GH_TOKEN)",
        "GITHUB_TOKEN=$($env:GITHUB_TOKEN)",
        "GH_ENTERPRISE_TOKEN=$($env:GH_ENTERPRISE_TOKEN)",
        "OPENAI_API_KEY=$($env:OPENAI_API_KEY)",
        "ANTHROPIC_API_KEY=$($env:ANTHROPIC_API_KEY)",
        "DEEPSEEK_API_KEY=$($env:DEEPSEEK_API_KEY)",
        "CLAUDE_CODE_OAUTH_TOKEN=$($env:CLAUDE_CODE_OAUTH_TOKEN)",
        "OPENROUTER_API_KEY=$($env:OPENROUTER_API_KEY)"
    )
    [IO.File]::WriteAllText($env:OSF_FAKE_ENV_CAPTURE, (($lines -join "`n") + "`n"))
}
if ($env:OSF_FAKE_SLEEP_SECS) {
    Start-Sleep -Seconds ([int]$env:OSF_FAKE_SLEEP_SECS)
    if ($env:OSF_FAKE_MARKER) {
        New-Item -Path $env:OSF_FAKE_MARKER -ItemType File | Out-Null
    }
}
Get-Content -Raw $env:OSF_FAKE_ANSWER
