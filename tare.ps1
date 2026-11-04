[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Arguments
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

$ReleaseBin = Join-Path $ScriptDir "target\release\tare.exe"
$DebugBin = Join-Path $ScriptDir "target\debug\tare.exe"

if (-not (Test-Path $ReleaseBin) -and -not (Test-Path $DebugBin)) {
    Write-Host "[tare] Compiling release binary..." -ForegroundColor Cyan
    Push-Location $ScriptDir
    try {
        cargo build --release
    } finally {
        Pop-Location
    }
}

$Bin = if (Test-Path $ReleaseBin) { $ReleaseBin } else { $DebugBin }

if ($Arguments.Length -gt 0 -and $Arguments[0] -eq "demo") {
    Write-Host "=== TideOS Eval Kit Tool 14: tare Demo ===" -ForegroundColor Green
    & $Bin table
    Write-Host "`n=== Evaluating Quality-Flat 40% Cost Reduction Candidate ===" -ForegroundColor Yellow
    & $Bin gate --spec (Join-Path $ScriptDir "packs\softfactory.toml") --baseline (Join-Path $ScriptDir "packs\fixtures\baseline_runs.jsonl") --candidate (Join-Path $ScriptDir "packs\fixtures\candidate_cost_cut_40pct.jsonl")
    Write-Host "`n=== Evaluating Cheap Call Vanity Trap Candidate ===" -ForegroundColor Yellow
    & $Bin gate --spec (Join-Path $ScriptDir "packs\softfactory.toml") --baseline (Join-Path $ScriptDir "packs\fixtures\baseline_runs.jsonl") --candidate (Join-Path $ScriptDir "packs\fixtures\candidate_cheap_call_trap.jsonl")
} else {
    & $Bin @Arguments
}
