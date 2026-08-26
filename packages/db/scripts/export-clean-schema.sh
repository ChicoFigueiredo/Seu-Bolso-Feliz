#!/usr/bin/env bash
# Exporta um dump schema-only do Postgres local do Supabase e remove tudo que
# depende de auth.*/RLS/storage.*, produzindo um DDL aplicável num Neon puro.
#
# Uso:
#   supabase start && supabase db reset   # garante as 42 migrations aplicadas
#   packages/db/scripts/export-clean-schema.sh > packages/db/schema.sql
#
# Requer: um container supabase_db_* rodando localmente (usamos pg_dump DE
# DENTRO do container porque a versão do Postgres do Supabase local, 17.x,
# costuma ser mais nova que o pg_dump do host — pg_dump recusa dump de um
# servidor mais novo que ele mesmo).
#
# O que este script remove do dump bruto (decidido inspecionando o dump real
# em 2026-08-26, não adivinhado a priori — ver task-3-report.md):
#   - todo bloco CREATE POLICY
#   - todo ALTER TABLE ... ENABLE/FORCE ROW LEVEL SECURITY
#   - toda FK constraint que referencia auth.users(id) (não existe no Neon —
#     autorização passa a ser WHERE user_id = $1 explícito na aplicação,
#     conforme ADR-009)
#   - funções que só sabem "quem é o usuário atual" via auth.uid()/auth.role()
#     SEM receber user_id como parâmetro (fn_set_secret,
#     generate_financial_periods, get_financial_period_for_date,
#     increment_session_tokens, register_pattern_feedback, search_suppliers) —
#     portar essas de verdade é trabalho de Fase 2+, não de "portar o DDL"
#   - dentro de funções que JÁ recebem p_user_id como parâmetro e usam
#     auth.uid() só como checagem extra de consistência, remove só essa
#     checagem (a função continua funcionando, a posse já é garantida pelo
#     parâmetro)
#
# Depois da limpeza, o script confirma com grep que não sobrou nenhuma
# referência a auth./CREATE POLICY/ROW LEVEL SECURITY.

set -euo pipefail

CONTAINER="${SUPABASE_DB_CONTAINER:-supabase_db_seu.bolso.feliz}"
LOCAL_DB_URL="postgresql://postgres:postgres@127.0.0.1:5432/postgres"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

RAW_SQL="$(mktemp)"
trap 'rm -f "$RAW_SQL"' EXIT

docker exec "$CONTAINER" pg_dump "$LOCAL_DB_URL" \
  --schema-only --no-owner --no-privileges \
  --schema=public \
  > "$RAW_SQL"

# clean-schema.py já valida sozinho (grep interno) que não sobrou auth./
# CREATE POLICY/ROW LEVEL SECURITY antes de escrever a saída; com `set -e`
# acima, um leftover detectado ali aborta este script também.
python3 "$SCRIPT_DIR/clean-schema.py" "$RAW_SQL"
