# ADR-011 — O inventário financeiro vive fora do app, num SQLite e num cofre locais

**Data:** 2026-11-11 · **Status:** aceita · **Revisada por:** ADR-014 (2026-11-12): o inventário passa a morar no desktop, ainda fora da nuvem

## Contexto e decisão

O CEO precisa inventariar todos os achados financeiros que existem em seis
discos, nas nuvens (OneDrive, Google Drive), no Gmail, no Outlook, no
celular, no Telegram e na Pluggy antes de qualquer análise. São ~9 milhões de
arquivos, a maioria não financeira. O inventário roda **fora do pipeline de
ingestão do app**: um índice SQLite e um cofre de cópias, ambos em
`/mnt/d/Chico/financas/`, fora do repositório. Nada vai para o Neon ou para o
Vercel Blob até o CEO aprovar um lote para promoção.

## Opções descartadas

- **Usar os scanners existentes** (`local-scanner`, `gmail-scanner`). Eles
  sobem cada arquivo para o Vercel Blob e gravam em `source_documents` na hora.
  O `--dry-run` não persiste nada, e o `--move-processed-to` move o original.
  Isso mandaria fotos de férias e livros para a nuvem e para o banco de
  produção.
- **Usar o Neon como índice do inventário.** Mesmo problema: o banco de
  produção receberia milhões de achados descartados.
- **Cofre em `financas/` dentro do repo, ignorado pelo git.** Um
  `git clean -fdx` ou um checkout que falha no meio apagaria ou corromperia o
  cofre. No dia desta decisão, um checkout fez exatamente isso com o working
  tree.

## Consequências

- O SQLite espelha as colunas de `source_documents` e usa o mesmo SHA-256 dos
  bytes (`computeContentHash`). Assim, a promoção é um mapeamento, e um
  documento promovido casa com o que já existir no app pelo hash.
- O cofre guarda uma cópia por conteúdo único (acervo endereçado por hash).
  As pastas por ciclo são uma vista gerada com hardlinks e podem ser refeitas
  a partir do SQLite. Os originais nunca são movidos nem alterados.
- As senhas de PDF ficam num arquivo cifrado dentro do cofre, aberto por uma
  senha-mestra digitada uma vez por sessão. O SQLite guarda só a regra
  fornecedor → estratégia, nunca o segredo. O `user_secrets` do Neon continua
  sendo o lugar das senhas para o app; o cofre local não o substitui.
- O código dos coletores fica versionado no repositório, e os dados nunca.
