# Storage S3-compatível (SeaweedFS)

> 🤚 O provisionamento real na VPS — criar os buckets, gerar a credencial
> dedicada, publicar o bloco nginx, reiniciar o `weed s3` com
> `-s3.externalUrl` — precisa do acesso SSH do CEO; esta sessão não conecta
> na VPS (regra do repo). O que está pronto: o pacote `@sbf/storage`
> (`packages/storage/`), testado contra um servidor HTTP fake local
> (`packages/storage/src/client.test.ts`) — sem depender de rede externa nem
> de um SeaweedFS de verdade. Este doc é a receita para o CEO aplicar na
> VPS. Trocar os call sites que hoje usam `supabase.storage` em
> `apps/web`/`workers`/`apps/mcp-server` para consumir esta API é trabalho
> de outra sessão — fora do escopo daqui.

## O que é

O Supabase Storage está saindo do projeto (ver contexto da migração para
Neon/Pluggy). No lugar, os arquivos (boletos, faturas, comprovantes — bucket
`ingestion-originals`, até 50 MiB; CSV exportado — bucket `user-files`) vão
para o **SeaweedFS** que já roda na VPS do CEO, com API S3 exposta em
`127.0.0.1:9000` atrás de nginx. `@sbf/storage` é o cliente desse S3: assina
cada requisição com AWS SigV4 via [`aws4fetch`](https://github.com/kotx/aws4fetch)
(lib pequena, roda em Node e em edge — ao contrário de `@aws-sdk/client-s3`,
que é pesado e não é o que este pacote usa).

## Variáveis de ambiente

| Variável               | Obrigatória       | Descrição                                                                                                                                                                |
| ---------------------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `S3_ENDPOINT`          | sim               | URL pública e com HTTPS do S3, ex. `https://s3.chico-figueiredo.com.br`. **Nunca** `127.0.0.1:9000` fora da própria VPS — ver a nota de rede abaixo.                     |
| `S3_ACCESS_KEY_ID`     | sim               | Access key da credencial **dedicada** do app (não a de admin do SeaweedFS).                                                                                              |
| `S3_SECRET_ACCESS_KEY` | sim               | Secret key correspondente.                                                                                                                                               |
| `S3_REGION`            | não (`us-east-1`) | SigV4 exige uma região; SeaweedFS não valida o valor, mas precisa bater entre cliente e servidor.                                                                        |
| `S3_FORCE_PATH_STYLE`  | não (`true`)      | SeaweedFS só aceita path-style (`endpoint/bucket/chave`). Só o literal `"false"` desliga — hoje isso lança erro explícito no pacote, virtual-host não está implementado. |

Lidas por `loadStorageConfigFromEnv()` (`packages/storage/src/env.ts`), que
lança um erro descrevendo o nome da variável ausente em vez de deixar a
primeira chamada à API falhar com um 403 confuso.

## Por que expor 127.0.0.1:9000 pra internet — e por que isso é seguro

O SeaweedFS hoje escuta só em `127.0.0.1:9000` na VPS. **A Vercel não
alcança localhost da VPS** — não tem VPN nem rede privada entre os dois. Pra
app rodando na Vercel gravar e ler comprovantes, o endpoint S3 precisa ficar
acessível pela internet, atrás de um domínio próprio.

Isso é seguro **desde que** três coisas estejam em vigor, porque o protocolo
S3 autentica **cada requisição individualmente** por assinatura SigV4 — não
é como abrir uma porta de admin sem senha:

1. **TLS** — sem isso a assinatura e o corpo do arquivo trafegam em claro.
   O bloco nginx abaixo já força HTTPS.
2. **Credencial dedicada, não a de admin** — se ela vazar (log, variável de
   ambiente exposta, etc.), o dano fica limitado aos dois buckets do app,
   não ao SeaweedFS inteiro.
3. **Restrição de bucket na credencial** — a credencial do app só tem ação
   `Read`/`Write`/`List` em `ingestion-originals` e `user-files`, nunca
   `Admin` nem acesso a outros buckets que porventura existam na mesma
   instância. Isso é configurado no SeaweedFS (`s3.configure -buckets=...`
   abaixo), não no nginx — nginx só faz proxy, quem decide o que uma
   credencial pode fazer é o próprio SeaweedFS.

Sem essas três, expor a porta é sim um risco real. Com elas, uma requisição
sem assinatura válida (ou assinada com uma credencial errada, ou pedindo uma
ação fora do escopo da credencial) é rejeitada pelo próprio SeaweedFS antes
de tocar em qualquer arquivo.

## Bloco nginx (proxy TLS para o S3 do SeaweedFS)

Substitua `s3.chico-figueiredo.com.br` pelo subdomínio real escolhido e os
caminhos de certificado pelos do certbot/TLS já usados na VPS (mesmo padrão
de [`pluggy-deploy.md`](./pluggy-deploy.md)). Só path-style está coberto —
`@sbf/storage` não implementa virtual-host (`bucket.s3.dominio.com`), então
não há necessidade do regex de subdomínio-por-bucket que a
[doc oficial do SeaweedFS](https://github.com/seaweedfs/seaweedfs/wiki/S3-Nginx-Proxy)
mostra para quem precisa dos dois modos.

```nginx
# Redireciona HTTP -> HTTPS. TLS não é opcional aqui: a credencial do app
# viaja em cada requisição.
server {
    listen 80;
    server_name s3.chico-figueiredo.com.br;
    return 301 https://$host$request_uri;
}

upstream seaweedfs_s3 {
    server 127.0.0.1:9000;
    keepalive 20;
    # Se um dia isso virar mais de uma instância de weed s3, considere
    # `hash $arg_uploadId consistent;` aqui para manter as partes de um
    # mesmo multipart upload no mesmo backend. Com uma instância só, não
    # faz diferença.
}

server {
    listen 443 ssl;
    http2 on;
    server_name s3.chico-figueiredo.com.br;

    ssl_certificate     /etc/letsencrypt/live/s3.chico-figueiredo.com.br/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/s3.chico-figueiredo.com.br/privkey.pem;

    # 60 MiB de folga sobre o teto de 50 MiB de app (boletos/faturas/comprovantes)
    # definido em ingestion-originals — ajuste se um novo tipo de arquivo
    # maior for suportado.
    client_max_body_size 60m;

    # Diretivas abaixo seguem a receita oficial do SeaweedFS para nginx:
    # streaming sem buffer é o que evita corromper corpo/assinatura em
    # uploads grandes, e os X-Forwarded-* são o que permite ao SeaweedFS
    # validar a assinatura SigV4 contra o host público (ver -s3.externalUrl
    # na seção seguinte) em vez do localhost:9000 interno.
    ignore_invalid_headers off;
    proxy_buffering off;
    proxy_request_buffering off;
    chunked_transfer_encoding off;
    proxy_http_version 1.1;
    proxy_connect_timeout 300;

    proxy_set_header Host $host;
    proxy_set_header Connection "";
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $remote_addr;
    proxy_set_header X-Forwarded-Host $host;
    proxy_set_header X-Forwarded-Port $server_port;
    proxy_set_header X-Forwarded-Proto $scheme;

    location / {
        proxy_pass http://seaweedfs_s3;
    }
}
```

### Ajustar o `weed s3` para rodar atrás do proxy

Sem isso, o SeaweedFS valida a assinatura SigV4 contra `localhost:9000` (o
endereço que ele mesmo enxerga), que não bate com o host público que o
cliente usou para assinar — toda requisição vinda pelo nginx falharia com
erro de assinatura. Onde quer que o processo `weed s3` seja iniciado hoje
(systemd, docker compose, supervisor — o que já estiver rodando na VPS),
acrescente a flag e reinicie:

```bash
weed s3 -port=9000 -s3.externalUrl=https://s3.chico-figueiredo.com.br ...(demais flags já em uso)
# equivalente via variável de ambiente, se preferir não mexer na linha de comando:
# WEED_S3_EXTERNAL_URL=https://s3.chico-figueiredo.com.br
```

## Criar os buckets e a credencial dedicada

Rodar na VPS, via `weed shell` (conecta por padrão no master local):

```bash
weed shell
```

Dentro do shell:

```
> s3.bucket.create -name=ingestion-originals
> s3.bucket.create -name=user-files
```

Gerar um access key e um secret key fortes (fora do `weed shell`, em
qualquer terminal):

```bash
openssl rand -hex 12    # access key
openssl rand -base64 32 # secret key
```

De volta ao `weed shell`, criar a credencial **dedicada** (nome `sbf-app`,
sem a ação `Admin`, restrita aos dois buckets do app — ver a seção de
segurança acima). Cole os valores gerados no lugar de `<...>`:

```
> s3.configure -user=sbf-app -access_key=<access-key-gerado> -secret_key=<secret-key-gerado> -buckets=ingestion-originals,user-files -actions=Read,Write,List -apply
```

`-apply` já ativa a credencial sem precisar reiniciar o `weed s3`. Se por
algum motivo a credencial nova não autenticar de primeira, reinicie o
processo `weed s3` e teste de novo — comportamento pode variar por versão
do SeaweedFS instalada na VPS.

Guarde `access-key-gerado`/`secret-key-gerado` como `S3_ACCESS_KEY_ID` /
`S3_SECRET_ACCESS_KEY` (Vercel + qualquer worker que rode fora dela).

## Verificação (smoke test)

Com as variáveis exportadas localmente (nunca commitadas):

```bash
export S3_ENDPOINT=https://s3.chico-figueiredo.com.br
export S3_ACCESS_KEY_ID=...
export S3_SECRET_ACCESS_KEY=...

cat > /tmp/storage-smoke.ts <<'EOF'
import { createStorageClientFromEnv } from "@sbf/storage";

const bucket = createStorageClientFromEnv().from("ingestion-originals");
await bucket.upload("smoke-test.txt", "conteudo de teste", { contentType: "text/plain" });
console.log(await bucket.getSignedUrl("smoke-test.txt", 60));
await bucket.remove("smoke-test.txt");
console.log("ok");
EOF

# rodar de dentro do monorepo, pra resolver o workspace "@sbf/storage"
bun run /tmp/storage-smoke.ts
```

A URL impressa deve abrir num navegador comum (fora da rede da VPS) e
mostrar "conteudo de teste" — isso confirma DNS, TLS, `-s3.externalUrl` e a
credencial de uma vez só.

## API do pacote (referência rápida)

```ts
import { createStorageClientFromEnv } from "@sbf/storage";

const storage = createStorageClientFromEnv(); // lê S3_* de process.env
const bucket = storage.from("ingestion-originals");

await bucket.upload(path, fileOrBuffer, { contentType, upsert: false });
const bytes = await bucket.download(path); // Uint8Array
const url = await bucket.getSignedUrl(path, 3600); // URL GET temporária
await bucket.remove(path); // ou remove([path1, path2])
const { objects, isTruncated } = await bucket.list(prefix);
```

Erros de storage chegam como `StorageError` (`status`, `code`, `key`) — ver
JSDoc em `packages/storage/src/errors.ts`, `client.ts` e `types.ts` para o
contrato completo (inclusive a ressalva de que `upsert: false` é HEAD-antes-
do-PUT, não atômico, e que `remove` é sequencial por chave, não o
batch-delete do S3). `packages/storage/src/client.test.ts` é a especificação
executável do comportamento contra respostas S3 reais (sucesso, 404, 403,
409, 5xx com retry).

## Fora de escopo deste documento

- Provisionamento real na VPS (buckets, credencial, nginx, restart do
  `weed s3`) — 🤚 depende do acesso SSH do CEO.
- Trocar `supabase.storage.from(...)` pelos métodos de `@sbf/storage` nos
  call sites atuais (`apps/web/src/app/actions/ingestion.ts`,
  `apps/web/src/components/ai-chat-drawer.tsx` e `upload-documents.tsx`,
  `apps/web/src/app/dashboard/import/page.tsx`, `apps/mcp-server`,
  `workers/ingestion`, `workers/gmail-scanner`, `workers/local-scanner`) —
  outra sessão, consumindo a API acima.
- Upload direto do navegador: `ai-chat-drawer.tsx` e `upload-documents.tsx`
  hoje sobem o arquivo direto do browser para o Supabase Storage (RLS +
  anon key). Um S3 genérico não tem equivalente seguro a isso sem URL
  pré-assinada de **PUT** (não implementada aqui — só GET, para exibição).
  Quem for trocar esses dois call sites vai precisar decidir entre gerar um
  presigned PUT ou rotear o upload por uma Server Action.
- Batch-delete real (`POST /?delete`) e paginação automática de `list` —
  `remove` é sequencial e `list` devolve uma página por chamada
  (`nextContinuationToken` para quem precisar paginar manualmente).
