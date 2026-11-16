#!/usr/bin/env bash
# Gera o THIRD-PARTY-LICENSES na raiz (dependências Rust do binário `sbf`).
# Requer: cargo install cargo-about --locked --features cli
set -euo pipefail
cd "$(dirname "$0")/../.."
command -v cargo-about >/dev/null || { echo "cargo-about ausente: cargo install cargo-about --locked --features cli" >&2; exit 1; }
cargo about generate --manifest-path crates/sbf/Cargo.toml -c scripts/desktop/about.toml scripts/desktop/about.hbs -o THIRD-PARTY-LICENSES
echo "gerado: THIRD-PARTY-LICENSES"
