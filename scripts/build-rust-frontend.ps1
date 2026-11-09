$ErrorActionPreference = 'Stop'
# Use the same toolchain discovery and output paths as npm/Unix builds.
& node (Join-Path $PSScriptRoot 'build-rust-frontend.mjs') @args
if ($LASTEXITCODE -ne 0) { throw 'Rust frontend build failed' }
