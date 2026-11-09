$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if ($cargo) { $cargoPath = $cargo.Source } else { $cargoPath = Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe' }
    & $cargoPath build -p cellhub-frontend --release --target wasm32-unknown-unknown
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
    $bindgen = Join-Path (Get-Location) '.tools/wasm-bindgen/bin/wasm-bindgen.exe'
    & $bindgen target/wasm32-unknown-unknown/release/cellhub_frontend.wasm --target web --out-dir frontend/rust-wasm --out-name cellhub_frontend
    if ($LASTEXITCODE -ne 0) { throw 'wasm-bindgen failed' }
} finally { Pop-Location }
