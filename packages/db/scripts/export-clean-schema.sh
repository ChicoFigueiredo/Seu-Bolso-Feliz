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
# em 2026-08-26, não adivinhado a priori — os números abaixo saíram desse
# dump: 161 blocos removidos, 4 guard clauses, 1 comentário órfão):
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
#   - dentro de 4 funções que JÁ recebem p_user_id como parâmetro e usam
#     auth.uid() para amarrar esse parâmetro à identidade do JWT, remove só
#     essa checagem -- NÃO porque fosse redundante (não era, em Supabase),
#     mas porque não há auth.uid()/JWT/PostgREST no Neon para checar contra.
#     Fase 2 precisa reintroduzir essa amarração por outro mecanismo antes
#     de expor qualquer uma dessas 4 funções a um cliente não confiável
#     (ver NOTA no topo do schema.sql gerado).
#
# Depois de limpar, o script ainda:
#   - reinstala `CREATE EXTENSION IF NOT EXISTS pg_trgm` (perdido pelo
#     --schema=public do pg_dump, necessário pros índices GIN
#     idx_supplier_aliases_trgm / idx_suppliers_name_trgm -- sem ele o DDL
#     nem chega a aplicar)
#   - reinstala `CREATE EXTENSION IF NOT EXISTS pgcrypto` (mesma perda do
#     --schema=public; encrypt_secret/decrypt_secret chamam
#     pgp_sym_encrypt/pgp_sym_decrypt. Aqui a falta quebraria só em RUNTIME,
#     não na aplicação do DDL -- por isso passou despercebido até a revisão
#     de branch inteiro)
#   - troca `CREATE SCHEMA public;` por `CREATE SCHEMA IF NOT EXISTS public;`
#     (Neon já vem com `public` criado -- sem o IF NOT EXISTS, aplicar este
#     arquivo numa instância Neon nova e nunca tocada falha na linha 1)
#   - grava no topo do arquivo gerado uma NOTA com os 4 gaps que esta task
#     deliberadamente NÃO fecha (schema `private`/chave de criptografia,
#     perda de ON DELETE CASCADE nas 46 FKs de auth.users removidas, a
#     amarração JWT->p_user_id acima, e as 6 funções descartadas por inteiro
#     com seus call sites ainda vivos). Essa NOTA, dentro do schema.sql
#     gerado, é o handoff canônico para a Fase 2 -- leia ela, não relatórios
#     de sessão.
#
# Confirma com grep (escopado a linhas fora de comentário `--`, já que a
# NOTA acima menciona auth.uid()/auth.users de propósito) que não sobrou
# nenhuma referência FUNCIONAL a auth./CREATE POLICY/ROW LEVEL SECURITY.

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
