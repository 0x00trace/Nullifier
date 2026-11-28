# nullifier-launch.ps1 - Windows launcher. Reads the same launcher/profiles.json as
# the bash launcher, so both platforms choose settings by the same rules.
#
#   .\launcher\nullifier-launch.ps1 -Detect
#   .\launcher\nullifier-launch.ps1 -List
#   .\launcher\nullifier-launch.ps1 -Game "D:\games\title" [-Safe] [-Diagnostics]
#   .\launcher\nullifier-launch.ps1 -Game DIR -Profile windows-amd
#
# Requirement: a D3D12 GPU with shader model 6.6 and mesh shaders.

[CmdletBinding()]
param(
  [string]$Game = $env:VICTORY_GAME_DIR,
  [string]$Bin  = $env:VICTORY_BIN,
  [string]$TitleExe = $(if ($env:VICTORY_TITLE_EXE) { $env:VICTORY_TITLE_EXE } else { "game_scarlett_master.exe" }),
  [string]$Profile = "",
  [switch]$Detect,
  [switch]$List,
  [switch]$Safe,
  [switch]$Diagnostics,
  [Parameter(ValueFromRemainingArguments = $true)] $Passthru
)

$ErrorActionPreference = "Stop"
$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Split-Path -Parent $Here
$MatrixPath = if ($env:VICTORY_PROFILES) { $env:VICTORY_PROFILES } else { Join-Path $Here "profiles.json" }
$Matrix = Get-Content $MatrixPath -Raw | ConvertFrom-Json

function Get-NvidiaVram {
  $candidates = @(
    (Join-Path $env:SystemRoot "System32\nvidia-smi.exe"),
    (Join-Path $env:ProgramFiles "NVIDIA Corporation\NVSMI\nvidia-smi.exe"),
    "nvidia-smi"
  )
  foreach ($smi in $candidates) {
    try {
      $line = & $smi --query-gpu=memory.total --format=csv,noheader,nounits 2>$null |
        Select-Object -First 1
      if ($line -match "^\s*(\d+)") { return [int64]$Matches[1] }
    } catch { }
  }
  return 0
}

function Get-GpuInfo {
  $vendor = "unknown"
  $name = "unknown"
  $vram = 0

  # Prefer a discrete card: a laptop can report Intel first, and its AdapterRAM
  # must not decide the vendor for the matched profile.
  $all = @(Get-CimInstance Win32_VideoController)
  $ctrl = $all | Where-Object { $_.Name -match "NVIDIA" } | Select-Object -First 1
  if (-not $ctrl) {
    $ctrl = $all | Where-Object { $_.Name -match "AMD|Radeon|ATI" } | Select-Object -First 1
  }
  if (-not $ctrl) {
    $ctrl = $all | Where-Object { $_.Name -match "Intel" } | Select-Object -First 1
  }
  if (-not $ctrl) {
    $ctrl = $all |
      Sort-Object -Property @{Expression = { if ($_.AdapterRAM) { $_.AdapterRAM } else { 0 } }} -Descending |
      Select-Object -First 1
  }
  if ($ctrl) {
    $name = $ctrl.Name
    if     ($name -match "NVIDIA")         { $vendor = "nvidia" }
    elseif ($name -match "AMD|Radeon|ATI") { $vendor = "amd" }
    elseif ($name -match "Intel")          { $vendor = "intel" }
  }

  # The driver's own answer is the only one that is right for every NVIDIA
  # card; AdapterRAM is a uint32 and caps or misreports.
  if ($vendor -eq "nvidia") { $vram = Get-NvidiaVram }

  # The registry has the real size for other drivers.
  if ($vram -eq 0) {
    try {
      $class = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}"
      Get-ChildItem $class -ErrorAction Stop | ForEach-Object {
        $q = (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue)."HardwareInformation.qwMemorySize"
        if ($q) {
          $mb = [int64]([uint64]$q / 1MB)
          if ($mb -gt $vram -and $mb -lt 262144) { $vram = $mb }
        }
      }
    } catch { }
  }
  if ($vram -eq 0) {
    try {
      Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Control\Video" -Recurse -ErrorAction SilentlyContinue |
        ForEach-Object {
          $q = (Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue)."HardwareInformation.qwMemorySize"
          if ($q) {
            $mb = [int64]([uint64]$q / 1MB)
            if ($mb -gt $vram -and $mb -lt 262144) { $vram = $mb }
          }
        }
    } catch { }
  }
  if ($vram -eq 0 -and $ctrl -and $ctrl.AdapterRAM) {
    $mb = [int64]($ctrl.AdapterRAM / 1MB)
    # 512 MB and other small values are a known WMI misreport on large cards.
    if ($mb -ge 1024) { $vram = $mb }
  }

  [pscustomobject]@{ Vendor = $vendor; Name = $name; VramMB = $vram }
}

