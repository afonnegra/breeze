# measure-idle-perprocess.ps1
#
# Re-measures NFR-04 (idle GPU) with PER-PROCESS attribution, using Windows PDH
# counters instead of nvidia-smi's GPU-wide aggregate. This is a correction of
# Task 4's original measurement, which used nvidia-smi (GPU-wide only, no
# per-process compute utilization on this WDDM consumer driver) and could not
# confirm whether breeze.exe or other apps holding GPU contexts (Spotify, Teams,
# Edge, LM Studio, etc.) were responsible for the measured 14.17% avg GPU util.
#
# Counter sets used (verified present on this machine before writing this
# script, via `Get-Counter -ListSet 'GPU Engine'` / 'GPU Process Memory'):
#   \GPU Engine(*)\Utilization Percentage   -> per-engine-instance, sum over all
#                                              engine instances for a given PID
#                                              is the Task-Manager-equivalent
#                                              per-process GPU % figure.
#   \GPU Process Memory(*)\Dedicated Usage  -> per-process VRAM (bytes), sum over
#                                              instances for the PID.
#
# Instance naming confirmed live on this machine (no adaptation needed):
#   pid_<PID>_luid_0x..._0x..._phys_0_eng_<N>_engtype_<3d|copy|videodecode|...>
# Wildcarding as "pid_<PID>_*" matches all engine instances for that PID, which
# is exactly what the brief specified. No naming-format adaptation was required.
#
# NOTE (window-length disclosure): the nominal interval below is $IntervalSeconds
# (default 5s), but each Get-Counter call for the PDH counters has its own
# latency on top of the Start-Sleep, so the REAL per-sample interval tends to
# run ~7-8s on this machine, not 5s. The printed summary below reports the
# actual elapsed wall-clock time computed from the first/last sample
# timestamps, alongside the nominal configured duration, so this self-discloses
# on every run instead of silently understating the window.
#
# Usage: .\measure-idle-perprocess.ps1 -Minutes 4 -BreezeProcessName breeze
#
# Output: CSV at .\perprocess-idle-samples.csv (mirrored into
# a CSV path of your choice by the caller) plus a
# printed avg/max summary for breeze GPU %, breeze VRAM MB, GPU-wide %, RSS MB.

param(
    [int]$Minutes = 4,
    [int]$IntervalSeconds = 5,
    [string]$BreezeProcessName = "breeze"
)

$ErrorActionPreference = "Stop"

$totalSamples = [int]([math]::Ceiling(($Minutes * 60) / $IntervalSeconds))
$csvPath = Join-Path $PSScriptRoot "perprocess-idle-samples.csv"

$proc = Get-Process -Name $BreezeProcessName -ErrorAction SilentlyContinue
if (-not $proc) {
    throw "No process named '$BreezeProcessName' is running. Launch breeze.exe first."
}
$breezePid = $proc.Id
Write-Output "Measuring PID $breezePid ('$BreezeProcessName') for $Minutes min ($totalSamples samples every ${IntervalSeconds}s)."

$rows = @()

