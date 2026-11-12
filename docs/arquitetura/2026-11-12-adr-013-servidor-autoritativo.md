# ADR-013 — O servidor é a autoridade: API Rust na VPS, estações com cache e alterações

**Data:** 2026-11-12 · **Status:** aceita

## Contexto e decisão

O desktop guarda uma cópia local em SQLite e precisa escrever no livro-caixa. O ponto de
partida tem quatro limites:

- Não existe API: o CRUD inteiro é feito por Server Actions.
- A sessão é um cookie JWT de 30 dias, sem revogação, e não existe identidade de
  dispositivo.
- O ADR-010 recusou API HTTP entre a Vercel e os workers.
- O Neon está em `us-east-1`, e a VPS fica em São Paulo.

**Decisão:**

- **A autoridade:** o `sbf servidor`, rodando na VPS, é a única autoridade do
  livro-caixa.
- **O canal das estações:**
  - Cada estação fala com o servidor só por HTTPS.
  - A credencial é própria de cada estação e pode ser revogada. O servidor a emite depois
    do login Google.
  - Nenhuma estação guarda a `DATABASE_URL`, nem o worker local.
- **Escrita offline:**
  - Uma edição feita sem rede vira **alteração**, que o servidor aplica inteira ou recusa
    com as regras da versão dele.
  - Uma alteração recusada aparece ao lado da versão do servidor. Quem decide é o
    usuário, e nada é mesclado sozinho.
- **O banco:** o Neon migra para `aws-sa-east-1`. Como a região é fixa, isso exige um
  projeto novo e migração, com o portão humano do ADR-007.

O ADR-010 continua valendo para os workers que rodam no servidor, ao lado do banco. Esta
ADR só cria o canal das estações, que o ADR-010 não cobria. O princípio de NAT dele se
mantém: a estação puxa e empurra, e o servidor nunca chama a estação.

## Opções descartadas

- **Desktop direto no Postgres.** Cada máquina guardaria a credencial do banco inteiro e
  aplicaria as regras da versão instalada. Uma estação desatualizada gravaria com regra
  velha.
- **API TS na Vercel.** Contraria o ADR-012.
- **Local-first multi-mestre.** Fundir duas cópias de dinheiro sozinho é risco sem
  ganho, para um usuário só.
- **Offline só para leitura.** É mais simples, mas o desktop perderia a escrita offline.

## Consequências

- **Esquema:** o banco precisa do que a sincronização exige e hoje não existe:
  - tombstone no lugar dos 30 deletes físicos das actions;
  - versão por linha;
  - `updated_at` em toda tabela sincronizada (hoje ficam de fora 14 tabelas, entre elas
    `transaction_allocations`).
- **Superfície de ataque:** passa a existir um endpoint exposto na VPS, o custo que o
  ADR-010 tinha evitado. A mitigação é a credencial por estação, revogável.
- **Web:** o web na Vercel passa a chamar a API em São Paulo, e a latência dele
  muda de lugar, mas não piora.
