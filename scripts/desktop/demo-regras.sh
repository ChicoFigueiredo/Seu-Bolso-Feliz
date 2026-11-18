#!/usr/bin/env bash
# Demonstração das regras de reconhecimento pelo terminal (spec 13, ADR-021).
#
# Cria um cofre TEMPORÁRIO (nada do cofre real é tocado), com fixtures fictícias de
# scripts/desktop/demo-regras/, e mostra: importar, testar, simular, ativar (VOCÊ digita a
# confirmação), trocar o Luau sem recompilar, importar a v2, ativar e reverter.
#
# Precisa de um terminal interativo: ativar e reverter não rodam sem ele (D14).
#   scripts/desktop/demo-regras.sh            usa o binário de debug (compila se faltar)
#   SBF=/caminho/do/sbf scripts/desktop/demo-regras.sh
set -euo pipefail

RAIZ=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
FIXTURES="$RAIZ/scripts/desktop/demo-regras"
REGRA="comprovante-pix-banco-exemplo"

if [[ ! -t 0 ]]; then
  echo "Esta demonstração pede um terminal interativo (ativar e reverter exigem confirmação digitada)." >&2
  exit 2
fi

if [[ -z "${SBF:-}" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
  (cd "$RAIZ" && cargo build -q -p sbf)
  ALVO=$(cd "$RAIZ" && cargo metadata --no-deps --format-version 1 |
    python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
  SBF="$ALVO/debug/sbf"
fi

COFRE=$(mktemp -d)
trap 'rm -rf "$COFRE"' EXIT
regras() { "$SBF" regras --inventario "$COFRE" "$@"; }
passo() { printf '\n== %s\n' "$*"; }

passo "1. Importar a regra v1 (rascunho, com o script Luau v1)"
regras importar "$FIXTURES/regra.json" --script "$FIXTURES/regra-v1.luau" --motivo "demonstração v1"

passo "2. Gravar o exemplo que a v1 precisa cumprir"
regras exemplo "$REGRA" "$FIXTURES/comprovante.txt" --esperado "$FIXTURES/esperado-v1.json"

passo "3. Testar a v1"
regras testar "$REGRA" 1

passo "4. Simular: o que mudaria se a v1 entrasse em vigor (só leitura)"
regras simular "$REGRA" 1 "$FIXTURES/comprovante.txt"

passo "5. Ativar a v1: digite a confirmação pedida"
regras ativar "$REGRA" 1

passo "6. Reconhecimento da v1"
regras listar
# O passo `sbf reconhecer` depende da integração do motor de regras no reconhecimento
# (a ser ligada pelo integrador). Quando existir, descomente:
# "$SBF" reconhecer --inventario "$COFRE" --arquivo "$FIXTURES/comprovante.txt"
regras simular "$REGRA" 1 "$FIXTURES/comprovante.txt"

passo "7. Trocar o Luau (sem recompilar nada) e importar a v2"
regras importar "$FIXTURES/regra.json" --script "$FIXTURES/regra-v2.luau" --motivo "demonstração v2: maiúsculas e tarifa"

passo "8. Simular a v2: a diferença para a v1 em vigor"
regras simular "$REGRA" 2 "$FIXTURES/comprovante.txt"

passo "9. Atualizar o exemplo, testar e ativar a v2: digite a confirmação"
regras exemplo "$REGRA" "$FIXTURES/comprovante.txt" --esperado "$FIXTURES/esperado-v2.json"
regras testar "$REGRA" 2
regras ativar "$REGRA" 2
regras simular "$REGRA" 2 "$FIXTURES/comprovante.txt"
# "$SBF" reconhecer --inventario "$COFRE" --arquivo "$FIXTURES/comprovante.txt"   # depende da integração

passo "10. Reverter para a v1: digite a confirmação"
regras reverter "$REGRA"
regras mostrar "$REGRA"

passo "Fim. O cofre temporário foi usado só aqui e será apagado."
