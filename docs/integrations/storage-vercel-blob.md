# Storage dos arquivos originais (Vercel Blob)

Os arquivos originais — boletos, faturas, comprovantes (`ingestion-originals`)
e o CSV das importações (`user-files`) — ficam no **Vercel Blob**. O pacote
`@sbf/storage` (`packages/storage/`) é o cliente, e é por ele que passam
`apps/web`, `apps/mcp-server`, `workers/ingestion`, `workers/gmail-scanner` e
`workers/local-scanner`.

## Duas decisões que explicam o resto do desenho

**Blob privado, sempre.** Todo objeto é gravado com `access: "private"`. A
URL do objeto não abre para quem não tem credencial, e a única forma de o
navegador mostrar um arquivo é a URL assinada de `getSignedUrl(path, ttl)`,
que expira. Não existe caminho neste pacote que produza link público
permanente — é documento financeiro, e "difícil de adivinhar" não é grau de
proteção aceitável.

**"Bucket" é prefixo de caminho.** O Vercel Blob tem uma loja só, sem
buckets. `storage.from("ingestion-originals").upload("user-1/x.pdf", …)`
grava em `ingestion-originals/user-1/x.pdf`. Quem junta e separa o prefixo é
sempre o `StorageBucket`, então `source_documents.storage_path` continua
guardando só a chave (`user-1/x.pdf`), como sempre guardou. É por isso também
que `addRandomSuffix` fica desligado no `put`: o nome do blob precisa ser
exatamente o que o Postgres registrou, senão o download não acha o arquivo.

## Variável de ambiente

| Variável                | Obrigatória | Descrição                                                                                                           |
| ----------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------- |
| `BLOB_READ_WRITE_TOKEN` | sim         | Token de leitura/escrita da loja. Carrega o id da loja — não há endpoint, região nem par de chaves para configurar. |

É a única. `loadStorageConfigFromEnv()` (`packages/storage/src/env.ts`) lança
um erro nomeando a variável quando ela falta, e o `StorageClient` recusa ser
construído sem token — o erro aparece na largada, não no meio de um upload. O
token vai explícito em toda chamada ao SDK, de propósito: assim o
`@vercel/blob` nunca sai procurando credencial sozinho no `process.env` de um
ambiente mal configurado.

### Onde pegar o token

1. Painel da Vercel → **Storage** → criar (ou abrir) a loja de **Blob** →
   conectar ao projeto.
2. Na loja, aba de conexão: copiar o valor de `BLOB_READ_WRITE_TOKEN`. Pela
   CLI, o equivalente é `vercel link` e depois `vercel env pull .env.local`,
   que traz a variável já preenchida.

O token precisa existir **nos dois lugares**:

- **Em produção**: a Vercel injeta a variável sozinha no projeto conectado à
  loja. Confira em Settings → Environment Variables antes do primeiro deploy
  que depende de upload.
- **No `.env.local` / `.env`**: os workers (`ingestion`, `gmail-scanner`,
  `local-scanner`) e o MCP server rodam fora da Vercel e leem a mesma
  variável do ambiente local. Sem ela, eles não sobem.

É um segredo: nunca vai para o repositório, nem para variável `NEXT_PUBLIC_*`.
Se vazar, revogue e gere outro no painel da loja.

## API do pacote (referência rápida)

```ts
import { createStorageClientFromEnv } from "@sbf/storage";

const storage = createStorageClientFromEnv(); // lê BLOB_READ_WRITE_TOKEN
const bucket = storage.from("ingestion-originals");

await bucket.upload(path, fileOrBuffer, { contentType, upsert: false });
const bytes = await bucket.download(path); // Uint8Array
const url = await bucket.getSignedUrl(path, 3600); // URL GET temporária
await bucket.remove(path); // ou remove([path1, path2])
const { objects, isTruncated, nextContinuationToken } = await bucket.list(prefix);
```

Detalhes que importam na hora de usar:

- `upsert: false` vira `allowOverwrite: false` no `put`. Quem recusa é o
  servidor, na mesma operação da escrita — é atômico, ao contrário do
  HEAD-antes-do-PUT do backend anterior. A recusa chega como `StorageError`
  com `code: "already_exists"` e `status: 409`.
- `download` lança `StorageError` 404/`not_found` quando o caminho não
  existe (o SDK devolveria `null`).
- `remove` aceita lista e resolve tudo numa chamada só. Remover o que já não
  existe é sucesso, não erro.
- `list` devolve uma página por chamada; use `nextContinuationToken` para
  paginar. As chaves voltam sem o prefixo do bucket.
- `getSignedUrl` custa uma ida à rede (pedir a delegação ao Vercel Blob); a
  assinatura da URL em si é local.

Toda falha chega como `StorageError` (`message`, `status`, `code`, `key`). A
tradução dos erros do SDK está em `packages/storage/src/errors.ts`;
`packages/storage/src/client.test.ts` é a especificação executável do
comportamento, rodando contra uma loja de blobs em memória
(`packages/storage/src/test-support/fake-blob-api.ts`) — sem rede.

## Verificação (smoke test)

Com o token no ambiente (nunca commitado):

```bash
cat > /tmp/storage-smoke.ts <<'EOF'
import { createStorageClientFromEnv } from "@sbf/storage";

const bucket = createStorageClientFromEnv().from("ingestion-originals");
await bucket.upload("smoke-test.txt", "conteudo de teste", { contentType: "text/plain" });
console.log(await bucket.getSignedUrl("smoke-test.txt", 60));
await bucket.remove("smoke-test.txt");
console.log("ok");
EOF

# de dentro do monorepo, pra resolver o workspace "@sbf/storage"
bun run /tmp/storage-smoke.ts
```

A URL impressa deve abrir num navegador e mostrar "conteudo de teste"; um
minuto depois, a mesma URL deve deixar de funcionar. Isso confirma token,
loja e privacidade de uma vez só.

## Fora de escopo deste documento

- **Upload direto do navegador.** Hoje todo upload passa por Server Action
  (`uploadDocument` em `apps/web/src/app/actions/ingestion.ts`), que é o que
  mantém o token no servidor. O Vercel Blob suporta upload direto do cliente
  com token de escrita de uso único (`@vercel/blob/client`), mas isso não
  está implementado aqui — quem for adotar precisa de uma rota que emita o
  token e valide o formato antes.
- **Limpeza de blobs órfãos.** `deleteSourceDocument` remove o arquivo junto
  com o registro; não há varredura periódica procurando blob sem linha
  correspondente no Postgres.
