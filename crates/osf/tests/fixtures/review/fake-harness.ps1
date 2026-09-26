if ($env:OSF_FAKE_HARNESS_LOG) {
    Add-Content -Path $env:OSF_FAKE_HARNESS_LOG -Value "run"
}
if ($env:OSF_FAKE_SLEEP_SECS) {
    Start-Sleep -Seconds ([int]$env:OSF_FAKE_SLEEP_SECS)
    if ($env:OSF_FAKE_MARKER) {
        New-Item -Path $env:OSF_FAKE_MARKER -ItemType File | Out-Null
    }
}
Get-Content -Raw $env:OSF_FAKE_ANSWER
