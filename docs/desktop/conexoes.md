# Conexões do desktop

Cada conexão traz documentos e mensagens de fora para o cofre e os passa pelas regras de
reconhecimento (spec 13), na mesma esteira do arquivo solto na janela. Nada vai para o
planejamento por conta delas: as regras só propõem.

## Gmail

Crate `crates/sbf-gmail`. Reaproveita o cliente OAuth2 do `sbf-coletor` (o mesmo do
`sbf coletor-vps gmail`, que leva o Gmail para a VPS); o cliente fica atrás do trait
`FonteDeGmail`, e os testes usam um cliente de mentira com e-mails fictícios.

### Comando

```text
sbf gmail coletar --inventario <cofre> [--consulta "<query do Gmail>"] [--desde AAAA-MM-DD] [--limite N]
```

- **Consulta padrão:** `has:attachment newer_than:30d -category:promotions`. `--consulta`
  troca (a mesma sintaxe da caixa de busca do Gmail). Para pegar avisos sem anexo (um aviso de
  boleto), use uma consulta sem `has:attachment`, por exemplo `from:cobranca@banco.exemplo
newer_than:30d`.
- **Checkpoint:** o `internalDate` da última mensagem processada em ordem, guardado em
  `varredura.cursor` (fonte `gmail` do `inventario.sqlite`). A próxima coleta acrescenta
  `after:<segundos>` (com uma hora de folga; origem já vista não baixa nada). `--desde`
  ignora o checkpoint. Uma mensagem que falha congela o checkpoint nela: as outras seguem e
  a próxima coleta tenta de novo.
- **`--limite`:** mensagens por coleta (padrão 200), das mais velhas para as mais novas; o
  resto fica para a próxima.
- A saída mostra só contagens (mensagens, anexos novos, documentos novos, repetidos,
  reconhecidos, ambíguos, sem regra, falhas). Nunca assunto, remetente ou texto.

### O que entra no cofre

- **Anexo** aceito (PDF, imagens, XML de NF-e, CSV, OFX, XLSX; a imagem embutida na assinatura,
  com `Content-ID`, fica de fora): salvo em `<cofre>/entrada/gmail/<sha12>-<nome>`, com
  permissão 0600 na pasta 0700. Vira **achado** (camada A, `caminho_acervo` =
  `entrada/gmail/...`) e **origem** com `origin_type = gmail`, `origin_key =
gmail:<id da mensagem>:<id da parte>`, `mensagem_ref = anexo` e as colunas `gmail_*`
  (mensagem, conversa, parte, remetente, assunto, data).
- **Dedup por sha256:** o mesmo PDF em dois e-mails é um achado com duas origens, e o
  reconhecimento roda uma vez só (no conteúdo novo). Coletar de novo não duplica nada.
- **E-mail sem anexo aceito** (o aviso de boleto): vira uma entrada do tipo `Mensagem`. O
  assunto e o corpo (`text/plain`, ou o HTML sem marcas) são guardados num `.txt` em
  `entrada/gmail/` (origem `gmail:<id>:corpo`, `mensagem_ref = eml`) e passam pelas regras
  como `TipoDeEntrada::Mensagem`, com `origem.canal = gmail`. Uma regra precisa declarar
  `aplica_a: [mensagem]` para ver essas entradas.
- Os documentos novos passam por `ReconhecedorDoCofre::reconhecer_de(Gmail, id da mensagem)`:
  as seis etapas de sempre (o texto do PDF e do XML vai para a tabela `texto`), com as regras
  vendo `origem.canal = gmail` e `origem.id_externo = <id da mensagem>`. Sem IA.

### Credenciais

Variáveis de ambiente, as mesmas do coletor da VPS (nunca por argumento, que aparece em `ps`):
`GOOGLE_MAIL_CLIENT_ID`, `GOOGLE_MAIL_CLIENT_SECRET`, `GMAIL_REFRESH_TOKEN`. O programa as lê
na hora de coletar e nunca as grava, loga nem mostra em erro (só os **nomes** das que faltam).
Sem elas, `sbf gmail coletar` sai com código 2 dizendo quais faltam; o worker e o MCP nem
ligam a coleta.

