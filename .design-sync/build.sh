#!/usr/bin/env bash
# Rebuild do bundle do design system. Caminhos absolutos: o conversor resolve
# --entry a partir do cwd, e a raiz do repo tem um src/ legado que ele captura
# por engano quando o caminho e relativo.
set -euo pipefail
ROOT=/mnt/d/Chico/seu.bolso.feliz
node "$ROOT/.ds-sync/package-build.mjs" --config "$ROOT/.design-sync/config.json" \
  --node-modules "$ROOT/packages/design-system/node_modules" \
  --entry "$ROOT/packages/design-system/dist/index.js" \
  --out "$ROOT/ds-bundle" "$@"
