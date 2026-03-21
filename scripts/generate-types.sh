#!/usr/bin/env bash
set -euo pipefail

# Gera tipos TypeScript a partir do schema do Supabase
# Uso: bash scripts/generate-types.sh

OUTPUT_FILE="packages/shared-types/src/database.types.ts"

if [[ -z "${SUPABASE_PROJECT_ID:-}" ]]; then
  echo "❌ SUPABASE_PROJECT_ID não definido. Usando Supabase local..."
  npx supabase gen types typescript --local 2>/dev/null > "$OUTPUT_FILE"
else
  npx supabase gen types typescript --project-id "$SUPABASE_PROJECT_ID" 2>/dev/null > "$OUTPUT_FILE"
fi

echo "✅ Tipos gerados em $OUTPUT_FILE"
