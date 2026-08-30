#!/usr/bin/env bash
# Aplica as migrations do Neon, em ordem, dentro de uma transação cada.
#
# Substitui o `supabase db push`. As migrations vivem em packages/db/migrations
# e são idempotentes: rodar duas vezes não quebra nem duplica.
#
# Usa DATABASE_URL_UNPOOLED: DDL pelo pooler pode cair num backend diferente
# no meio do trabalho.
set -euo pipefail

URL="${DATABASE_URL_UNPOOLED:-${DATABASE_URL:-}}"
if [ -z "$URL" ]; then
  echo "DATABASE_URL_UNPOOLED (ou DATABASE_URL) é obrigatório" >&2
  exit 1
fi

for arquivo in packages/db/migrations/*.sql; do
  echo "→ $(basename "$arquivo")"
  psql "$URL" -v ON_ERROR_STOP=1 -q -f "$arquivo"
done
echo "✔ migrations aplicadas"
