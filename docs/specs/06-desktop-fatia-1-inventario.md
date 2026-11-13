# 06 — Spec: Desktop, fatia 1 — Inventário no desktop

> **Estado: ✅ aprovada pelo CEO em 2026-11-13, junto com o protótipo (versão 3).**
> Implementação em andamento. Escrita em 2026-11-12 a partir do grill do mesmo dia.
> Decisões em ADR-012 a ADR-018 (`docs/arquitetura/`), vocabulário em `CONTEXT.md`. Fatos
> do inventário atual verificados contra o código de `workers/inventario/` no mesmo dia.

## Objetivo

O desktop Rust abre o inventário e o cofre que já existem (`inventario.sqlite`, o
acervo) e vira o lugar onde o CEO **revisa e decide**.

A fatia substitui o Datasette e o "editar o SQLite na mão", e acrescenta:

- o **chaveiro**;
- a abertura de **PDF protegido**;
- os **lotes** aprovados para promoção;
- a **estante** e a **cópia de segurança do cofre** no Google Drive;
- os **relatórios do §11** sobre os registros, com a marca "a conferir". É a primeira
  resposta de gerenciamento financeiro, antes do livro-caixa chegar na fatia 2.

A shell nasce com o produto inteiro no painel de navegação (ver "O produto inteiro na
navegação"), para não ser redesenhada a cada fatia.

Os coletores continuam em Bun por enquanto. Cada um é absorvido numa fatia 1b, 1c… e,
pela regra do ADR-012, a versão TS é apagada no mesmo PR.

Hoje, verificado no código:

- **Nenhuma decisão do usuário é gravada.** A triagem é toda automática (`regras@2`).
- **Não existe promoção.**
- **Nenhum código escreve em `pendencia`, `regra_senha` ou `tentativa_senha`.**
- **Nenhum PDF protegido é aberto.** Eles só recebem a marca `protegido_senha=1`.

## Fora do escopo desta fatia

- Estação, sincronização, API, livro-caixa e caixa de entrada: fatias 2 e 3.
- **Enviar** um lote ao servidor: a fatia 1 só aprova.
- Modo `sbf worker`, agenda, conexões, alertas e canais: fatia 4. Aqui a estante e a
  cópia de segurança rodam sob demanda.
- Roteiros (fatia 5) e IA, RAG, pontos de atenção e MCP (fatia 6).
- Portar os coletores, a E3, o `reparar-acervo` e a `vista-ciclos`: fatias 1b em diante.

## Onde o código mora

Os identificadores são todos em português, sem acento, inclusive o mecânico (ADR-018).

### Repositório novo `ChicoFigueiredo/bancada`

Licença MIT OR Apache-2.0, `cargo deny` com a allowlist do Chess Studio. Nada aqui
depende de Bevy nem de domínio financeiro.

| Crate              | O que é                                                                                                                                                                                                                                                                                                                                                                                                | Vem de (Chess Studio)                                                                                                                              |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `bancada-casca`    | A shell. Ribbon, com modelo puro, vista egui, KeyTips e bastidores. Lista única de Ações, parametrizada pelo app. Mapa de atalhos e Paleta (Ctrl+K). Barra de Acesso Rápido e painel de navegação (novo). Dock com egui_tiles e painéis. Barra de status. Temas por tokens JSON, densidade, escala e i18n Fluent. Família de ícones plugável (o SBF usa Material Symbols Rounded e o Chess, Phosphor). | `chess-studio/src/{ribbon,dock.rs,panel_chrome.rs,status_bar.rs,palette.rs,ui_theme,i18n.rs,focus.rs}`, `chess-input/src/{actions.rs,controls.rs}` |
| `bancada-segredos` | Keyring do SO primeiro. Fallback em arquivo próprio, Argon2id + XChaCha20-Poly1305, formato versionado com AAD. `Segredo` com zeroize. Genérico sobre o rótulo.                                                                                                                                                                                                                                        | `chess-vault`                                                                                                                                      |
| `bancada-armazem`  | Abre SQLite com PRAGMAs (WAL, `busy_timeout`, `foreign_keys`). Escada de migrações por `PRAGMA user_version`, que recusa banco mais novo que o app. Transação IMMEDIATE curta. Tabela só de acréscimo com trigger que recusa UPDATE e DELETE.                                                                                                                                                          | `chess-store/src/{migrations.rs,store.rs}`                                                                                                         |

`bancada-rede` (portão de rede, cache, OAuth loopback) só entra na fatia 2.

### Neste monorepo, em `crates/`

O monorepo ganha um `Cargo.toml` na raiz, e `target/` vai para o `.gitignore`.

| Crate            | O que é                                                                                                                                                                         |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sbf`            | O binário. Na fatia 1 tem só o modo janela: `sbf [--inventario <pasta-do-cofre>]`.                                                                                              |
| `sbf-inventario` | Domínio e dados do inventário. Achado, origem, registro, decisão, lote, pendência e regra de senha. As migrações do `inventario.sqlite`, as projeções e as 12 vistas. Sem egui. |
| `sbf-chaveiro`   | O chaveiro do SBF sobre `bancada-segredos`. Importa o `segredos/cofre.enc` e guarda o CPF e as senhas por rótulo.                                                               |
| `sbf-pdf`        | A abertura de PDF protegido: estratégias, tentativas, cópia temporária e extração.                                                                                              |
| `sbf-estante`    | A estante e a cópia de segurança do cofre, via rclone.                                                                                                                          |
| `sbf-janela`     | A interface egui: áreas, tabelas, painéis e as Ações do SBF.                                                                                                                    |

O commitlint ganha os escopos `desktop` e `inventario`.

## A janela

O protótipo navegável é a referência visual (link na seção "Revisão"). A estrutura,
de cima para baixo:

1. **Barra de título**, com a Barra de Acesso Rápido (Desfazer decisão, Paleta e
   Atualizar).
2. **Ribbon** nos modos clássico, simples e recolhido (Ctrl+F1), com KeyTips (Alt). As
   abas:
   - **Arquivo**, que abre os bastidores:
     - Inventário: a pasta do cofre e o banco;
     - Chaveiro: estado, Importar `cofre.enc` e os rótulos, nunca os valores;
     - Estante e cópia de segurança: pasta no Drive e última execução;
     - Opções: os 7 temas do SBF, densidade e escala;
     - Sobre.
   - **Início:** Pesquisar, Período (ciclo atual, anterior ou intervalo), Filtros
     (estado do achado, tipo, fonte, quem decidiu) e Atualizar.
   - **Triagem**, com seis grupos:
     - Triagem: Documento, Descartar, Em dúvida;
     - Corrigir: Fornecedor, Categoria, Tipo, Data e valor;
     - Relacionar: Marcar duplicidade, Unir conciliados, Confirmar recorrência;
     - Interpretação: Aprovar, Rejeitar;
     - Pendência: Sinalizar, Responder;
     - Histórico: Desfazer, Ver decisões.
   - **Lotes:** Novo lote, Adicionar seleção, Remover e Aprovar lote. O botão Enviar ao
     servidor fica desabilitado, com o tooltip "Chega na fatia 3, com a caixa de
     entrada".
   - **Ferramentas:** Abrir PDF protegido, Regras de senha, Atualizar estante e Fazer
     cópia de segurança.
   - **Exibir:** uma caixa por painel (Detalhe, Origens, Texto extraído, Histórico de
     decisões, Prévia), Layout de tela, Tema e Densidade.
   - **Achado** (contextual, aparece quando há um selecionado): Abrir original, Mostrar
     no cofre, Ver texto, Abrir protegido e Copiar hash.
3. **Corpo**, em três partes:
   - **Painel de navegação** à esquerda, já com o produto inteiro (próxima seção).
     Nesta fatia funcionam o grupo Inventário e a área Relatórios. As outras áreas
     aparecem com o selo da fatia em que chegam e o motivo.
   - **Tabela densa** no centro, virtualizada.
   - **Dock** à direita, com Detalhe, Origens, Texto e Histórico de decisões.
4. **Barra de status:**
   - o inventário aberto e as contagens (achados, documentos, em dúvida, a triar);
   - o estado do chaveiro (aberto ou fechado);
   - o aviso de disco sem cifra (ADR-015);
   - a última cópia de segurança.

Os estados do achado aparecem como **A triar, Documento, Descartado e Em dúvida**, sempre
com quem decidiu: "você" ou "regras@2".

### Navegação relacional: todo nome é link

Regra da shell, decidida em 2026-11-12, que vale para o produto inteiro e não tela a
tela. O web de hoje não tem isso: a página de fornecedor das telas antigas não lista as
movimentações dele.

- **Todo nome é link, sem lista fechada:** toda entidade que tem identidade própria tem
  página. Hoje isso inclui:
  - **Dinheiro:** movimentação, transferência, conta, instituição, produto financeiro,
    cartão, fatura, ciclo, categoria, tag, centro, compromisso, recorrente, dívida,
    parcela, fornecedor e contrato.
  - **Documentos e inventário:** documento, achado, origem, registro, decisão, lote,
    pendência e regra de senha.
  - **Fontes:** conexão (inclusive as fontes do inventário, que são conexões sem agenda),
    coleta, alerta, canal e roteiro.
  - **Estação e análise:** estação, alteração, ponto de atenção e destinatário.

  Entidade nova nasce com página. As exceções são de propósito: os segredos do chaveiro
  nunca aparecem, e os estados internos do pipeline não chegam à interface.

- **A página de um item** tem um cabeçalho com o resumo e abas com tudo o que se liga a
  ele. O fornecedor, por exemplo:
  - **Cabeçalho:** total por ciclo, se é recorrente, apelidos e contratos.
  - **Abas:** Movimentações, Registros · a conferir, Documentos, Compromissos e
    recorrentes.
- **Duas fontes, nunca somadas:** "Movimentações" vem do livro-caixa (fatia 2) e
  "Registros · a conferir" vem do inventário (fatia 1). Um registro já promovido mostra
  o link da movimentação em que virou. Nesta fatia, as abas do livro-caixa aparecem
  desabilitadas, com o motivo.
- **Rastro:** toda página tem a aba Rastro, que mostra a cadeia de proveniência. Para
  trás vai até a origem e a conexão. Para a frente vai até a movimentação, o rateio e os
  centros. Cada elo é um link e diz quem decidiu (você ou `regras@2`) e quando.
- **Todo total é clicável:** em conta, cartão, dívida, centro, fornecedor, ciclo e
  qualquer outro lugar, o número abre a lista dos itens que o somam.
- **Histórico:** Voltar e Avançar (Alt+←/→ e os botões laterais do mouse), com a trilha
  "Fornecedores › Energia Exemplo".
- **Em paralelo:** Ctrl+clique abre a página num painel flutuante, que pode ser
  destacado em janela do sistema, para comparar lado a lado. O clique normal continua
  abrindo no centro.
- **Nesta fatia:** funcionam as páginas das entidades do inventário: achado, origem,
  registro, decisão, lote, pendência, regra de senha, conexão e coleta. Também
  funcionam fornecedor, ciclo, conta e fatura, a partir dos registros. As outras nascem
  com o livro-caixa.

### O produto inteiro na navegação

O desktop cobre tudo o que o web faz: as áreas da `nova-ia`, mais Fornecedores e
Relatórios, que hoje só existem nas telas antigas. As configurações vão para Opções, nos
bastidores. O protótipo mostra todas as áreas com dados fictícios.

| Grupo      | Áreas (fatia em que chegam)                                                                                                                                                                              |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Dinheiro   | Hoje (2), Movimentações (2 leitura, 3 edição), Contas e cartões com faturas (2), Dívidas (2), Recorrentes (2), Centros (2), Fornecedores (2), Relatórios (1, sobre o inventário; 2, sobre o livro-caixa) |
| Documentos | Caixa de entrada (3)                                                                                                                                                                                     |
| Inventário | as áreas da tabela abaixo (1)                                                                                                                                                                            |
| Fontes     | Conexões (4), Alertas e canais (4), Roteiros (5)                                                                                                                                                         |
| Estação    | Sincronização: alterações aguardando, alterações recusadas lado a lado, estações (2)                                                                                                                     |
| Análise    | Pontos de atenção (6), Destinatários e níveis de confiança (6)                                                                                                                                           |

### Áreas e vistas

As 12 consultas do Datasette (`workers/inventario/datasette/metadata.json`) viram áreas.
A regra de "movimento entre contas" (`tipo IN ('transferencia_interna',
'investimento','pagamento_fatura','fatura_cartao')`) hoje está repetida em várias
consultas. Ela vira **uma** função em `sbf-inventario`.

| Área              | Fonte                                                                                          |
| ----------------- | ---------------------------------------------------------------------------------------------- |
| Visão geral       | O painel de contagens de `relatorio.ts`: resumo, etapas, fontes, os últimos 13 ciclos e avisos |
| A triar           | achados com `estado='novo'`                                                                    |
| Em dúvida         | `duvidas`                                                                                      |
| Documentos        | `documentos`                                                                                   |
| Registros         | `registros`                                                                                    |
| Ciclos            | `por_ciclo`                                                                                    |
| Fornecedores      | `fornecedores`                                                                                 |
| Faturas           | `faturas_conferencia`                                                                          |
| Contas (lacunas)  | `contas_buracos`                                                                               |
| E-mails           | `emails_por_remetente` + `emails`                                                              |
| Arquivos e pastas | `arquivos` + `pastas_pessoais`                                                                 |
| Coletas           | `varreduras`                                                                                   |
| Pendências        | a tabela `pendencia`, agora com escrita                                                        |
| Lotes             | as tabelas novas `lote` e `lote_item`                                                          |
| Relatórios        | as perguntas do §11 sobre os registros, com a marca "a conferir" (próxima seção)               |

### Relatórios do §11

A área Relatórios responde às perguntas do §11 do prompt V6 com os **registros do
inventário**:

- quanto saiu no ciclo atual e nos últimos ciclos;
- maiores fornecedores;
- o que se repete todo ciclo e quais são as assinaturas;
- o que aumentou;
- onde o dinheiro se concentra, por categoria e por fornecedor;
- o que ainda não foi classificado;
- possíveis duplicidades;
- o que falta para a análise ficar confiável: lacunas por conta, faturas que não fecham
  e fontes sem coleta.

Regras:

- **A marca:** todo número leva "a conferir" (`CONTEXT.md`). Nesta fatia não aparecem
  "Resultado do mês", "Disponível agora" nem "Livre após compromissos", que pertencem ao
  livro-caixa.
- **Gasto:** movimento entre contas não conta como gasto. É a mesma função única das
  vistas.
- **Fonte:** um seletor mostra "Inventário · a conferir". "Livro-caixa" aparece
  desabilitado até a fatia 2, e os dois usam **o mesmo código de relatório**, só muda a
  fonte (ADR-012: uma implementação por regra).
- **"O que parece cortável"** aponta para Pontos de atenção (fatia 6). Esta fatia não
  julga gasto.

## O banco: dono, migrações e o log de decisões

- **Dono do esquema:** o Rust, a partir desta fatia. O esquema atual do Bun (versão "2",
  `src/schema.sql`) vira a **migração 1**.
  - Ao abrir um banco com `meta.schema_versao='2'` e `user_version=0`, o Rust confere
    se as tabelas batem e só carimba `user_version=1`.
  - Antes de migrar, o desktop mostra o que vai mudar e faz uma cópia do banco pela API
    de backup do SQLite.
- **A migração 2 cria:**
  - `decisao`, só de acréscimo, com UPDATE e DELETE recusados por trigger. Colunas: `id`,
    `alvo_tipo` (achado, registro, fornecedor, lote ou pendencia), `alvo_id`, `campo`,
    `antes`, `depois` (JSON), `autor` (`usuario` ou `regras@N`), `motivo`, `em` e
    `desfaz` (a decisão que esta desfaz, ou nulo).
  - `lote(id, nome, criado_em, aprovado_em, promovido_em)` e
    `lote_item(lote_id, alvo_tipo, alvo_id)`.
  - A coluna `decidido_por` em `achado` e `registro`, preenchida a partir de
    `classificado_por` / `extraido_por`.
  - Os índices que faltam: `origem(gmail_message_id)` e `origem(local_filepath)`.
- **Projeção:**
  - Uma decisão grava a linha em `decisao` e atualiza a projeção (`achado.estado`,
    `document_type`, `supplier_name_raw`, `registro.estado`, `relacao`…) na **mesma
    transação**.
  - O comando de manutenção `reprojetar` refaz a projeção a partir do log, e um teste
    confere que o resultado é o mesmo.
  - Desfazer é uma nova decisão com `desfaz` preenchido, que restaura o `antes`.
- **A decisão do usuário prevalece.** Nenhuma rodada de regras muda o que tem
  `decidido_por='usuario'`.
- **Concorrência com os scripts Bun:** vale o LICOES.md. WAL, `busy_timeout` de 30 s,
  transação IMMEDIATE curta, e nenhuma transação atravessa um quadro da interface. A
  leitura usa uma conexão separada.

### Como cada decisão do §10 cai no banco

| Ação (prompt V6, §10)                | `alvo_tipo` / `campo`                       | Projeção                                                                                             |
| ------------------------------------ | ------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| Documento / Descartar / Em dúvida    | achado / `estado`                           | `achado.estado`                                                                                      |
| Corrigir fornecedor                  | achado ou registro / `fornecedor`           | `supplier_name_raw` ou `fornecedor_normalizado`                                                      |
| Corrigir categoria                   | registro / `categoria`                      | `categoria_provavel`                                                                                 |
| Corrigir tipo                        | achado / `document_type`; registro / `tipo` | idem                                                                                                 |
| Corrigir data e valor                | achado / `data_chave`, `valor_centavos`     | idem, e recalcula `ciclo_id`                                                                         |
| Marcar duplicidade                   | achado ↔ achado / `duplicidade`             | `relacao(tipo='duplicidade')`. Conteúdo idêntico nunca chega aqui: é o mesmo achado com duas origens |
| Unir conciliados                     | registro ↔ registro / `conciliacao`         | `relacao_registro(tipo='conciliacao')`, tabela da migração 2: a `relacao` só aceita achados          |
| Confirmar recorrência                | fornecedor / `recorrente`                   | a decisão em si; vira compromisso recorrente só na fatia 3                                           |
| Aprovar / Rejeitar                   | registro / `estado`                         | `aprovado` ou `divergente`, com motivo                                                               |
| Sinalizar / Responder pendência      | pendencia                                   | `pendencia`, com `resposta` e `resolvida_em`                                                         |
| Novo lote / Adicionar / Aprovar lote | lote                                        | `lote`, `lote_item`; os registros do lote aprovado ficam `aprovado`                                  |

## Chaveiro

- **Importação:**
  - Lê o `segredos/cofre.enc` atual: scrypt com N, r e p do próprio arquivo, AES-256-GCM
    com AAD `sbf-inventario-segredos-v1`, conteúdo `{cpf, senhas{rotulo: senha}}`. A
    senha-mestra é pedida uma vez.
  - Grava no `bancada-segredos`, com o serviço "Seu Bolso Feliz" e as chaves `cpf` e
    `senha:<rotulo>`.
  - Sem keyring disponível (por exemplo, Linux sem Secret Service), usa o arquivo de
    fallback, aberto por senha-mestra uma vez por sessão.
- **Onde fica:** o arquivo de fallback mora na pasta de dados do app (XDG_DATA_HOME,
  %APPDATA% ou Application Support), **nunca dentro do cofre**, porque o glossário manda
  o chaveiro não ficar junto dos documentos.
- **O arquivo antigo:** depois de importar e conferir, o desktop **oferece** apagar o
  `cofre.enc`. Nunca apaga sozinho. O `workers/inventario/src/segredos.ts` sai do repo
  no mesmo PR.
- **Garantias:**
  - Senha errada e arquivo adulterado dão o mesmo erro.
  - Nenhum valor vai para log, SQLite ou arquivo temporário.
  - Os bastidores mostram só os rótulos.

## PDF protegido (Apêndice V6)

1. **Alvos:** os achados com `protegido_senha=1`.
2. **Estratégias:**
   - `cpf_completo`, `cpf_pontuado`, `cpf_primeiros:N`, `cpf_ultimos:N` e
     `senha_propria:<rotulo>`.
   - A ordem segue a prioridade das linhas de `regra_senha` para o par (fornecedor,
     tipo), depois as do fornecedor com tipo `*`.
   - A lista padrão de estratégias só entra se você ligar a opção "tentar estratégias
     padrão", que vem desligada. Nada de tentativa cega.
3. **Tentativa:**
   - O candidato é gerado só em memória, com zeroize.
   - **A senha nunca vai num argumento de linha de comando**, porque fica visível no
     `ps`.
   - **Como abrir** (decidido pelo spike `docs/specs/spikes/2026-11-12-pdf-protegido.md`):
     - O `lopdf` 0.47 (MIT) decifra o original em memória.
     - O PDF decifrado vai pela entrada padrão para `pdftotext -layout -l 3 - -`. Nos
       testes, o texto saiu idêntico byte a byte ao que o inventário produz hoje.
     - A senha não sai do processo.
     - Senha errada é um erro tipado (`InvalidPassword`).
     - PDF "reparado e sem nenhuma letra" é tratado como corrompido.
   - **Fallback**, para cifra que o `lopdf` não implementa: `qpdf --password-file=-
--decrypt <original> -` em pipe para o `pdftotext`, nunca com cópia em disco.
4. **Nenhuma cópia decifrada no disco.** O caminho principal não cria arquivo. O único
   caso que vai precisar de pasta temporária é o OCR de PDF protegido escaneado, que
   fica para quando a E3 for portada. Nesse caso valem:
   - pasta 0700 com guarda RAII;
   - faxina das sobras na abertura do app, porque abort, SIGKILL e queda de energia
     deixam sobra;
   - a restrição de AppArmor do `qpdf` no Ubuntu, que só aceita `*.pdf` fora de pastas
     ocultas.
5. **Registro:**
   - O texto vai para `texto` e o `canonical_fingerprint` é preenchido.
   - Cada tentativa grava `tentativa_senha` com a estratégia e o resultado, nunca a
     senha.
   - Um sucesso atualiza `regra_senha` (`sucessos`, `ultimo_sucesso`).
6. **Aprender a regra:** quando uma estratégia vence para um par (fornecedor, tipo) sem
   regra, o desktop **propõe**, por exemplo: "Faturas do Banco Alfa usam os 5 últimos
   dígitos do CPF. Guardar como regra?". Só grava com a sua confirmação, que também vira
   uma decisão.
7. **Limite:** depois de N falhas por documento (padrão 6), o achado vira pendência
   "PDF protegido sem regra" e as tentativas param.
8. **O original nunca muda.** A estante recebe o PDF original, ainda protegido.

## Prévia do arquivo

Decidida com o CEO em 2026-11-13, depois do primeiro uso da janela ("faltou algo
essencial: não tem preview").

- **Onde:** em três lugares, com o mesmo conteúdo:
  - no canto: o painel Prévia do dock, visível por padrão, com a miniatura do item
    selecionado na tabela ou da página aberta;
  - na aba **Arquivo** da página de achado e documento (e de origem e registro, que mostram
    o documento deles), grande, com páginas e zoom;
  - destacada: o botão Destacar abre a prévia numa janela própria do sistema.
  - Ao parar o mouse meio segundo sobre o nome de um achado ou documento numa tabela, um
    tooltip mostra a miniatura (~220 px).
- **Tipos:**
  - PDF, todas as páginas, desenhadas pelo `pdftoppm` lendo a entrada padrão;
  - imagens (png, jpg, webp, gif, bmp, tiff) no próprio app, e HEIC pelo `ffmpeg` em
    memória;
  - e-mail (`.eml`, `.msg`): remetente, assunto, data, corpo em texto e anexos (link quando o
    anexo é um achado);
  - texto (xml, ofx, ofc, html, csv, txt) como texto;
  - Office (xls, xlsx, xlsm, doc, docx, rtf) convertido em PDF pelo LibreOffice numa pasta
    0700 em memória (`$XDG_RUNTIME_DIR`, tmpfs), apagada logo depois, com faxina das sobras
    ao abrir;
  - zip: a lista do que tem dentro, sem extrair;
  - o resto: os metadados e "Abrir no programa do sistema".
- **PDF protegido:** com o chaveiro aberto e uma regra que funcione, decifra em memória
  (lopdf) e desenha. Sem regra, mostra "Protegido por senha" e o atalho Nova regra. Nunca
  grava cópia decifrada.
- **Cache:** só em memória, rotativo: guarda as últimas 30 prévias ou até 256 MB, o que vier
  primeiro (ajustável em Opções). Nada do conteúdo vai para disco fora do cofre (ADR-015).
- **Fonte do arquivo:** a cópia do acervo (`achado.caminho_acervo`, relativa à pasta do
  cofre). Sem cópia no acervo (o inventário só copia o que já virou documento), lê o
  original da primeira origem local acessível (`origem.local_filepath`), **só leitura**, com
  o selo "lido da origem"; decidido com o CEO em 2026-11-13. Sem nenhuma das duas, mostra
  os metadados e o motivo.
- **Limites:** o mesmo teto de 50 MiB do PDF protegido, imagens até 64 megapixels (reduzidas
  para a tela), e prazo de 30 s por conversão; passou disso, mostra o motivo.
- **Programas externos** (`pdftoppm`, `ffmpeg`, `soffice`) sempre pelo `sbf-processo`:
  caminho absoluto, ambiente mínimo, sem senha em argumento.

## Lotes

- **Montar:** você cria um lote com nome e adiciona achados em estado Documento e
  registros.
- **Aprovar:** grava `aprovado_em` e muda os registros do lote para `aprovado`.
- **Enviar:** o botão Enviar ao servidor fica desabilitado até a fatia 3. Um lote
  aprovado continua editável só por uma nova decisão (desaprovar).

## Estante e cópia de segurança do cofre

As duas usam o rclone, que o inventário já usa. O remote do Drive e a pasta raiz `SBF/`
são configurados em Opções.

### Estante (`SBF/Estante/`)

- **Organização:** `<ciclo AAAA-MM>/<tipo>/<data_fornecedor_valor_hash8.ext>`, o mesmo
  nome de `vista-nome.ts`. Documento sem data-chave vai para `a-classificar/<tipo>/`.
- **Conteúdo:** só achados em estado Documento, com os bytes originais. Na fatia 2 entram
  também os documentos que vêm do servidor.
- **Atualização:** é incremental. Uma correção de fornecedor ou data vira um rename na
  estante, não um arquivo novo; o spike decide entre `rclone sync --track-renames` e uma
  lista própria.
- **A coleta do Drive ignora `SBF/`.** O `coletar-nuvem.ts` ganha essa exclusão nesta
  fatia, como mudança transitória até ser portado.

### Cópia de segurança (`SBF/Copia-de-seguranca/`)

- **O que vai:** o acervo inteiro e um instantâneo do `inventario.sqlite`, tirado pela
  API de backup e nunca copiando o WAL, num remote rclone crypt.
- **A senha da cripto** fica no chaveiro e vai ao rclone sem passar por argumento de
  linha de comando.
- **Aviso de perda:** a tela avisa que sem essa senha a cópia é ilegível, e que você
  precisa tê-la também fora do computador.

## Aviso de disco sem cifra (ADR-015)

Ao abrir, o desktop descobre se a pasta do cofre está num volume cifrado:

- no Linux, procura dm-crypt entre os ancestrais do dispositivo;
- no Windows, consulta o BitLocker;
- no macOS, consulta o FileVault.

Se o volume não for cifrado, ou não der para saber, a barra de status mostra o aviso com
ícone e texto. O aviso não bloqueia nada.

## O que muda nos scripts Bun nesta fatia

São mudanças transitórias, cada uma apagada quando o dono dela for portado:

- `banco.ts` / `aplicarSchema`: com `user_version>=1`, não roda DDL, e recusa um banco
  numa versão que não conhece.
- `e3-triar.ts`: pula achado com `decidido_por='usuario'`.
- `coletar-nuvem.ts`: ignora a pasta `SBF/` do Drive.
- `segredos.ts`: sai, substituído pelo chaveiro.
- `datasette/metadata.json` e `inventario:ver`: saem no PR que fecha o aceite das vistas
  (INV-03).

## Armadilhas do port, para as fatias seguintes e para os testes desta

- **Look-around:** `triagem.ts` tem cerca de 20, `extrair.ts` 13 e `pluggy-normalizar.ts`
  1. O crate `regex` não suporta; use `fancy-regex` ou reescreva.
- **`\b`:** o do JS é ASCII e o do Rust é Unicode. Use `(?-u:\b)` onde a regra depende
  disso, como nos rótulos com acento de `extrair.ts`.
- **`origin_key` local:** usa `st.mtimeMs` formatado como número JS, que pode ter fração.
  O Rust precisa produzir a mesma string (por exemplo com `ryu-js`), senão a
  idempotência quebra.
- **`canonical_fingerprint`:** é `lower` + `\s+` → espaço + `trim`. O teste usa
  fixtures geradas pelo próprio TS, porque `toLowerCase` e `\s` do JS e do Rust divergem
  nas bordas.
- **Ciclo:** fuso America/Sao_Paulo, dia de corte 20, e o mesmo teste dia a dia de
  2024 a 2026 que o `ciclo.test.ts` faz contra `@sbf/domain`.

## Matriz de aceite

Cada item termina como PASS, FAIL, UNVERIFIED ou N/A, com teste ou evidência (regra do
`00-estado-real.md`).

| ID     | Critério                                                                                                                                                                                                                                                                                                                           |
| ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| INV-01 | Abre o inventário de verdade, avisa da migração e faz a cópia antes. Fora isso, nada muda no banco até a primeira decisão.                                                                                                                                                                                                         |
| INV-02 | Um banco na versão "2" do Bun vira `user_version=2` do Rust. O Rust recusa banco mais novo que ele, e o Bun recusa banco mais novo que ele.                                                                                                                                                                                        |
| INV-03 | Cada uma das 12 vistas devolve as mesmas linhas que a consulta do Datasette correspondente, sobre o banco de teste.                                                                                                                                                                                                                |
| INV-04 | A Visão geral mostra as mesmas contagens que o `relatorio.ts` para o mesmo banco.                                                                                                                                                                                                                                                  |
| INV-05 | Toda Ação do §10 grava uma decisão e a projeção na mesma transação. Desfazer restaura. `reprojetar` reproduz o estado (teste de propriedade).                                                                                                                                                                                      |
| INV-06 | O `e3-triar` rodado depois de uma decisão sua não muda o achado.                                                                                                                                                                                                                                                                   |
| INV-07 | `decisao` recusa UPDATE e DELETE.                                                                                                                                                                                                                                                                                                  |
| INV-08 | O chaveiro importa um `cofre.enc` de teste (N=2^10). Senha errada e arquivo adulterado dão o mesmo erro. O valor de teste não aparece em nenhum arquivo, log ou banco tocado (busca pelo valor).                                                                                                                                   |
| INV-09 | O PDF de teste abre pela regra, e a senha não aparece no argv (conferido por `/proc` durante a execução) nem em log. Nenhum arquivo decifrado é criado (conferido com `strace`). O texto sai igual ao do `pdftotext` sobre o original decifrado. Depois de N falhas o achado vira pendência. A regra só é gravada com confirmação. |
| INV-10 | Aprovar um lote marca os itens. Enviar fica desabilitado, com o motivo.                                                                                                                                                                                                                                                            |
| INV-11 | A estante tem só documentos, com o nome de `vista-nome`. Um PDF protegido chega protegido. A coleta do Drive ignora `SBF/`.                                                                                                                                                                                                        |
| INV-12 | Restaurar a partir da cópia cifrada reproduz o acervo e o banco com o mesmo hash.                                                                                                                                                                                                                                                  |
| INV-13 | O aviso de disco sem cifra aparece num volume sem cifra e some num cifrado.                                                                                                                                                                                                                                                        |
| INV-14 | A shell funciona: Ribbon nos 3 modos, KeyTips, Paleta, painel de navegação, os 7 temas, densidade e escala. Toda Ação vem da lista única.                                                                                                                                                                                          |
| INV-15 | Abre e navega no Linux, e no Windows e no macOS por VM, como faz o Chess Studio.                                                                                                                                                                                                                                                   |
| INV-16 | Com o inventário real, abre em menos de 3 s e filtra em menos de 300 ms nas vistas principais. As tabelas são virtualizadas, sem paginação visível.                                                                                                                                                                                |
| INV-17 | Nenhum dado real em protótipo, fixture ou teste do repositório.                                                                                                                                                                                                                                                                    |
| INV-18 | Cada resposta do §11 bate com uma consulta de referência sobre o banco de teste e leva "a conferir". Nenhum termo do livro-caixa aparece na fatia.                                                                                                                                                                                 |
| INV-19 | Todo nome visível é link para a página do item. A página do fornecedor lista os registros dele e os documentos, com as abas do livro-caixa desabilitadas com o motivo. Voltar e Avançar refazem o caminho. Ctrl+clique abre em painel flutuante.                                                                                   |
| INV-20 | De qualquer conta, cartão, dívida, centro ou fornecedor se chega a cada registro e movimentação ligados. Todo total abre os itens que o somam. A aba Rastro de um item leva até a origem para trás e até os centros para a frente.                                                                                                 |

## Tickets e paralelização

Os tickets viram issues em `ChicoFigueiredo/Seu-Bolso-Feliz`: um ticket, uma branch
`feat/*`, um PR. Os da `bancada` vão para o repositório dela.

São até 6 subagentes ao mesmo tempo, com arquivos disjuntos. Eu reviso e integro. PR de
visual, UX ou segurança leva `ready-for-human` e espera o OK do CEO.

| Fase | Ticket                                                                                                                                                                                      | Modelo sugerido  | Depende de |
| ---- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- | ---------- |
| A    | T1 `bancada`: criar o repo e extrair `bancada-armazem`                                                                                                                                      | Sonnet           | —          |
| A    | T2 `bancada-segredos`, a partir do `chess-vault`                                                                                                                                            | Opus (segurança) | T1 (repo)  |
| A    | T3 `bancada-casca`: Ribbon, Ações, Paleta, KeyTips, painel de navegação, histórico Voltar/Avançar, link de item e página em painel flutuante, dock, status, temas, Fluent, ícones plugáveis | Opus             | T1 (repo)  |
| A    | T4 `sbf-inventario`: migrações 1 e 2, log de decisões, projeção, desfazer, e as mudanças transitórias em `banco.ts` e `e3-triar.ts`                                                         | Opus             | —          |
| A    | T5 `sbf-inventario`: as 12 vistas e a Visão geral, com fixture contra o Datasette                                                                                                           | Sonnet           | T4         |
| A    | T6 Tokens dos 7 temas do SBF (`packages/design-system/src/tokens.css`) em JSON para a casca                                                                                                 | Haiku            | —          |
| B    | T7 `sbf-chaveiro`: importar o `cofre.enc` e apagar `segredos.ts`                                                                                                                            | Opus             | T2         |
| B    | T8 `sbf-pdf`: PDF protegido, com o spike `lopdf` × `qpdf`                                                                                                                                   | Opus             | T4, T7     |
| B    | T9 `sbf-janela`: áreas, páginas de item, Ações do §10 e Histórico de decisões                                                                                                               | Sonnet           | T3, T5     |
| B    | T10 Lotes                                                                                                                                                                                   | Sonnet           | T4, T9     |
| B    | T11 `sbf-estante`: estante, cópia de segurança e exclusão na coleta do Drive                                                                                                                | Sonnet           | T4, T7     |
| B    | T12 Aviso de disco sem cifra nos 3 SOs                                                                                                                                                      | Haiku            | T3         |
| B    | T14 Relatórios do §11 sobre os registros, com o seletor de fonte                                                                                                                            | Sonnet           | T5         |
| C    | T13 Empacotamento (scripts do Chess adaptados) e relatório da matriz de aceite                                                                                                              | Sonnet           | todos      |

## Revisão

- Protótipo navegável do produto inteiro: https://claude.ai/artifact/G2oqge11uFBBdVHQnr61fE
  (privado, dados fictícios).
- O CEO aprova a spec e o protótipo. Só depois os tickets viram issues e o código começa.

## Referências

- Decisões: `docs/arquitetura/2026-11-12-adr-012` a `-018-*.md`, e os ADR-010 e ADR-011
  revisados.
- Inventário atual:
  - `workers/inventario/src/schema.sql` e `banco.ts`;
  - `relatorio.ts` e `relatorio-encerramento.ts`;
  - `datasette/metadata.json`;
  - `segredos.ts`, `vista-nome.ts` e `ciclo.ts`;
  - `LICOES.md`.
- Pedidos de origem: `docs/prompts/2026-11-11-seu-bolso-feliz-prompt-operacional-v6-atualizado.md`,
  no §10 (interface) e no Apêndice V6 (PDF protegido).
- Chess Studio: `/mnt/d/Chico/chess-studio`, com os crates citados acima,
  `docs/design/` e as ADR-0002 e ADR-0005.
