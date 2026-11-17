#!/usr/bin/env bash
# Configura o secret BANCADA_TOKEN no GitHub do Seu Bolso Feliz.
# Uso: scripts/desktop/configurar-token-da-bancada.sh
set -euo pipefail

REPO_BANCADA="ChicoFigueiredo/bancada"
REPO_SBF="ChicoFigueiredo/Seu-Bolso-Feliz"

cat <<'TXT'
Por que: o workspace Rust depende por caminho do repositório privado da bancada.
A CI do GitHub precisa de um token só de leitura para baixá-lo.
Sem o secret BANCADA_TOKEN, o job Rust avisa e pula o cargo (não falha).
TXT

command -v gh >/dev/null || { echo "Erro: o gh (GitHub CLI) não está instalado." >&2; exit 1; }

URL="https://github.com/settings/personal-access-tokens/new"
echo
echo "Abrindo: $URL"
if command -v xdg-open >/dev/null; then xdg-open "$URL" >/dev/null 2>&1 || true; fi

cat <<TXT

No formulário, marque:
  1. Resource owner: a sua conta (ChicoFigueiredo).
  2. Repository access: Only select repositories -> somente $REPO_BANCADA.
  3. Repository permissions -> Contents: Read-only (Metadata fica só leitura sozinho).
  4. Expiration: escolha uma validade (ex.: 90 dias) e anote para renovar.
Clique em Generate token e copie o valor.

TXT

TOKEN=""
read -rs -p "Cole o token (não aparece na tela): " TOKEN
echo
[[ -n "$TOKEN" ]] || { echo "Erro: token vazio." >&2; exit 1; }

echo "Conferindo acesso a $REPO_BANCADA..."
if ! GH_TOKEN="$TOKEN" gh api "repos/$REPO_BANCADA" --silent; then
  TOKEN=""
  echo "Erro: o token não consegue ler $REPO_BANCADA. Revise repositório e permissão." >&2
  exit 1
fi

echo "Gravando o secret BANCADA_TOKEN em $REPO_SBF..."
if printf '%s' "$TOKEN" | gh secret set BANCADA_TOKEN -R "$REPO_SBF"; then
  TOKEN=""
  echo "Pronto: BANCADA_TOKEN configurado. A próxima execução da CI roda o cargo."
else
  TOKEN=""
  echo "Erro: não foi possível gravar o secret (o gh está logado com permissão no repo?)." >&2
  exit 1
fi
