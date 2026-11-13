# UNVERIFIED: escrito sem acesso a um Windows. Empacota um .zip portátil em dist\ (sbf.exe + assets).
# Dependências externas (pdftotext, qpdf, rclone) precisam estar no PATH; veja docs\desktop\instalacao.md.
# Uso (PowerShell, na raiz do repositório): .\scripts\desktop\empacotar-windows.ps1 [-SemBuild]
param([switch]$SemBuild)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..")

$meta = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
$versao = ($meta.packages | Where-Object { $_.name -eq "sbf" }).version
$alvo = $meta.target_directory
$nome = "seu-bolso-feliz-$versao-windows-x86_64"

if (-not $SemBuild) { cargo build --release -p sbf; if ($LASTEXITCODE) { throw "cargo build falhou" } }
$bin = Join-Path $alvo "release\sbf.exe"
if (-not (Test-Path $bin)) { throw "binário não encontrado: $bin" }

$stage = "dist\$nome"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item $bin "$stage\sbf.exe"
foreach ($c in "crates\sbf-janela\assets", "crates\sbf\assets") {
  if (Test-Path $c) { Copy-Item -Recurse $c "$stage\assets"; break }
}
foreach ($f in "LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "THIRD-PARTY-LICENSES") {
  if (Test-Path $f) { Copy-Item $f "$stage\" }
}
Copy-Item docs\desktop\instalacao.md "$stage\INSTALACAO.md"

$zip = "dist\$nome.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path $stage -DestinationPath $zip
Remove-Item -Recurse -Force $stage
Get-FileHash $zip -Algorithm SHA256 | ForEach-Object { "$($_.Hash.ToLower())  $nome.zip" } | Set-Content dist\SHA256SUMS-windows
Write-Host "gerado: $zip"
