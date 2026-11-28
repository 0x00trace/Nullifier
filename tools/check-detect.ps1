# check-detect.ps1: the Windows launcher's hardware detection, against stand-ins.
#
#   ./tools/check-detect.ps1
#
# Runs `launcher/nullifier-launch.ps1 -Detect` with a stand-in GPU list
# (`-GpuJson`), so the vendor, the VRAM and the profile choice can be checked
# on any Windows machine: no card, no driver, no game files. The Linux
# equivalent is tools/check-detect.sh.
#
# Why this exists: the Linux check found that NVIDIA's VRAM was taken from
# another card or reported 0, so no profile with vram_min_mb matched. The same
# class of mistake on Windows is a card that is not first in WMI's list, a
# uint32 AdapterRAM that caps, or nvidia-smi being the only truthful answer.
# Each case below fails loudly if one of those comes back.
set-strictmode -version latest
$ErrorActionPreference = "Stop"

$Here = Split-Path -Parent $MyInvocation.MyCommand.Path
$Root = Split-Path -Parent $Here
$Launcher = Join-Path $Root "launcher/nullifier-launch.ps1"

if (-not (Test-Path -LiteralPath $Launcher)) {
  Write-Host "check-detect: no launcher at $Launcher" -ForegroundColor Red
  exit 1
}

$HostExe = $null
$pwsh = Get-Command pwsh -ErrorAction SilentlyContinue
if ($pwsh) { $HostExe = $pwsh.Source }
if (-not $HostExe) { $HostExe = (Get-Command powershell.exe -ErrorAction Stop).Source }

$T = Join-Path ([IO.Path]::GetTempPath()) ("nullifier-detect-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $T | Out-Null

$script:Failed = 0

function Write-Stub([string]$Name, [string]$Json) {
  $path = Join-Path $T $Name
  Set-Content -LiteralPath $path -Value $Json -Encoding ASCII
  return $path
}

function Invoke-Detect {
  param([string]$Stub, [string[]]$Extra = @(), [hashtable]$Env = @{})

  $saved = @{}
  foreach ($k in $Env.Keys) {
    $saved[$k] = [Environment]::GetEnvironmentVariable($k, "Process")
    [Environment]::SetEnvironmentVariable($k, $Env[$k], "Process")
  }
  try {
    $argv = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $Launcher,
              "-Detect", "-GpuJson", $Stub) + $Extra
    $out = (& $HostExe @argv 2>&1 | Out-String)
    $code = $LASTEXITCODE
  } finally {
    foreach ($k in $saved.Keys) {
      [Environment]::SetEnvironmentVariable($k, $saved[$k], "Process")
    }
  }
  return [pscustomobject]@{ Out = $out; Code = $code }
}

function Assert-Detect {
  param([string]$Case, $Result, [string]$Pattern, [string]$What)

  if ($Result.Code -ne 0) {
    Write-Host "   FAIL: $Case exited $($Result.Code)" -ForegroundColor Red
    $script:Failed++
  } elseif ($Result.Out -notmatch $Pattern) {
    Write-Host "   FAIL: $Case - $What" -ForegroundColor Red
    $script:Failed++
  } else {
    Write-Host "   ok: $Case - $What"
  }
}

function Assert-NoMatch {
  param([string]$Case, $Result, [string]$Pattern, [string]$What)

  if ($Result.Out -match $Pattern) {
    Write-Host "   FAIL: $Case - $What" -ForegroundColor Red
    $script:Failed++
  } else {
    Write-Host "   ok: $Case - $What"
  }
}

# --- the stand-ins -----------------------------------------------------------
# NVIDIA second in the list, as on a laptop whose integrated card WMI lists
# first, and with the 4 GB uint32 AdapterRAM misreport only the driver's own
# answer (nvidia-smi) can correct.
$nvidiaStub = Write-Stub "nvidia.json" @'
{
  "controllers": [
    { "Name": "AMD Radeon(TM) 610M", "AdapterRAM": 536870912 },
    { "Name": "NVIDIA GeForce RTX 5080", "AdapterRAM": 4293918720 }
  ],
  "nvidia_smi_mb": 8188
}
'@

$nvidiaNoSmiStub = Write-Stub "nvidia-no-smi.json" @'
{
  "controllers": [
    { "Name": "NVIDIA GeForce RTX 3060", "AdapterRAM": 4278190080 }
  ],
  "nvidia_smi_mb": null
}
'@

