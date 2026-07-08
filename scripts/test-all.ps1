#!/usr/bin/env pwsh
#Requires -Version 5.1
$ErrorActionPreference = "Stop"
. "$PSScriptRoot\dev-env.ps1"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")

Set-Location (Join-Path $Root "rust")
Write-Host "Running Rust tests..." -ForegroundColor Cyan
cargo test
if ($LASTEXITCODE -ne 0) { throw "cargo test failed (exit $LASTEXITCODE)" }

Set-Location $Root
Write-Host "Running Flutter unit/widget tests..." -ForegroundColor Cyan
flutter test
if ($LASTEXITCODE -ne 0) { throw "flutter test failed (exit $LASTEXITCODE)" }

Write-Host "All tests passed." -ForegroundColor Green
