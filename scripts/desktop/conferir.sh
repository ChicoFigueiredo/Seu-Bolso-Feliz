#!/usr/bin/env bash
# Verificação de ponta a ponta: fmt, clippy, testes Rust, testes Bun do worker e build release.
# Roda tudo, não para na primeira falha, e imprime um resumo. Saída 0 só se tudo passou.
# Uso: scripts/desktop/conferir.sh
set -o pipefail
cd "$(dirname "$0")/../.."
export PATH="$HOME/.bun/bin:$HOME/.cargo/bin:$PATH"
LOG=$(mktemp -d)
declare -a NOMES RESULT

passo() {
  local nome=$1; shift
  echo ">>> $nome"
  if "$@" >"$LOG/${#NOMES[@]}.log" 2>&1; then RESULT+=("PASS"); else RESULT+=("FAIL"); tail -n 25 "$LOG/${#NOMES[@]}.log"; fi
  NOMES+=("$nome")
}

passo "cargo fmt --check" cargo fmt --all --check
passo "cargo clippy -D warnings" cargo clippy -j 4 --workspace --all-targets -- -D warnings
passo "cargo test --workspace" cargo test -j 4 --workspace
passo "bun test (workers/inventario)" bash -c 'cd workers/inventario && bun test'
passo "cargo build --release" cargo build -j 4 --release --workspace

echo
echo "=== Resumo ==="
FALHAS=0
for i in "${!NOMES[@]}"; do
  printf '%-34s %s\n' "${NOMES[$i]}" "${RESULT[$i]}"
  [[ "${RESULT[$i]}" == PASS ]] || FALHAS=1
done
echo "logs completos em: $LOG"
exit $FALHAS
