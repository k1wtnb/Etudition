# F0 のファームをビルドする（STM32CubeIDE に同梱の GCC、CMake、Ninja を使う）。
# 使い方：powershell -ExecutionPolicy Bypass -File build.ps1 [-Preset Debug|Release]
param([string]$Preset = "Release")
$ErrorActionPreference = "Stop"

$plugins = Get-ChildItem "C:\ST\STM32CubeIDE*\STM32CubeIDE\plugins" -Directory -ErrorAction SilentlyContinue |
    Select-Object -First 1
if (-not $plugins) { throw "STM32CubeIDE が C:\ST の下に見つからない" }
foreach ($tool in "gnu-tools-for-stm32", "cmake.win32", "ninja.win32") {
    $dir = Get-ChildItem $plugins.FullName -Directory -Filter "*$tool*" |
        Sort-Object Name -Descending | Select-Object -First 1
    if (-not $dir) { throw "STM32CubeIDE の中に $tool が見つからない" }
    $env:PATH = (Join-Path $dir.FullName "tools\bin") + ";" + $env:PATH
}

Set-Location $PSScriptRoot
cmake --preset $Preset
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cmake --build --preset $Preset
exit $LASTEXITCODE