Para o worker (serviço do usuário), a unit precisa enxergar as variáveis: por exemplo
`systemctl --user edit sbf-worker` com `Environment=` ou `EnvironmentFile=` apontando para um
arquivo seu fora do repositório, ou `systemctl --user import-environment`.

### Worker

`worker.json` (pasta de dados do app), `gmail_minutos`: de quantos em quantos minutos o
worker coleta (padrão 30; `0` desliga). A primeira coleta é logo ao iniciar. O log traz
`gmail mensagens=N novos=N`; uma falha vira `falhas` no estado do worker, sem conteúdo.

### MCP

`gmail_coletar` (`consulta`, `desde`, `limite`, todos opcionais): coleta agora, **aplica
direto** (D15) e devolve só contagens. Faz o backup do inventário e do `regras.sqlite` antes
da primeira escrita da sessão. Só aparece quando as credenciais estão no ambiente do
servidor MCP.

### Para a área Conexões

A área Conexões da janela (`sbf-janela`, `conexoes_ui.rs`) já mostra o Gmail num cartão próprio, ao lado do Telegram, lendo `sbf_gmail::estado_do_gmail`.

```rust
// sbf_gmail
pub fn estado_do_gmail(cofre: &Path) -> EstadoDaConexao;

pub struct EstadoDaConexao {
    pub situacao: SituacaoDaConexao,   // SemCredenciais | Pronta | Coletada | ComFalha
    pub faltando: Vec<String>,         // nomes das variáveis que faltam
    pub ultima_coleta: Option<String>, // ISO 8601 UTC da última coleta que terminou bem
    pub consulta: Option<String>,      // a consulta da última coleta
    pub origens: u64,                  // anexos e textos de e-mail no inventário
    pub documentos: u64,               // documentos distintos vindos do Gmail
    pub ultima_falha: Option<String>,  // categoria da última falha, sem conteúdo
}
```

Só lê (as variáveis e o `inventario.sqlite`); não toca na rede. Para coletar de dentro da
janela, chame `sbf_gmail::coletar` (síncrono, cancelável por `AtomicBool`) numa thread, como o
`sbf gmail coletar` faz.

## Telegram: a sua conta (e o estado das outras fontes)

A área **Conexões** (Fontes › Conexões) mostra o que alimenta o inventário por conta própria.
A primeira conexão de verdade é o **Telegram pela sua conta** (MTProto, crate `sbf-telegram`,
biblioteca `grammers`): o SBF entra como um aplicativo conectado, **só lê** e baixa as fotos, os
documentos e os PDFs das conversas que você escolher. Nunca envia, apaga, entra em grupo nem
marca como lido (ADR-017). O bot do Telegram (`sbf canais`, na VPS) é outra coisa: leva só
avisos e recebe documentos mandados a ele.

### O que você faz uma vez

1. Em `my.telegram.org` › API development tools, crie um aplicativo e anote o `api_id` e o
   `api_hash`. No `.env` da raiz do repositório (ou no ambiente):

   ```
   TELEGRAM_API_ID=...
   TELEGRAM_API_HASH=...
   ```

   O programa lê só essas duas chaves, em tempo de execução. Depois do primeiro `entrar` elas
   ficam também no chaveiro, para o worker (que não tem `.env`) achá-las.

2. No terminal: `sbf telegram entrar`. Pede o número (com o código do país), o código que o
   Telegram manda **ao app** (não por SMS) e, se a conta tem, a senha da verificação em duas
   etapas (sem eco). Nada disso é guardado: só a chave de autorização que resulta.
3. `sbf telegram conversas` lista as conversas (id, tipo, nome). `sbf telegram seguir <id|nome>`
   escolhe as que entram. **Padrão: nenhuma.** (Na janela, a mesma escolha é uma caixa por
   conversa.)

### Comandos

| Comando                                                                                  | O que faz                                                           |
| ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| `sbf telegram entrar`                                                                    | Login interativo (número, código, senha 2FA). Quem roda é você.     |
| `sbf telegram sair`                                                                      | Encerra a sessão no Telegram e apaga a guardada.                    |
| `sbf telegram conversas`                                                                 | Lista as conversas da conta (e atualiza a lista que a janela usa).  |
| `sbf telegram seguir <id\|nome>` / `deixar <id\|nome>`                                   | Passa a coletar / deixa de coletar.                                 |
| `sbf telegram coletar --inventario <cofre> [--desde AAAA-MM-DD] [--conversa <id\|nome>]` | Coleta já. `--desde` vale só na primeira coleta de cada conversa.   |
| `sbf telegram estado`                                                                    | O que a janela mostra: situação, conversas seguidas, última coleta. |

