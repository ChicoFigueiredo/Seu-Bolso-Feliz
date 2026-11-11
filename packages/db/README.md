# @sbf/db

Acesso ao Postgres (Neon) via Drizzle. `client.ts` abre a conexão, `schema.ts` e
`relations.ts` são o schema tipado, `queue.ts` é a fila de jobs, `secrets.ts`
grava segredos cifrados e `owner.ts` responde "gravar em nome de quem?" para os
processos que rodam sem sessão.

## ⚠️ `schema.sql` é a linha de base, não o estado atual

`schema.sql` é um `pg_dump` congelado no momento da migração para o Neon. O
banco de verdade é ele **mais** tudo em `migrations/`, aplicado em ordem.

Isso já custou caro: uma tela foi montada oferecendo 4 tipos de instituição
porque foi isso que `schema.sql` mostrou, enquanto o banco aceita 6 desde a
migration `0003`. Um `CHECK` lido no lugar errado vira um `<select>` que ou
esconde opções válidas, ou oferece opção que o banco recusa.

**Antes de confiar num `CHECK`, num default ou numa coluna, confirme nos dois
lugares** — ou, melhor, pergunte ao banco:

```sql
SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conname = 'institutions_type_check';
```

## Driver

O `neon-http` **não tem `transaction()`**. Para atomicidade, use `batch()`.
Para `LISTEN/NOTIFY` (a fila), é preciso a conexão direta: `DATABASE_URL_UNPOOLED`.
