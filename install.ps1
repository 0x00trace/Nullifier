# Nullifier, one command in PowerShell:
#   irm https://raw.githubusercontent.com/iNiKKo/Nullifier/main/install.ps1 | iex
#
# Downloads the source (no git needed), builds the engine and launcher, then
# opens the launcher. Install Rust first: https://rustup.rs
$ErrorActionPreference = "Stop"
$dir = Join-Path $HOME "Nullifier"
if (-not (Test-Path (Join-Path $dir "Cargo.toml"))) {
    $zip = Join-Path $env:TEMP "nullifier.zip"
    $src = Join-Path $env:TEMP "nullifier-src"
    Write-Host "== downloading Nullifier"
    Invoke-WebRequest "https://github.com/iNiKKo/Nullifier/archive/refs/heads/main.zip" -OutFile $zip
    if (Test-Path $src) { Remove-Item $src -Recurse -Force }
    Expand-Archive $zip $src -Force
    Move-Item (Join-Path $src "Nullifier-main") $dir
}
Set-Location $dir
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "Rust is not installed. Install it from https://rustup.rs, then run this again."
    exit 1
}
Write-Host "== building the engine (about ten minutes the first time)"
cargo build --release -p nullifier
Write-Host "== building the launcher"
cargo build --release --manifest-path tools\launcher\Cargo.toml
Write-Host "Done. Double-click Nullifier.bat, or:"
Start-Process (Join-Path $dir "Nullifier.bat")
