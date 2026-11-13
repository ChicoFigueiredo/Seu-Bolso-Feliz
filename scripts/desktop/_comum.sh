#!/usr/bin/env bash
# Funções comuns dos scripts de empacotamento do Seu Bolso Feliz desktop (source, não execute).
# Define: RAIZ, VERSAO, PASTA_ALVO, PASTA_ASSETS (vazia se a janela ainda não tem assets).

RAIZ=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$RAIZ"

META=$(cargo metadata --no-deps --format-version 1)
VERSAO=$(python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="sbf"))' <<<"$META")
PASTA_ALVO=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])' <<<"$META")

# Os arquivos ao lado do binário (fontes, temas) moram em crates/sbf-janela/assets quando a
# janela existe. Sem a pasta, o pacote sai só com o binário.
PASTA_ASSETS=""
for candidata in crates/sbf-janela/assets crates/sbf/assets; do
  if [[ -d "$candidata" ]]; then PASTA_ASSETS="$candidata"; break; fi
done

# Copia as licenças que existirem (o repositório pode ainda não ter LICENSE-*).
copiar_licencas() {
  local destino=$1 f
  for f in LICENSE LICENSE-MIT LICENSE-APACHE THIRD-PARTY-LICENSES; do
    [[ -f "$f" ]] && cp "$f" "$destino/"
  done
  return 0
}