$amdStub = Write-Stub "amd.json" @'
{
  "controllers": [
    { "Name": "AMD Radeon RX 9070 XT", "AdapterRAM": 0 }
  ],
  "nvidia_smi_mb": null
}
'@

$intelStub = Write-Stub "intel.json" @'
{
  "controllers": [
    { "Name": "Intel(R) Arc(TM) A770 Graphics", "AdapterRAM": 0 }
  ],
  "nvidia_smi_mb": null
}
'@

$noneStub = Write-Stub "none.json" @'
{
  "controllers": [],
  "nvidia_smi_mb": null
}
'@

try {
  Write-Host ""
  Write-Host "== NVIDIA beside an integrated AMD card, nvidia-smi says 8188 MB"
  $r = Invoke-Detect $nvidiaStub
  Assert-Detect "nvidia" $r '(?m)^GPU vendor\s+nvidia\s*$' "the discrete card decides the vendor"
  Assert-Detect "nvidia" $r '(?m)^VRAM\s+8188 MB\s*$' "VRAM comes from nvidia-smi, not AdapterRAM"
  Assert-Detect "nvidia" $r '(?m)^profile\s+windows-nvidia\b' "profile is windows-nvidia"
  Assert-Detect "nvidia" $r '(?m)^\s*VICTORY_RELAY=2\s*$' "the profile's RELAY is set"

  Write-Host ""
  Write-Host "== NVIDIA with no nvidia-smi answer"
  $r = Invoke-Detect $nvidiaNoSmiStub
  Assert-Detect "no-smi" $r '(?m)^GPU vendor\s+nvidia\s*$' "vendor still nvidia"
  Assert-Detect "no-smi" $r '(?m)^VRAM\s+4080 MB\s*$' "AdapterRAM is the fallback"
  Assert-Detect "no-smi" $r '(?m)^profile\s+windows-nvidia\b' "profile is windows-nvidia"

  Write-Host ""
  Write-Host "== AMD only"
  $r = Invoke-Detect $amdStub
  Assert-Detect "amd" $r '(?m)^GPU vendor\s+amd\s*$' "vendor is amd"
  Assert-Detect "amd" $r '(?m)^profile\s+windows-amd\b' "profile is windows-amd"
  Assert-Detect "amd" $r '(?m)^\s*VICTORY_CARD_BUFFERS_MB=1024\s*$' "1024 MB card buffers"
  Assert-NoMatch "amd" $r '(?m)^\s*VICTORY_RELAY=' "the NVIDIA relay is not set"

  Write-Host ""
  Write-Host "== Intel only"
  $r = Invoke-Detect $intelStub
  Assert-Detect "intel" $r '(?m)^GPU vendor\s+intel\s*$' "vendor is intel"
  Assert-Detect "intel" $r '(?m)^profile\s+windows-other\b' "falls back to windows-other"
  Assert-Detect "intel" $r '(?m)^\s*VICTORY_CARD_BUFFERS_MB=512\s*$' "512 MB card buffers"

  Write-Host ""
  Write-Host "== no display controller"
  $r = Invoke-Detect $noneStub
  Assert-Detect "none" $r '(?m)^GPU vendor\s+unknown\s*$' "vendor is unknown"
  Assert-Detect "none" $r '(?m)^profile\s+windows-other\b' "falls back to windows-other"

  Write-Host ""
  Write-Host "== an exported setting wins over the profile"
  $r = Invoke-Detect $nvidiaStub @() @{ VICTORY_LATE_WAIT = "1" }
  Assert-Detect "env" $r '(?m)^\s*VICTORY_LATE_WAIT=1\s*$' "VICTORY_LATE_WAIT=1 from the environment"
  Assert-Detect "env" $r '(?m)^\s*VICTORY_RELAY=2\s*$' "the rest of the profile still applies"

  Write-Host ""
  Write-Host "== -Profile forces a profile"
  $r = Invoke-Detect $nvidiaStub @("-Profile", "windows-amd")
  Assert-Detect "forced" $r '(?m)^profile\s+windows-amd\b' "windows-amd wins on an NVIDIA card"
  Assert-NoMatch "forced" $r '(?m)^\s*VICTORY_RELAY=' "the forced profile's sets, not the card's"

  Write-Host ""
  if ($script:Failed -ne 0) {
    Write-Host "check-detect: $($script:Failed) check(s) failed" -ForegroundColor Red
    exit 1
  }
  Write-Host "check-detect: all checks passed"
  exit 0
} finally {
  Remove-Item -LiteralPath $T -Recurse -Force -ErrorAction SilentlyContinue
}
