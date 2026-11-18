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
