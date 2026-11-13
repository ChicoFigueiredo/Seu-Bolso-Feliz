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
