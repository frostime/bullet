$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
function Run-Checked([string]$Program, [string[]]$Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
}
Run-Checked 'npm.cmd' @('ci')
Run-Checked 'npm.cmd' @('run', 'check')
Run-Checked 'npm.cmd' @('test')
Run-Checked 'cargo' @('test', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--no-default-features')
Run-Checked 'npm.cmd' @('run', 'desktop:build', '--', '--', '--locked')
Write-Host 'Installer: src-tauri/target/release/bundle/nsis/'
