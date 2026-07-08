# generate-fixtures.ps1 — Genera WAVs de prueba (16 kHz mono 16-bit) con SAPI TTS.
# Ejecutar con Windows PowerShell 5.1 (powershell.exe), NO con pwsh 7:
#   System.Speech es assembly de .NET Framework (GAC) y no existe en .NET Core.
#   powershell.exe -NoProfile -ExecutionPolicy Bypass -File test-fixtures\generate-fixtures.ps1
#
# NOTA (2026-06-12): en esta máquina solo hay voces SAPI en-US (David, Zira).
# Los fixtures es-* se generan con la voz default en-US leyendo texto español:
# es una APROXIMACIÓN marcada — si whisper no los transcribe bien, el test ES
# queda condicionado a que el usuario provea un fixture real (ver plan Task 3 Step 0).
#Requires -Version 5.1
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Speech

$outDir = $PSScriptRoot
$fixtures = @(
    @{ Name = "es-corta";  Lang = "es-ES"; Text = "Hola, esto es una prueba de transcripción." },
    @{ Name = "es-media";  Lang = "es-ES"; Text = "El sistema de dictado convierte la voz en texto usando un modelo de inteligencia artificial que corre localmente en la tarjeta gráfica." },
    @{ Name = "en-corta";  Lang = "en-US"; Text = "Hello, this is a transcription test." },
    @{ Name = "en-media";  Lang = "en-US"; Text = "The dictation system converts speech to text using an artificial intelligence model running locally on the graphics card." },
    # Fixtures largos para el benchmark de latencia (Task 6) y el test de concurrencia (Task 4).
    @{ Name = "en-5s";  Lang = "en-US"; Text = "The quick brown fox jumps over the lazy dog near the quiet river bank." },
    @{ Name = "en-15s"; Lang = "en-US"; Text = "Local speech recognition has improved dramatically in recent years. Modern graphics cards can transcribe spoken language in real time without sending any audio to the cloud. This keeps private conversations on the device where they belong, safe and fully under the control of the user." },
    @{ Name = "en-30s"; Lang = "en-US"; Text = "Voice dictation is one of the oldest dreams of computing, and it has finally become practical for everyday work. A modern laptop with a dedicated graphics card can run a large neural network entirely offline, converting speech to text in well under a second. There is no subscription, no server, and no network connection involved. The audio never leaves the machine, which means private thoughts stay private. This recording exists to measure how quickly the system handles longer passages of continuous speech." }
)

foreach ($f in $fixtures) {
    $synth = New-Object System.Speech.Synthesis.SpeechSynthesizer
    $voice = $synth.GetInstalledVoices() | Where-Object { $_.VoiceInfo.Culture.Name -eq $f.Lang } | Select-Object -First 1
    if ($voice) { $synth.SelectVoice($voice.VoiceInfo.Name) }
    else { Write-Warning "No hay voz $($f.Lang) instalada; usando default (la transcripción del test puede degradarse)" }
    $format = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
    $synth.SetOutputToWaveFile((Join-Path $outDir "$($f.Name).wav"), $format)
    $synth.Speak($f.Text)
    $synth.Dispose()
    Write-Host "OK: $($f.Name).wav"
}
