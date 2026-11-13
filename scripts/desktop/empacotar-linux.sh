#!/usr/bin/env bash
# Empacota o Seu Bolso Feliz desktop para Linux em dist/: .tar.gz portátil e .deb.
# Uso: scripts/desktop/empacotar-linux.sh [--sem-build]   (o .deb requer dpkg-deb)
set -euo pipefail
source "$(dirname "$0")/_comum.sh"

ARQ=$(uname -m)
case "$ARQ" in x86_64) ARQ_DEB=amd64 ;; aarch64) ARQ_DEB=arm64 ;; *) ARQ_DEB=$ARQ ;; esac
NOME="seu-bolso-feliz-$VERSAO-linux-$ARQ"

[[ "${1:-}" == "--sem-build" ]] || cargo build --release -p sbf
BIN="$PASTA_ALVO/release/sbf"
[[ -x "$BIN" ]] || { echo "binário não encontrado: $BIN" >&2; exit 1; }
if [[ ! -f THIRD-PARTY-LICENSES ]]; then
  scripts/desktop/gerar-licencas.sh || echo "aviso: sem THIRD-PARTY-LICENSES (cargo-about ausente)" >&2
fi

rm -rf "dist/$NOME" dist/deb-root
mkdir -p "dist/$NOME"

# --- tar.gz portátil: executável + assets (se houver) lado a lado -------------------------
install -m755 "$BIN" "dist/$NOME/sbf"
[[ -n "$PASTA_ASSETS" ]] && cp -r "$PASTA_ASSETS" "dist/$NOME/assets"
copiar_licencas "dist/$NOME"
cp docs/desktop/instalacao.md "dist/$NOME/INSTALACAO.md"
tar -C dist -czf "dist/$NOME.tar.gz" "$NOME"
rm -rf "dist/$NOME"
echo "gerado: dist/$NOME.tar.gz"

# --- .deb: /usr/bin/sbf + /usr/share/seu-bolso-feliz/assets --------------------------------
if command -v dpkg-deb >/dev/null; then
  RAIZ_DEB=dist/deb-root
  install -Dm755 "$BIN" "$RAIZ_DEB/usr/bin/sbf"
  mkdir -p "$RAIZ_DEB/usr/share/seu-bolso-feliz" "$RAIZ_DEB/usr/share/doc/seu-bolso-feliz"
  [[ -n "$PASTA_ASSETS" ]] && cp -r "$PASTA_ASSETS" "$RAIZ_DEB/usr/share/seu-bolso-feliz/assets"
  copiar_licencas "$RAIZ_DEB/usr/share/doc/seu-bolso-feliz"
  install -d "$RAIZ_DEB/usr/share/applications"
  cat > "$RAIZ_DEB/usr/share/applications/seu-bolso-feliz.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Seu Bolso Feliz
Comment=Inventario financeiro local
Exec=sbf --inventario
Terminal=false
Categories=Office;Finance;
DESK
  TAM_KB=$(du -sk "$RAIZ_DEB" | cut -f1)
  mkdir -p "$RAIZ_DEB/DEBIAN"
  cat > "$RAIZ_DEB/DEBIAN/control" <<CTRL
Package: seu-bolso-feliz
Version: $VERSAO
Section: utils
Priority: optional
Architecture: $ARQ_DEB
Installed-Size: $TAM_KB
Depends: libc6, libxkbcommon0, libwayland-client0, libgl1, poppler-utils, qpdf
Recommends: rclone, gnome-keyring | libsecret-1-0
Maintainer: Chico Figueiredo <fran.fig@gmail.com>
Description: Seu Bolso Feliz, inventario financeiro local
 Aplicativo desktop que le o cofre de documentos financeiros, mostra o
 inventario e registra as suas decisoes. Funciona sem conta e sem rede.
 poppler-utils (pdftotext) e qpdf leem PDFs protegidos; rclone faz a
 estante e a copia de seguranca no Drive.
CTRL
  find "$RAIZ_DEB" -type d -exec chmod 755 {} +
  dpkg-deb --root-owner-group --build "$RAIZ_DEB" "dist/seu-bolso-feliz_${VERSAO}_${ARQ_DEB}.deb" >/dev/null
  rm -rf "$RAIZ_DEB"
  echo "gerado: dist/seu-bolso-feliz_${VERSAO}_${ARQ_DEB}.deb"
fi
(cd dist && sha256sum ./*.tar.gz ./*.deb > SHA256SUMS 2>/dev/null || true)
