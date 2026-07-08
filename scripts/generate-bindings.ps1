#!/usr/bin/env pwsh
#Requires -Version 5.1
$ErrorActionPreference = "Stop"
. "$PSScriptRoot\dev-env.ps1"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")
Set-Location $Root

Write-Host "Generating flutter_rust_bridge bindings..." -ForegroundColor Cyan
flutter_rust_bridge_codegen generate
if ($LASTEXITCODE -ne 0) { throw "FRB codegen failed (exit $LASTEXITCODE)" }
Write-Host "Bindings generated." -ForegroundColor Green
