#!/usr/bin/env bash
# Postgres de teste do livro-caixa (spec 07): `postgres:17` em Docker, só em 127.0.0.1,
# com `packages/db/schema.sql` + `packages/db/migrations/*.sql` e dados FICTÍCIOS.
# Nunca toca o Neon nem lê `.env`.
#
#   scripts/desktop/postgres-de-teste.sh testar [args do cargo test]
#       sobe um contêiner, roda os testes de integração do sbf-servidor contra ele
#       (SBF_PG_TESTE=1) e apaga o contêiner no fim, passe ou falhe;
#   scripts/desktop/postgres-de-teste.sh subir
#       sobe um contêiner com os dados fictícios de exemplo e imprime
#       `export SBF_DATABASE_URL=...` (para `sbf servidor` à mão);
#   scripts/desktop/postgres-de-teste.sh descer
#       apaga todos os contêineres `sbf-teste-pg-*`.
#
# Variáveis: SBF_PG_IMAGEM (padrão postgres:17), SBF_PG_PORTA (padrão: uma livre).
set -euo pipefail

RAIZ=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
IMAGEM="${SBF_PG_IMAGEM:-postgres:17}"
PREFIXO="sbf-teste-pg"

porta_livre() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'
}

# Sobe o contêiner, espera ficar pronto e aplica o esquema. Deixa NOME e URL definidos.
subir_banco() {
  NOME="${PREFIXO}-$$-$RANDOM"
  local porta="${SBF_PG_PORTA:-$(porta_livre)}"
  # trust: o contêiner só escuta em 127.0.0.1 e morre no fim; não há senha para vazar.
  docker run -d --rm --name "$NOME" \
    -e POSTGRES_HOST_AUTH_METHOD=trust -e POSTGRES_DB=sbf \
    -p "127.0.0.1:${porta}:5432" "$IMAGEM" >/dev/null
  URL="postgres://postgres@127.0.0.1:${porta}/sbf?sslmode=disable"
  local i
  for i in $(seq 1 60); do
    # pg_isready dentro do contêiner, e uma consulta de verdade (o initdb reinicia o servidor).
    if docker exec "$NOME" psql -U postgres -d sbf -qtAc 'select 1' >/dev/null 2>&1; then
      break
    fi
    sleep 0.5
  done
  sleep 1
  docker exec "$NOME" psql -U postgres -d sbf -qtAc 'select 1' >/dev/null
  aplicar "$RAIZ/packages/db/schema.sql"
  local m
  for m in "$RAIZ"/packages/db/migrations/*.sql; do
    aplicar "$m"
  done
}

aplicar() {
  docker exec -i "$NOME" psql -U postgres -d sbf -v ON_ERROR_STOP=1 -q -o /dev/null <"$1" \
    2> >(grep -v '^NOTICE\|^psql:.*NOTICE' >&2)
}

# Dados fictícios de exemplo: um dono, uma conta, duas categorias e três movimentações.
semear_exemplo() {
  docker exec -i "$NOME" psql -U postgres -d sbf -v ON_ERROR_STOP=1 -q <<'SQL'
INSERT INTO public.users (id, email, name, role, google_sub)
VALUES ('00000000-0000-4000-8000-0000000000a1', 'dono.ficticio@example.com', 'Dono Fictício', 'owner', 'sub-ficticio-1');
INSERT INTO public.user_financial_preferences (user_id, financial_cycle_start_day)
VALUES ('00000000-0000-4000-8000-0000000000a1', 20);
INSERT INTO public.institutions (id, user_id, name, type)
VALUES ('00000000-0000-4000-8000-0000000000b1', '00000000-0000-4000-8000-0000000000a1', 'Banco Exemplo', 'bank');
INSERT INTO public.financial_products (id, user_id, institution_id, name, type, current_balance)
VALUES ('00000000-0000-4000-8000-0000000000c1', '00000000-0000-4000-8000-0000000000a1',
        '00000000-0000-4000-8000-0000000000b1', 'Conta Exemplo', 'checking_account', 1234.56);
INSERT INTO public.categories (id, user_id, name) VALUES
  ('00000000-0000-4000-8000-0000000000d1', '00000000-0000-4000-8000-0000000000a1', 'Mercado'),
  ('00000000-0000-4000-8000-0000000000d2', '00000000-0000-4000-8000-0000000000a1', 'Casa');
INSERT INTO public.transactions (user_id, financial_product_id, type, amount, description, event_date, is_confirmed, category_id) VALUES
  ('00000000-0000-4000-8000-0000000000a1', '00000000-0000-4000-8000-0000000000c1', 'expense', 0.10, 'Bala Exemplo', '2026-11-02', true, '00000000-0000-4000-8000-0000000000d1'),
  ('00000000-0000-4000-8000-0000000000a1', '00000000-0000-4000-8000-0000000000c1', 'expense', 0.20, 'Chiclete Exemplo', '2026-11-03', true, NULL),
  ('00000000-0000-4000-8000-0000000000a1', '00000000-0000-4000-8000-0000000000c1', 'income', 5000.00, 'Salário Exemplo', '2026-11-05', true, NULL);
SQL
}

descer_todos() {
  local ids
  ids=$(docker ps -aq --filter "name=^${PREFIXO}-")
  if [[ -n "$ids" ]]; then
    # shellcheck disable=SC2086
    docker rm -f $ids >/dev/null
  fi
}

case "${1:-}" in
  testar)
    shift
    trap 'docker rm -f "$NOME" >/dev/null 2>&1 || true' EXIT
    subir_banco
    cd "$RAIZ"
    SBF_PG_TESTE=1 SBF_PG_TESTE_URL="$URL" cargo test -j 4 -p sbf-servidor "$@"
    ;;
  subir)
    subir_banco
    semear_exemplo
    echo "# contêiner: $NOME (apague com: $0 descer)"
    echo "export SBF_DATABASE_URL='$URL'"
    ;;
  descer)
    descer_todos
    ;;
  *)
    sed -n '2,15p' "$0"
    exit 2
    ;;
esac
