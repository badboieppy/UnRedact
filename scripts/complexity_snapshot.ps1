param(
    [string]$OutPath = "benchmark/complexity_snapshot.json"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-RepoRoot {
    $scriptDir = $PSScriptRoot
    return (Resolve-Path (Join-Path $scriptDir "..")).Path
}

function Read-Lines([string]$path) {
    return Get-Content -Path $path -Encoding UTF8
}

function Count-Matches([string[]]$lines, [string]$pattern) {
    return (($lines | Where-Object { $_ -match $pattern }) | Measure-Object).Count
}

function Collect-Files([string]$root, [string[]]$dirs, [string[]]$extensions) {
    $all = @()
    foreach ($dir in $dirs) {
        $abs = Join-Path $root $dir
        if (-not (Test-Path $abs)) { continue }
        foreach ($ext in $extensions) {
            $all += Get-ChildItem -Path $abs -Recurse -File -Filter "*.$ext" | Select-Object -ExpandProperty FullName
        }
    }
    return $all | Sort-Object -Unique
}

function Top-ByCount($map, [int]$top = 12) {
    return $map.GetEnumerator() | Sort-Object -Property Value -Descending | Select-Object -First $top | ForEach-Object {
        [ordered]@{
            file = $_.Key
            count = $_.Value
        }
    }
}

$repoRoot = Get-RepoRoot
Push-Location $repoRoot
try {
    $cargoTomlPath = Join-Path $repoRoot "Cargo.toml"
    $cargoToml = Read-Lines $cargoTomlPath

    $inFeatures = $false
    $featureNames = @()
    foreach ($line in $cargoToml) {
        $trimmed = $line.Trim()
        if ($trimmed -eq "[features]") {
            $inFeatures = $true
            continue
        }
        if ($inFeatures -and $trimmed.StartsWith("[")) {
            break
        }
        if (-not $inFeatures) { continue }
        if ($trimmed -match "^([A-Za-z0-9_-]+)\s*=") {
            $featureNames += $Matches[1]
        }
    }

    $rustFiles = Collect-Files $repoRoot @("src", "tests") @("rs")
    $webFiles = Collect-Files $repoRoot @("web") @("js", "mjs", "html")

    $cfgMentions = 0
    $publicExports = 0
    $conditionByFile = @{}
    $runtimeFlagMentions = 0
    $envMentions = 0

    foreach ($file in $rustFiles) {
        $lines = Read-Lines $file
        $cfgMentions += Count-Matches $lines "^\s*#\[\s*cfg"
        $publicExports += Count-Matches $lines "^\s*pub(\(|\s)"
        $runtimeFlagMentions += Count-Matches $lines "--[a-zA-Z0-9][a-zA-Z0-9_-]*"
        $envMentions += Count-Matches $lines "UNREDACT_[A-Z0-9_]+"

        $ifCount = Count-Matches $lines "^\s*if\s+|^\s*else if\s+"
        $matchCount = Count-Matches $lines "^\s*match\s+"
        $whileCount = Count-Matches $lines "^\s*while\s+"
        $total = $ifCount + $matchCount + $whileCount
        if ($total -gt 0) {
            $rel = Resolve-Path $file -Relative
            $conditionByFile[$rel] = $total
        }
    }

    foreach ($file in $webFiles) {
        $lines = Read-Lines $file
        $runtimeFlagMentions += Count-Matches $lines "--[a-zA-Z0-9][a-zA-Z0-9_-]*"
        $envMentions += Count-Matches $lines "UNREDACT_[A-Z0-9_]+"

        $ifCount = Count-Matches $lines "\bif\s*\("
        $switchCount = Count-Matches $lines "\bswitch\s*\("
        $ternaryCount = Count-Matches $lines "\?.*:"
        $total = $ifCount + $switchCount + $ternaryCount
        if ($total -gt 0) {
            $rel = Resolve-Path $file -Relative
            if ($conditionByFile.ContainsKey($rel)) {
                $conditionByFile[$rel] += $total
            } else {
                $conditionByFile[$rel] = $total
            }
        }
    }

    $conditionTotal = ($conditionByFile.Values | Measure-Object -Sum).Sum
    if (-not $conditionTotal) { $conditionTotal = 0 }

    $snapshot = [ordered]@{
        generated_utc = (Get-Date).ToUniversalTime().ToString("o")
        summary = [ordered]@{
            compile_features = ($featureNames | Sort-Object -Unique).Count
            cfg_mentions = $cfgMentions
            runtime_flag_mentions = $runtimeFlagMentions
            env_var_mentions = $envMentions
            public_export_lines = $publicExports
            condition_lines = $conditionTotal
        }
        features = ($featureNames | Sort-Object -Unique)
        top_condition_files = @(Top-ByCount $conditionByFile 15)
    }

    $outFull = Join-Path $repoRoot $OutPath
    $outDir = Split-Path -Parent $outFull
    if ($outDir -and -not (Test-Path $outDir)) {
        New-Item -ItemType Directory -Path $outDir | Out-Null
    }

    $json = $snapshot | ConvertTo-Json -Depth 6
    Set-Content -Path $outFull -Value $json -Encoding UTF8
    Write-Output "wrote $outFull"
}
finally {
    Pop-Location
}
