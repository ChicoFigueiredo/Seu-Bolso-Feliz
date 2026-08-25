# Deploy do worker pluggy-sync (VPS)

> Fase 5 do plano ([`docs/planejamento/2026-08-24-plano-integracao-pluggy.md`](../planejamento/2026-08-24-plano-integracao-pluggy.md)) —
> parcialmente 🤚: Dockerfile/compose/healthcheck feitos sem acesso à VPS;
> o deploy real e a verificação de restart nela **precisam** do acesso SSH
> (`ssh root@ssh.chico-figueiredo.com.br`), que a sessão de desenvolvimento
> não tem. Este doc é a receita pronta pra quando esse acesso existir — sem
> valor real de segredo em lugar nenhum aqui.

## O que é

`workers/pluggy-sync/src/index.ts` é uma rodagem única (pensada pra cron),
não um daemon — ver o cabeçalho do arquivo. O
[`Dockerfile`](../../workers/pluggy-sync/Dockerfile) e o
[`docker-entrypoint.sh`](../../workers/pluggy-sync/docker-entrypoint.sh)
envolvem essa rodagem única num loop (`sync → sleep → sync → ...`) pra virar
um container de longa duração compatível com `restart: unless-stopped`. O
[`healthcheck.ts`](../../workers/pluggy-sync/healthcheck.ts) confere só que
o _loop_ está vivo (heartbeat em arquivo), não que o último sync deu certo —
uma falha de API do Pluggy não deve derrubar o container, só um processo
travado deve.

## Pré-requisitos na VPS

- Docker + Docker Compose v2 instalados (`docker compose version`).
- Acesso de rede de saída da VPS para a API do Pluggy e para o Postgres
  (Neon, quando a Fase 6 estiver pronta; até lá, Supabase).
- Repositório clonado (ou só o diretório `workers/pluggy-sync/` + o restante
  do monorepo, já que o build precisa da árvore completa — ver comentário no
  Dockerfile).

## Configurar segredos

```bash
cd workers/pluggy-sync
cp .env.example .env
# edite .env com os valores reais: SUPABASE_URL, SUPABASE_SECRET_KEY,
# LOCAL_USER_ID, PLUGGY_CLIENT_ID, PLUGGY_CLIENT_SECRET
```

`.env` já está coberto pelo `.gitignore` raiz (padrão `.env`) — nunca commitar
esse arquivo. Nenhum segredo entra na imagem em tempo de build (sem build
args, sem `COPY` de `.env`); tudo é lido do ambiente do container em runtime.

## Start / stop / restart / status / logs

Rodar sempre a partir de `workers/pluggy-sync/` (onde está o `docker-compose.yml`
e o `.env`):

```bash
# build + start (detached), aplica o healthcheck e restart: unless-stopped
docker compose up -d --build

# status do container (inclui STATUS de healthcheck: healthy/unhealthy/starting)
docker compose ps

# logs em tempo real (cada rodada de sync, incluindo falhas — ver nota acima
# sobre heartbeat vs. sucesso do sync)
docker compose logs -f

# restart manual (não é o mesmo que o restart automático de crash —
# restart: unless-stopped só age se o processo do container morrer)
docker compose restart

# parar (mantém o container criado, não roda mais nada até `start`/`up`)
docker compose stop

# parar e remover o container (a imagem construída fica em cache local)
docker compose down
```

## Rodar um backfill manual

O loop do container roda sync **incremental** por padrão. Um backfill (365
dias, com checkpoint — Fase 3) é uma operação pontual, não faz parte do
loop de restart. Rode como container avulso, fora do entrypoint em loop:

```bash
docker compose run --rm --entrypoint "bun run src/index.ts" pluggy-sync --backfill
```

## ⚠️ Não rodar dois processos ao mesmo tempo na mesma conexão

`startOrResumeCheckpoint` (`workers/pluggy-sync/src/checkpoint.ts`) faz um
SELECT seguido de upsert, **sem claim atômico** (ao contrário do locking
otimista de `transitionJob` em `workers/ingestion`). Isso é seguro hoje
porque só existe um processo rodando este worker por vez. No dia em que a
VPS estiver rodando o loop **e** alguém disparar o mesmo backfill
manualmente (ou de outra máquina, ex. local) contra a mesma conexão, existe
uma janela de corrida real — dois processos podem pegar o mesmo checkpoint
e duplicar trabalho ou corromper a paginação.

**Pendência registrada, não implementada nesta fase** (fora do escopo do
Dockerfile/compose): a solução já desenhada é uma coluna `locked_at` +
`UPDATE ... WHERE locked_at IS NULL OR locked_at < now() - interval
'10 minutes' RETURNING id`, análoga ao locking de `transitionJob`. Até lá,
regra operacional: rodar o worker (loop **ou** backfill manual) em só um
lugar por vez, nunca VPS e local simultaneamente na mesma conexão Pluggy.

## Variáveis de ambiente

| Variável                    | Obrigatória       | Descrição                                                       |
| --------------------------- | ----------------- | --------------------------------------------------------------- |
| `SUPABASE_URL`              | sim               | URL do projeto (produção/branch de dev, ver plano Fase 1)       |
| `SUPABASE_SECRET_KEY`       | sim               | Service role key — nunca a publishable key                      |
| `LOCAL_USER_ID`             | sim               | App de usuário único (ADR-007)                                  |
| `PLUGGY_CLIENT_ID`          | sim               | Meu Pluggy → app → credenciais (🤚 CEO cria)                    |
| `PLUGGY_CLIENT_SECRET`      | sim               | idem                                                            |
| `SYNC_INTERVAL_SECONDS`     | não (3600)        | Intervalo entre rodadas de sync incremental no loop             |
| `HEARTBEAT_MAX_AGE_SECONDS` | não (2×intervalo) | Idade máxima do heartbeat antes do healthcheck marcar unhealthy |

## Fora de escopo deste documento

- Deploy real na VPS e verificação de que `restart: unless-stopped` de fato
  recupera o container — depende do acesso SSH (🤚 CEO), não verificável
  localmente.
- Provisionamento de Neon/Vercel (Fase 6).