### O que a coleta faz

Para cada conversa seguida, mídias com id maior que o **checkpoint** da conversa (o último id
tratado), da mais antiga para a mais nova, no máximo 200 por conversa e por rodada:

1. baixa para `<cofre>/entrada/telegram/<nome>-<id>/<mensagem>-<arquivo>` (foto vira
   `foto-<mensagem>.jpg`; áudio, vídeo, figurinha e arquivo acima de 100 MB ficam de fora);
2. registra no inventário como **achado** (por sha256) com origem `telegram:<conversa>:<mensagem>`:
   o mesmo arquivo em outra mensagem é só **uma origem a mais** (e a cópia repetida some da
   `entrada/`);
3. roda o **Reconhecimento** em cada arquivo **novo**, com as regras de reconhecimento. O arquivo
   chega com `canal = telegram` (uma regra pode exigir `{"canal": ["telegram"]}`). Sem IA: o que
   o leitor local e as regras não leem fica para a triagem;
4. avança o checkpoint. Uma falha numa mídia (expirou, disco) para só aquela conversa e deixa o
   checkpoint antes dela; a próxima rodada tenta de novo. Perder o checkpoint só custa reler:
   a origem única e o sha256 evitam duplicar.

### Onde ficam as coisas

| O quê                                 | Onde                                                                                     |
| ------------------------------------- | ---------------------------------------------------------------------------------------- |
| Sessão MTProto (chave de autorização) | **Só o chaveiro** (`mensageiro:telegram`). Nunca arquivo solto, log ou mensagem de erro. |
| `api_id` / `api_hash`                 | Ambiente, `.env` da raiz e (depois do `entrar`) chaveiro (`mensageiro:telegram-api`).    |
| Conversas seguidas                    | `telegram.json`, na pasta de dados do app (fora do cofre).                               |
| Checkpoint, última coleta, contagens  | `telegram-estado.json`, na pasta de dados do app.                                        |
| Última lista de conversas             | `telegram-conversas.json`, na pasta de dados do app.                                     |
| Mídias baixadas                       | `<cofre>/entrada/telegram/`.                                                             |

### Worker, MCP e janela

- **Worker:** coleta as conversas seguidas a cada `telegram_minutos` (padrão **15**, `0`
  desliga) do `worker.json`. Sem conversa seguida ou sem sessão guardada, não faz nada. Sessão
  recusada pelo Telegram conta como falha do passo `telegram` e a janela passa a pedir
  `sbf telegram entrar`.
- **MCP:** `telegram_conversas` (lista; `atualizar=true` pergunta ao Telegram), `telegram_seguir`
  (segue ou, com `deixar=true`, deixa) e `telegram_coletar` (coleta já; `conversa` e `desde`
  opcionais). Aplicam direto (D15). O nome das conversas vai ao cliente MCP: ele nasce
  mascarado (ADR-015), mas esses nomes não passam por esse mascaramento.
- **Janela:** Conexões lista o Telegram (situação, conversas seguidas, última coleta, arquivos),
  com **Coletar agora** e uma caixa Seguir/Deixar por conversa; a Pluggy e as pastas do worker
  aparecem só com o estado (última varredura e quantos itens o inventário guarda). O login pela
  janela ainda não existe: ela mostra "rode `sbf telegram entrar` no terminal".

### Limites conhecidos

- Só lê o que a conta vê. Mensagens de texto não entram (só mídia); o texto financeiro de
  conversas continua pelo caminho do export (`docs/desktop/mensageiros.md`).
- Conta nova no Telegram não se cria por aqui (o Telegram exige o app oficial).
- A sessão aparece na lista de dispositivos da conta no Telegram; `sbf telegram
sair` o remove.
- Nada foi testado contra a conta real nesta entrega: os testes usam uma fonte falsa. A primeira
  coleta de verdade é com você, depois do `entrar`.
