# Lições do inventário

Aprendidas rodando o inventário de verdade (11/11/2026). Todo script novo deste
worker segue estas regras; cada uma custou uma falha real.

## Banco (SQLite do cofre)

- **Vários processos gravam ao mesmo tempo** (um scanner por disco, Gmail,
  nuvem). Todo acesso usa `abrirBanco()` (WAL, `busy_timeout`). No `sqlite3` de
  linha de comando, comece com `.timeout 60000`, senão dá `database is locked`.
- **Rodada parcial não substitui a completa.** Uma fonte tem várias varreduras
  (completa, retomada, só áreas A). Relatório agrega por fonte em vez de pegar
  a última.
- **Nunca deixe uma transação aberta atravessando `await`.** Com outro processo
  gravando, a leitura dentro dela fixa um snapshot e a escrita seguinte falha com
  `SQLITE_BUSY_SNAPSHOT`, que o `busy_timeout` não resolve. Junte resultados em
  memória e grave em lotes curtos com `db.transaction(fn).immediate(...)`.
- **Esquema evolui por `CREATE TABLE IF NOT EXISTS` e `garantirColuna`.** Tabela
  nova vai no `schema.sql`; coluna nova em tabela existente, em `aplicarSchema`.

## Arquivos e cofre

- **Escrita no cofre é atômica:** grava em `<destino>.parcial-<pid>` e renomeia.
  Dois discos copiavam o mesmo conteúdo ao mesmo tempo e um via o arquivo do
  outro pela metade.
- **Nunca apagar cópia sem confirmar o original** (existe e tem o mesmo
  tamanho). O original nunca é tocado.
- **Faxina não corre atrás de quem está escrevendo.** Coletor grava o arquivo
  antes de registrar no índice; um "arquivo sem dono" pode ser só o próximo
  registro. Limpeza ignora `.parcial-*` e tudo com menos de uma hora.
- **Arquivo grande (> 100 MB) não é copiado**, só hasheado.
- **NTFS (ntfs3) lista nomes que não abre** (acentos e símbolos de arquivos
  vindos do Windows): `ENOENT` no `stat` de algo que o `readdir` acabou de
  mostrar. Conta como erro de leitura, não como sumiço; confira se o conteúdo
  veio por um espelho.
- **Pasta sendo escrita por outro processo fica de fora** (`--pular`) até
  terminar, ou o hash pega arquivo pela metade.

## Processos

- **`pkill -f padrão` mata o próprio shell** se o padrão aparece na linha de
  comando. Ancore no executável: `pkill -f '^bun run workers/inventario/src/x'`.
  **Todos** os padrões do comando, não só o primeiro (aconteceu duas vezes).
  E o executável pode vir pelo caminho completo (`/home/chico/.bun/bin/bun run …`),
  que `^bun run` não pega: use `'^(/[^ ]*/)?bun run workers/inventario/src/x'`.
- **SIGTERM só é atendido quando o laço cede ao event loop.** Laço síncrono
  longo (contagem de milhões de arquivos) ignora o sinal; ceda com
  `await Bun.sleep(0)` de tempos em tempos ou aceite que vai precisar de SIGKILL
  (e então o código precisa tolerar trabalho pela metade).
- **Retomada por idempotência:** toda origem tem `origin_key` única; reexecutar
  pula o que já viu. Falha transitória vai para a tabela `refazer`, não se perde
  quando o cursor avança.
- **Ordem da E3:** `e3-triar`, `e3-pluggy`, `e3-extrair` (pdftotext), `e3-ocr`
  (tesseract nas imagens que a triagem marcou "precisa de OCR"), depois
  `e3-triar --duvidas-com-texto`, `reparar-acervo` e `vista-ciclos`.
- **Passada que roda a cada 30 min não pode acumular nota.** A triagem anexava o
  mesmo motivo a cada rodada, até 31 vezes na mesma linha. Nota igual à que já
  existe não entra de novo.
- **Regra de texto boa para PDF falha no OCR:** a frase "comprovante de pix"
  sai quebrada em linhas e "R$" vira "RS". Conte marcas (valor, destino, chave,
  id, instituição…) e limite a texto curto: manuais e FAQs longos juntam as
  mesmas marcas à toa. Simule (`--simular`) antes de gravar.

## APIs externas

- **Gmail estoura cota com 403 "Quota exceeded", não só 429.** Ritmo fixo (uma
  chamada a cada 50 ms) e espera de 65 s no 403/429.
- **Toda chamada de rede tem tempo limite.** Um `fetch` pendurado parou o coletor
  do Gmail sem erro nenhum; o processo seguia vivo e o cursor, parado. 401 no meio
  da coleta é token de acesso vencido: tente de novo.
- **Token OAuth do Google em app de teste vence.** `get:gmail-token --gravar`
  grava nos `.env`.
- **Bun carrega `.env.local` por cima do `.env`.** Trocar segredo só no `.env`
  não tem efeito se o `.env.local` tiver a mesma variável.

## Código e testes

- **Lógica pura em módulo sem `bun:sqlite` nem `Bun.*`**, para o vitest (que
  roda em Node) conseguir testar. O script com efeitos importa o módulo puro.
- **Teste de cripto usa parâmetros baratos;** o custo de produção estoura o
  timeout quando a máquina está ocupada.
- **Prettier e ESLint só nos arquivos que você mexeu** quando há outros agentes
  trabalhando na mesma árvore.
- **commitlint:** assunto em minúsculas, linhas do corpo até 100 caracteres.
- **Painel: não apague snapshot recente.** Aba em segundo plano fica presa numa
  versão antiga dos dados por muito tempo e passa a ver 404 em tudo quando o
  arquivo some. Guardar só o anterior não bastou: mantenha os últimos 48 (24 h,
  ~1 MB) e apague só o que passar disso (lista em `scratchpad/painel/blobs.txt`).
  Em 12/11 a página gravou 404 em `runs` de 15 s a 4 min depois de cada
  atualização dos datasets, com o blob novo presente na lista de assets e
  legível. Confirmado que era uma aba velha: uma abertura nova às 14:14 UTC
  gravou o documento `page` em `data/users/me` com os dados do snapshot certo.
  Para saber se a página funciona, olhe `page` (o que a última abertura
  desenhou), não só `runs`.
- **Id de elemento igual ao id de um dataset confunde o painel** (a marca da
  tabela acusava "no dataset" com o dataset carregando): use ids distintos.
- **Nada de nomes de arquivo, valores ou documentos no painel do claude.ai:** lá
  vão só contagens. O detalhe se vê localmente (Datasette).
