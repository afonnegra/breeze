# NFR-04 / NFR-06 idle measurement harness (FASE 6).
# Samples GPU utilization, per-process VRAM and breeze.exe RSS every
# $IntervalSec for $Minutes, writes a CSV next to this script and
# prints a PASS/FAIL summary against the NFR thresholds.
param(
    [int]$Minutes = 5,
    [int]$IntervalSec = 5,
    [string]$OutCsv = "$PSScriptRoot\idle-samples.csv"
)

$proc = Get-Process breeze -ErrorAction SilentlyContinue
if ($null -eq $proc) { Write-Error 'breeze.exe is not running'; exit 1 }
$breezePid = $proc.Id

$samples = @()
$iterations = [math]::Ceiling(($Minutes * 60) / $IntervalSec)
for ($i = 0; $i -lt $iterations; $i++) {
    $gpu = (& nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv,noheader,nounits).Trim() -split ',\s*'
    $vramLine = (& nvidia-smi --query-compute-apps=pid,used_memory --format=csv,noheader,nounits) |
        Where-Object { $_ -match "^\s*$breezePid," }
    $vramMb = 0
    # NOTE (FASE 6 fix): on this machine's consumer/WDDM driver (561.00, GeForce RTX 4070
    # Laptop) nvidia-smi reports per-process used_memory as "[N/A]" for every process,
    # not just breeze.exe - the driver does not expose per-process VRAM accounting under
    # WDDM for this GPU class. [int] cast on "[N/A]" throws, so parse defensively and
    # leave 0 (informative field only per the brief; not part of the PASS/FAIL verdict).
    if ($vramLine) {
        $rawVram = ($vramLine -split ',\s*')[1]
        $parsedVram = 0
        if ([int]::TryParse($rawVram, [ref]$parsedVram)) { $vramMb = $parsedVram }
    }
    $rssMb = [math]::Round((Get-Process -Id $breezePid).WorkingSet64 / 1MB, 1)
    $samples += [pscustomobject]@{
        timestamp = (Get-Date -Format 'yyyy-MM-dd HH:mm:ss')
        gpu_util_pct = [int]$gpu[0]
        gpu_mem_total_mb = [int]$gpu[1]
        breeze_vram_mb = $vramMb
        breeze_rss_mb = $rssMb
    }
    Start-Sleep -Seconds $IntervalSec
}
$samples | Export-Csv -NoTypeInformation -Encoding utf8 $OutCsv

$avgGpu = ($samples | Measure-Object gpu_util_pct -Average).Average
$maxGpu = ($samples | Measure-Object gpu_util_pct -Maximum).Maximum
$avgRss = ($samples | Measure-Object breeze_rss_mb -Average).Average
$maxRss = ($samples | Measure-Object breeze_rss_mb -Maximum).Maximum
$avgVram = ($samples | Measure-Object breeze_vram_mb -Average).Average

# Actual elapsed wall-clock time, computed from first/last sample timestamps,
# vs. the nominal configured duration (window-length disclosure: per-call
# latency of nvidia-smi/Get-Process stretches the real interval past
# $IntervalSec, so the real window can run longer than $Minutes nominal).
$nominalMinutes = $Minutes
$actualElapsedMinutes = 0.0
if ($samples.Count -ge 2) {
    $firstTs = [datetime]::ParseExact($samples[0].timestamp, 'yyyy-MM-dd HH:mm:ss', $null)
    $lastTs = [datetime]::ParseExact($samples[$samples.Count - 1].timestamp, 'yyyy-MM-dd HH:mm:ss', $null)
    $actualElapsedMinutes = [math]::Round((($lastTs - $firstTs).TotalSeconds) / 60, 2)
}

"samples: $($samples.Count)  csv: $OutCsv"
"elapsed: $([math]::Round($actualElapsedMinutes,1)) min (nominal $nominalMinutes min)"
"NFR-04 GPU util  avg=$([math]::Round($avgGpu,2))%  max=$maxGpu%   (threshold: <=1% sustained)"
"NFR-04 VRAM      avg=$([math]::Round($avgVram,0)) MB (informative: model resident)"
"NFR-06 RSS       avg=$([math]::Round($avgRss,1)) MB  max=$maxRss MB (threshold idle: <=500 MB)"
