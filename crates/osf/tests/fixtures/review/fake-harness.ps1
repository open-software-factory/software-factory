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
        "GH_ENTERPRISE_TOKEN=$($env:GH_ENTERPRISE_TOKEN)"
    )
    Set-Content -Path $env:OSF_FAKE_ENV_CAPTURE -Value ($lines -join "`n")
}
if ($env:OSF_FAKE_SLEEP_SECS) {
    Start-Sleep -Seconds ([int]$env:OSF_FAKE_SLEEP_SECS)
    if ($env:OSF_FAKE_MARKER) {
        New-Item -Path $env:OSF_FAKE_MARKER -ItemType File | Out-Null
    }
}
Get-Content -Raw $env:OSF_FAKE_ANSWER
