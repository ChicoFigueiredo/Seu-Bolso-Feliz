# Conexões: o Telegram da sua conta (e o estado das outras fontes)

A área **Conexões** (Fontes › Conexões) mostra o que alimenta o inventário por conta própria.
A primeira conexão de verdade é o **Telegram pela sua conta** (MTProto, crate `sbf-telegram`,
biblioteca `grammers`): o SBF entra como um aplicativo conectado, **só lê** e baixa as fotos, os
documentos e os PDFs das conversas que você escolher. Nunca envia, apaga, entra em grupo nem
marca como lido (ADR-017). O bot do Telegram (`sbf canais`, na VPS) é outra coisa: leva só
avisos e recebe documentos mandados a ele.

## O que você faz uma vez

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

## Comandos

| Comando                                                                                  | O que faz                                                           |
| ---------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| `sbf telegram entrar`                                                                    | Login interativo (número, código, senha 2FA). Quem roda é você.     |
| `sbf telegram sair`                                                                      | Encerra a sessão no Telegram e apaga a guardada.                    |
| `sbf telegram conversas`                                                                 | Lista as conversas da conta (e atualiza a lista que a janela usa).  |
| `sbf telegram seguir <id\|nome>` / `deixar <id\|nome>`                                   | Passa a coletar / deixa de coletar.                                 |
| `sbf telegram coletar --inventario <cofre> [--desde AAAA-MM-DD] [--conversa <id\|nome>]` | Coleta já. `--desde` vale só na primeira coleta de cada conversa.   |
| `sbf telegram estado`                                                                    | O que a janela mostra: situação, conversas seguidas, última coleta. |

## O que a coleta faz

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

## Onde ficam as coisas

| O quê                                 | Onde                                                                                     |
| ------------------------------------- | ---------------------------------------------------------------------------------------- |
| Sessão MTProto (chave de autorização) | **Só o chaveiro** (`mensageiro:telegram`). Nunca arquivo solto, log ou mensagem de erro. |
| `api_id` / `api_hash`                 | Ambiente, `.env` da raiz e (depois do `entrar`) chaveiro (`mensageiro:telegram-api`).    |
| Conversas seguidas                    | `telegram.json`, na pasta de dados do app (fora do cofre).                               |
| Checkpoint, última coleta, contagens  | `telegram-estado.json`, na pasta de dados do app.                                        |
| Última lista de conversas             | `telegram-conversas.json`, na pasta de dados do app.                                     |
| Mídias baixadas                       | `<cofre>/entrada/telegram/`.                                                             |

## Worker, MCP e janela

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

## Limites conhecidos

- Só lê o que a conta vê. Mensagens de texto não entram (só mídia); o texto financeiro de
  conversas continua pelo caminho do export (`docs/desktop/mensageiros.md`).
- Conta nova no Telegram não se cria por aqui (o Telegram exige o app oficial).
- A sessão aparece na lista de dispositivos da conta no Telegram; `sbf telegram
sair` o remove.
- Nada foi testado contra a conta real nesta entrega: os testes usam uma fonte falsa. A primeira
  coleta de verdade é com você, depois do `entrar`.
