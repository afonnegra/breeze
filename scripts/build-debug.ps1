#!/usr/bin/env pwsh
#Requires -Version 5.1
$ErrorActionPreference = "Stop"
. "$PSScriptRoot\dev-env.ps1"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")

Set-Location (Join-Path $Root "rust")
Write-Host "cargo build (debug)..." -ForegroundColor Cyan
cargo build
if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }

Set-Location $Root
Write-Host "flutter build windows (debug)..." -ForegroundColor Cyan
flutter build windows --debug
if ($LASTEXITCODE -ne 0) { throw "flutter build failed (exit $LASTEXITCODE)" }

Write-Host "Debug build done." -ForegroundColor Green
