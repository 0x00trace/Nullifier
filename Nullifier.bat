@echo off
rem Nullifier - opens the graphical launcher. Double-click, or run: Nullifier.bat
setlocal
cd /d "%~dp0"

set "BIN=tools\launcher\target\release\nullifier-launcher.exe"
if exist "%BIN%" goto run

where cargo >nul 2>nul
if errorlevel 1 (
  echo Rust is not installed yet.
  echo Install it from https://rustup.rs, then run this file again.
  pause
  exit /b 1
)

echo Building the launcher once; this takes a few minutes...
cargo build --release --manifest-path tools\launcher\Cargo.toml
if errorlevel 1 (
  echo The build failed. The messages above say why.
  pause
  exit /b 1
)

:run
start "" "%~dp0%BIN%"
exit /b 0
