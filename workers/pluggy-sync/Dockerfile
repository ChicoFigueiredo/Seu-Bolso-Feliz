# syntax=docker/dockerfile:1
#
# Build a partir da RAIZ do monorepo (docker-compose.yml deste diretório já
# aponta o context certo):
#   docker build -f workers/pluggy-sync/Dockerfile -t sbf/pluggy-sync .
#
# Sem segredo embutido: PLUGGY_CLIENT_ID/SECRET, SUPABASE_SECRET_KEY e
# LOCAL_USER_ID entram só em runtime (env do container), nunca como
# build arg nem COPY.
FROM oven/bun:1-slim
WORKDIR /app

# Workspace Bun: "workspace:*" (ver package.json raiz) exige a árvore
# completa de workspaces pra resolver — .dockerignore já corta
# node_modules, apps/web/.next, docs, testes etc. antes daqui.
COPY . .

RUN bun install --frozen-lockfile --production \
 && chmod +x workers/pluggy-sync/docker-entrypoint.sh \
 && groupadd --gid 1001 pluggy \
 && useradd --uid 1001 --gid pluggy --home-dir /app --shell /usr/sbin/nologin pluggy \
 && chown -R pluggy:pluggy /app

USER pluggy
WORKDIR /app/workers/pluggy-sync

HEALTHCHECK --interval=5m --timeout=10s --start-period=1m --retries=3 \
  CMD bun run healthcheck.ts

ENTRYPOINT ["/app/workers/pluggy-sync/docker-entrypoint.sh"]
