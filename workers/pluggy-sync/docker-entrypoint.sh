#!/usr/bin/env sh
# Loop de execução do worker pluggy-sync em produção (VPS).
#
# O worker em si (src/index.ts) é uma rodagem única, pensada pra cron (ver
# cabeçalho de src/index.ts) — não um daemon. Este script transforma isso
# num container de longa duração compatível com `restart: unless-stopped`:
# roda o sync, espera SYNC_INTERVAL_SECONDS, repete.
#
# O heartbeat é tocado a cada início de rodada, não só em caso de sucesso —
# ele prova que o *loop* está vivo, não que o último sync deu certo. Uma
# falha de API do Pluggy não deve derrubar o container (reiniciar não
# resolve uma API fora do ar); só um processo travado deve. Ver
# healthcheck.ts para o outro lado dessa checagem.
set -eu

HEARTBEAT_FILE="${HEARTBEAT_FILE:-/tmp/pluggy-sync-heartbeat}"
SYNC_INTERVAL_SECONDS="${SYNC_INTERVAL_SECONDS:-3600}"

echo "pluggy-sync: loop iniciado (intervalo ${SYNC_INTERVAL_SECONDS}s, args: $*)"

while true; do
  touch "$HEARTBEAT_FILE"
  if ! bun run src/index.ts "$@"; then
    echo "pluggy-sync: rodada falhou — próxima tentativa em ${SYNC_INTERVAL_SECONDS}s" >&2
  fi
  sleep "$SYNC_INTERVAL_SECONDS"
done