function Test-Applies($p, $os, $vendor, $vram) {
  $m = $p.match
  if ($m.os   -and $m.os   -ne "*" -and $m.os   -ne $os)     { return $false }
  if ($m.vendor -and $m.vendor -ne "*" -and $m.vendor -ne $vendor) { return $false }
  if ($m.vram_min_mb -and $vram -lt [int]$m.vram_min_mb)     { return $false }
  if ($m.vram_max_mb -and $vram -gt [int]$m.vram_max_mb)     { return $false }
  return $true
}

$gpu = Get-GpuInfo

if ($List) {
  $Matrix.profiles | ForEach-Object {
    "{0,-20} os={1,-8} vendor={2,-8} confidence={3,-12} {4}" -f `
      $_.name, $_.match.os, $_.match.vendor, $_.confidence, $_.note
  }
  exit 0
}

$chosen = $null
if ($Profile) { $chosen = $Matrix.profiles | Where-Object { $_.name -eq $Profile } | Select-Object -First 1 }
if (-not $chosen) { $chosen = $Matrix.profiles | Where-Object { Test-Applies $_ "windows" $gpu.Vendor $gpu.VramMB } | Select-Object -First 1 }
if (-not $chosen) { Write-Error "no profile applies to vendor=$($gpu.Vendor) vram=$($gpu.VramMB)"; exit 3 }

$sets = @{}
foreach ($prop in $Matrix.defaults.PSObject.Properties) { $sets[$prop.Name] = $prop.Value }
foreach ($prop in $chosen.sets.PSObject.Properties)     { $sets[$prop.Name] = $prop.Value }

function Merge-Override($name) {
  $o = $Matrix.overrides | Where-Object { $_.name -eq $name } | Select-Object -First 1
  if ($o) { foreach ($prop in $o.sets.PSObject.Properties) { $sets[$prop.Name] = $prop.Value } }
}
if ($Safe)        { Merge-Override "safe" }
if ($Diagnostics) { Merge-Override "diagnostics" }

# Environment wins over the profile, so a contributor can A/B one switch.
foreach ($k in @($sets.Keys)) {
  if ([Environment]::GetEnvironmentVariable($k)) { $sets[$k] = [Environment]::GetEnvironmentVariable($k) }
  else { [Environment]::SetEnvironmentVariable($k, [string]$sets[$k], "Process") }
}

if ($Detect) {
  Write-Host ("OS                       windows {0}" -f [Environment]::OSVersion.Version)
  Write-Host ("GPU                      {0}" -f $gpu.Name)
  Write-Host ("GPU vendor               {0}" -f $gpu.Vendor)
  Write-Host ("VRAM                     {0} MB" -f $gpu.VramMB)
  Write-Host ("profile                  {0} ({1})" -f $chosen.name, $chosen.confidence)
  Write-Host ""
  $sets.GetEnumerator() | Sort-Object Name | ForEach-Object { Write-Host ("  {0}={1}" -f $_.Key, $_.Value) }
  Write-Host ""
  Write-Host ("why                      {0}" -f $chosen.note)
  exit 0
}

if (-not $Game)        { Write-Error "need -Game DIR (or VICTORY_GAME_DIR)"; exit 2 }
if (-not (Test-Path $Game)) { Write-Error "not a directory: $Game"; exit 2 }
if (-not $Bin) {
  $native = Join-Path $Root "target\release\nullifier.exe"
  $diag   = Join-Path $Root "target\x86_64-pc-windows-msvc\diag\nullifier.exe"
  $diagHost = Join-Path $Root "target\diag\nullifier.exe"
  if (Test-Path $native) { $Bin = $native }
  elseif (Test-Path $diagHost) { $Bin = $diagHost }
  else { $Bin = $diag }
}
if (-not (Test-Path $Bin)) { Write-Error "no nullifier.exe at $Bin; build it first (cargo build --release -p nullifier)"; exit 2 }

$exe = Join-Path $Game $TitleExe
if (-not (Test-Path $exe)) { Write-Error "title exe not found: $exe"; exit 2 }

Write-Host ("profile {0}: running {1}" -f $chosen.name, $exe)
$argv = @("run", $exe, "--execute") + $Passthru
& $Bin @argv
exit $LASTEXITCODE