for ($i = 0; $i -lt $totalSamples; $i++) {
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"

    # --- Per-process GPU utilization (PDH GPU Engine, summed over all engine
    #     instances for this PID: 3D, copy, video decode, etc.) ---
    $breezeGpuPct = 0.0
    try {
        $engineSamples = (Get-Counter "\GPU Engine(pid_${breezePid}_*)\Utilization Percentage" -ErrorAction Stop).CounterSamples
        if ($engineSamples) {
            $breezeGpuPct = ($engineSamples | Measure-Object -Property CookedValue -Sum).Sum
        }
    } catch {
        # No instances for this PID at this sample (e.g. GPU engine handle not
        # currently open) -> treat as 0, not an error.
        $breezeGpuPct = 0.0
    }

    # --- Per-process VRAM (PDH GPU Process Memory, Dedicated Usage, bytes) ---
    $breezeVramMb = 0.0
    try {
        $memSamples = (Get-Counter "\GPU Process Memory(pid_${breezePid}_*)\Dedicated Usage" -ErrorAction Stop).CounterSamples
        if ($memSamples) {
            $sumBytes = ($memSamples | Measure-Object -Property CookedValue -Sum).Sum
            $breezeVramMb = [math]::Round($sumBytes / 1MB, 2)
        }
    } catch {
        $breezeVramMb = 0.0
    }

    # --- GPU-wide utilization from nvidia-smi, kept for context/comparison
    #     against the original Task 4 measurement ---
    $gpuWidePct = 0
    try {
        $smiOut = & nvidia-smi --query-gpu=utilization.gpu --format=csv,noheader,nounits 2>$null
        if ($smiOut) {
            [int]::TryParse(($smiOut | Select-Object -First 1).Trim(), [ref]$gpuWidePct) | Out-Null
        }
    } catch {
        $gpuWidePct = 0
    }

    # --- breeze RSS (working set) ---
    $rssMb = 0.0
    try {
        $p = Get-Process -Id $breezePid -ErrorAction Stop
        $rssMb = [math]::Round($p.WorkingSet64 / 1MB, 2)
    } catch {
        # Process exited mid-measurement.
        $rssMb = 0.0
    }

    $row = [PSCustomObject]@{
        timestamp        = $timestamp
        breeze_gpu_pct   = [math]::Round($breezeGpuPct, 2)
        breeze_vram_mb   = $breezeVramMb
        gpu_wide_pct     = $gpuWidePct
        breeze_rss_mb    = $rssMb
    }
    $rows += $row

    Write-Output ("[{0}] breeze_gpu={1}%  breeze_vram={2}MB  gpu_wide={3}%  rss={4}MB" -f `
        $timestamp, $row.breeze_gpu_pct, $row.breeze_vram_mb, $row.gpu_wide_pct, $row.breeze_rss_mb)

    if ($i -lt ($totalSamples - 1)) {
        Start-Sleep -Seconds $IntervalSeconds
    }
}

$rows | Export-Csv -Path $csvPath -NoTypeInformation -Encoding UTF8

$avgGpu = ($rows | Measure-Object -Property breeze_gpu_pct -Average).Average
$maxGpu = ($rows | Measure-Object -Property breeze_gpu_pct -Maximum).Maximum
$avgVram = ($rows | Measure-Object -Property breeze_vram_mb -Average).Average
$maxVram = ($rows | Measure-Object -Property breeze_vram_mb -Maximum).Maximum
$avgGpuWide = ($rows | Measure-Object -Property gpu_wide_pct -Average).Average
$maxGpuWide = ($rows | Measure-Object -Property gpu_wide_pct -Maximum).Maximum
$avgRss = ($rows | Measure-Object -Property breeze_rss_mb -Average).Average
$maxRss = ($rows | Measure-Object -Property breeze_rss_mb -Maximum).Maximum

# --- Actual elapsed wall-clock time, computed from first/last sample
#     timestamps, vs. the nominal configured duration (window-length
#     disclosure: Get-Counter's own per-call latency stretches the real
#     interval past $IntervalSeconds, so the real window runs longer than
#     $Minutes nominal). ---
$nominalMinutes = $Minutes
$actualElapsedMinutes = 0.0
if ($rows.Count -ge 2) {
    $firstTs = [datetime]::ParseExact($rows[0].timestamp, "yyyy-MM-dd HH:mm:ss", $null)
    $lastTs = [datetime]::ParseExact($rows[$rows.Count - 1].timestamp, "yyyy-MM-dd HH:mm:ss", $null)
    $actualElapsedMinutes = [math]::Round((($lastTs - $firstTs).TotalSeconds) / 60, 2)
}

Write-Output "----- SUMMARY -----"
Write-Output ("samples: {0}  csv: {1}" -f $rows.Count, $csvPath)
Write-Output ("elapsed: {0:N1} min (nominal {1} min)" -f $actualElapsedMinutes, $nominalMinutes)
Write-Output ("breeze GPU %   avg={0:N2}  max={1:N2}" -f $avgGpu, $maxGpu)
Write-Output ("breeze VRAM MB avg={0:N2}  max={1:N2}" -f $avgVram, $maxVram)
Write-Output ("GPU-wide %     avg={0:N2}  max={1:N2}" -f $avgGpuWide, $maxGpuWide)
Write-Output ("breeze RSS MB  avg={0:N2}  max={1:N2}" -f $avgRss, $maxRss)
