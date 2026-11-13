#!/usr/bin/env bash
# UNVERIFIED: escrito sem acesso a um Mac. Empacota "Seu Bolso Feliz.app" em dist/ (.tar.gz e .dmg).
# Sem assinatura nem notarização (o Gatekeeper pede "Abrir assim mesmo").
# Dependências externas (pdftotext, qpdf, rclone): instalar com `brew install poppler qpdf rclone`.
# Uso, em um Mac: scripts/desktop/empacotar-macos.sh [--sem-build]
set -euo pipefail
source "$(dirname "$0")/_comum.sh"

ARQ=$(uname -m)
NOME="seu-bolso-feliz-$VERSAO-macos-$ARQ"
[[ "${1:-}" == "--sem-build" ]] || cargo build --release -p sbf
BIN="$PASTA_ALVO/release/sbf"
[[ -x "$BIN" ]] || { echo "binário não encontrado: $BIN" >&2; exit 1; }
[[ -f THIRD-PARTY-LICENSES ]] || scripts/desktop/gerar-licencas.sh || true

STAGE="dist/$NOME"
APP="$STAGE/Seu Bolso Feliz.app"
rm -rf "$STAGE"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
install -m755 "$BIN" "$APP/Contents/MacOS/sbf"
[[ -n "$PASTA_ASSETS" ]] && cp -r "$PASTA_ASSETS" "$APP/Contents/Resources/assets"
copiar_licencas "$APP/Contents/Resources"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Seu Bolso Feliz</string>
  <key>CFBundleDisplayName</key><string>Seu Bolso Feliz</string>
  <key>CFBundleIdentifier</key><string>app.seubolsofeliz.desktop</string>
  <key>CFBundleExecutable</key><string>sbf</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSAO</string>
  <key>CFBundleVersion</key><string>$VERSAO</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.finance</string>
</dict>
</plist>
PLIST
cp docs/desktop/instalacao.md "$STAGE/INSTALACAO.md"
tar -C dist -czf "dist/$NOME.tar.gz" "$NOME"
echo "gerado: dist/$NOME.tar.gz"
if command -v hdiutil >/dev/null; then
  hdiutil create -volname "Seu Bolso Feliz" -srcfolder "$STAGE" -ov -format UDZO "dist/$NOME.dmg" >/dev/null
  echo "gerado: dist/$NOME.dmg"
fi
rm -rf "$STAGE"
(cd dist && shasum -a 256 ./*.tar.gz ./*.dmg > SHA256SUMS-macos 2>/dev/null || true)
