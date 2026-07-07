param(
    [switch]$Serve,
    [int]$Port = 8080
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $scriptDir

function Resolve-CommandPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$PrimaryPath,

        [Parameter(Mandatory = $true)]
        [string]$CommandName
    )

    if (Test-Path $PrimaryPath) {
        return $PrimaryPath
    }

    $command = Get-Command $CommandName -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    return $null
}

$rustupPath = Resolve-CommandPath -PrimaryPath (Join-Path $env:USERPROFILE '.cargo/bin/rustup.exe') -CommandName 'rustup.exe'
$rustcPath = Resolve-CommandPath -PrimaryPath (Join-Path $env:USERPROFILE '.cargo/bin/rustc.exe') -CommandName 'rustc.exe'

if (-not $rustcPath) {
    throw 'rustc no encontrado. Instala Rustup primero.'
}

if ($rustupPath) {
    & $rustupPath target add wasm32-unknown-unknown 2>$null
}

& $rustcPath --edition 2021 `
    --target wasm32-unknown-unknown `
    -O -C panic=abort -C lto=fat `
    --crate-name raydrone_core --crate-type=lib `
    ..\core\src\lib.rs -o libraydrone_core.rlib

& $rustcPath --edition 2021 `
    --target wasm32-unknown-unknown `
    -O -C panic=abort -C lto=fat `
    --extern raydrone_core=libraydrone_core.rlib `
    --crate-type=cdylib `
    raydrone.rs -o raydrone.wasm

$size = (Get-Item raydrone.wasm).Length
Write-Output "OK raydrone.wasm generado ($size bytes)"

if ($Serve) {
    $python = Get-Command python.exe -ErrorAction SilentlyContinue
    if (-not $python) {
        $python = Get-Command py.exe -ErrorAction SilentlyContinue
    }
    if (-not $python) {
        throw 'Python no encontrado. Instala Python o habilita el launcher py.exe.'
    }

    Write-Output "Sirviendo http://localhost:$Port/"
    & $python.Source -m http.server $Port
}