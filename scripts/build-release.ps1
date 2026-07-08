#!/usr/bin/env pwsh
#Requires -Version 5.1
$ErrorActionPreference = "Stop"
. "$PSScriptRoot\dev-env.ps1"

$Root = Resolve-Path (Join-Path $PSScriptRoot "..")

Set-Location (Join-Path $Root "rust")
Write-Host "cargo build --release..." -ForegroundColor Cyan
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }

Set-Location $Root
Write-Host "flutter build windows (release)..." -ForegroundColor Cyan
flutter build windows --release
if ($LASTEXITCODE -ne 0) { throw "flutter build failed (exit $LASTEXITCODE)" }

# Verificación post-build: la DLL Rust debe estar en el bundle
$outDir = Join-Path $Root "build\windows\x64\runner\Release"
$requiredDlls = @("rust_lib_inputvoice.dll")
foreach ($dll in $requiredDlls) {
    if (-not (Test-Path (Join-Path $outDir $dll))) {
        throw "Missing DLL in bundle: $dll (buscada en $outDir)"
    }
}

Write-Host "Release build done. DLLs verified." -ForegroundColor Green
