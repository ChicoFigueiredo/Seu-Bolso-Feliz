#!/usr/bin/env bash
# Regenera as fixtures de ouro do sbf-livro a partir do código TS deste worktree.
# As fixtures ficam versionadas: `cargo test -p sbf-livro` não precisa de bun nem de banco.
#
#   crates/sbf-livro/ouro/gerar.sh            # regras puras (ciclo, rateio, amortização, recorrência)
#   crates/sbf-livro/ouro/gerar.sh --banco    # também a Hoje e os Centros (Postgres em Docker)
set -euo pipefail
export PATH="$HOME/.bun/bin:$PATH"
cd "$(dirname "$0")"

TZ=America/Sao_Paulo bun ciclo.ts > fixtures/ciclo-sao-paulo.json
TZ=UTC bun ciclo.ts > fixtures/ciclo-utc.json
bun rateio.ts > fixtures/rateio.json
bun amortizacao.ts > fixtures/amortizacao.json
bun --preload ./preparo.ts recorrencia.ts > fixtures/recorrencia.json

# Painéis: Postgres local de teste em Docker (porta só em 127.0.0.1, contêiner apagado no fim).
if [[ "${1:-}" == "--banco" ]]; then
  NOME="sbf-teste-livro-ouro-$$"
  RAIZ="$(cd ../../.. && pwd)"
  docker run -d --rm --name "$NOME" -e POSTGRES_PASSWORD=teste -e POSTGRES_DB=livro \
    -p 127.0.0.1::5432 postgres:17-alpine > /dev/null
  trap 'docker rm -f "$NOME" > /dev/null' EXIT
  until docker exec "$NOME" pg_isready -U postgres -d livro -q 2> /dev/null; do sleep 0.5; done
  sleep 1
  # Um psql por arquivo: o schema.sql zera o search_path, e as migrações contam com ele.
  for f in "$RAIZ/packages/db/schema.sql" "$RAIZ"/packages/db/migrations/*.sql; do
    docker exec -i "$NOME" psql -U postgres -d livro -v ON_ERROR_STOP=1 -q < "$f" > /dev/null
  done
  PORTA="$(docker port "$NOME" 5432/tcp | head -1 | sed 's/.*://')"
  SBF_OURO_PG="postgres://postgres:teste@127.0.0.1:$PORTA/livro" TZ=America/Sao_Paulo \
    bun --preload ./preparo.ts paineis.ts > fixtures/paineis.json
fi
