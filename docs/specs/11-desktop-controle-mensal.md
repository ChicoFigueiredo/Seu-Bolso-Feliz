# Spec 11 — Desktop: controle mensal, horizonte de 360 meses e MCP editável

**Status:** decidida (grill com o CEO em 2026-11-16) · **Depende de:** ADR-019, spec 09, spec 10

A especificação original da outra equipe e as fontes (a planilha `.xlsm` e a pré-importação
`.csv`) são privadas e ficam no cofre, em `financas/entrada/planilhas/`. Esta spec guarda só
as regras. Todo exemplo aqui é fictício.

## Objetivo

A primeira tela do desktop passa a ser o **controle mensal**, do jeito da planilha que o CEO
usa hoje, com algumas diferenças:

- cada número é rastreável até a linha, a regra e o documento;
- qualquer um dos próximos 360 meses pode ser aberto, com o que as regras projetam;
- pausar, retomar, cancelar, adiar e dispensar são ações distintas, versionadas e
  auditadas;
- o Claude consulta, simula e propõe pelo MCP, e a confirmação é sempre na janela.

## Decisões do CEO (2026-11-16)

| #   | Decisão                                                                                                                                                                                                                                                                        |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| D1  | **Escopo de hoje:** as fatias 0 a 4 inteiras: ambiente, importação, tela, horizonte e MCP.                                                                                                                                                                                     |
| D2  | **Gravação local primeiro** (ADR-019): `planejamento.sqlite` no cofre; a sincronização com o Neon vem depois.                                                                                                                                                                  |
| D3  | **Salário:** entra pelo **líquido** (o depósito real). As consignações são **desconto em folha** e não saem do caixa de novo. A projeção repete o líquido como estimativa; a arquitetura deixa espaço para projeções de salário e despesa (cenários) sem implementá-las agora. |
| D4  | **Datas do salário:** com feriado bancário, a data de disponibilidade pode vir antes da data do crédito, e as duas aparecem.                                                                                                                                                   |
| D5  | **Itens ocultos:** recorrentes achados na Pluggy e ausentes da planilha entram **direto** como recorrentes estimados (fonte `Pluggy`). Itens com valor diferente da planilha ganham uma versão nova a partir do ciclo seguinte.                                                |
| D6  | **Centros:** o que o CEO paga para outra pessoa da casa vai para o centro dela; os ganhos dessa pessoa ficam fora. As transferências dela para o CEO entram como entrada recorrente.                                                                                           |
| D7  | **Bolão:** é **dinheiro de passagem**, num centro próprio, fora de receita e despesa.                                                                                                                                                                                          |
| D8  | **Em atraso:** ocorrências vencidas sem pagamento continuam devidas e aparecem junto do período corrente.                                                                                                                                                                      |
| D9  | **Visão:** abre no **ciclo 20–19** (a aba do mês na planilha é o ciclo); um botão troca para mês civil.                                                                                                                                                                        |
| D10 | **Saldo:** o último saldo da Pluggy coletado no cofre, com a data da coleta. Passou de 1 dia, aparece como desatualizado.                                                                                                                                                      |
| D11 | **Prazos de dívida:** só o que der para provar (o plano de empréstimos da planilha, contratos, parcelas pagas). Sem prova, `Prazo::Desconhecido`, marcado como estimativa.                                                                                                     |
| D12 | **Prioridade é a ordem:** grupo de cima antes do de baixo; dentro do grupo, linha de cima antes.                                                                                                                                                                               |
| D13 | **Grupos da planilha** ficam como estão, inclusive "Parcial", que é um grupo do CEO.                                                                                                                                                                                           |
| D14 | **Restos antigos:** linhas com vencimento muito anterior ao mês importado entram só como histórico do mês (`Frequencia::Unica`), sem virar recorrente.                                                                                                                         |
| D15 | **Conciliação simples hoje:** o casamento por fornecedor, valor e janela de data com as transações da Pluggy vira **sugestão**; o CEO confirma em lote.                                                                                                                        |
| D16 | **Windows:** fora por ora. O AC-01 fica N/A e tudo roda no Linux da estação.                                                                                                                                                                                                   |

## Arquitetura

```text
crates/
├── sbf-planejamento   # COSTURA (lib.rs) + motor de projeção + banco + serviço
├── sbf-planilha       # leitura do .xlsm sem macros, prévia, importação,
│                      # itens ocultos, conciliação, saldos (lê o inventário)
├── sbf-janela         # área "Controle mensal" (primeira tela) + propostas
├── sbf-mcp            # ferramentas de consulta, simulação e proposta
└── sbf                # `sbf planejamento importar|ocultos|conciliar` e a ligação
```

- **O contrato** é `crates/sbf-planejamento/src/lib.rs`. O trait `Planejamento` é o único
  serviço que a janela e o MCP usam.
- **Ocorrências não são gravadas.** Elas são calculadas, de modo determinístico, das regras,
  versões, pausas, cancelamentos e exceções. A identidade de uma ocorrência é
  `recorrente:AAAA-MM[:pN]`.
- **Banco** (`planejamento.sqlite` no cofre). Guarda recorrentes, versões, pausas,
  cancelamentos, exceções, conciliações, grupos, propostas, origens importadas e o
  histórico, que só cresce. A exclusão é lógica. As migrações são versionadas por
  `user_version`.
- **Concorrência.** Toda alteração de recorrente leva `versao_base`. Base velha é recusada
  com `VersaoVelha`, e nada muda.

### Motor de projeção (regras)

- **Vigência.** Uma versão vale de `desde` em diante. Mudar o valor "daqui em diante" cria
  uma versão nova, e mudar "só este mês" cria uma `Excecao::ValorSoDesteMes`.
- **Pausa.** Suprime as ocorrências em `[desde, ate)`. Retomar fecha a pausa aberta.
- **Cancelamento.** Suprime a partir de `desde`. Nada antes muda.
- **Supressão não apaga o que é devido.** Uma ocorrência com pago informado, conciliação ou
  exceção de pagamento numa competência suprimida continua visível e devida.
- **Prazo.** `Parcelas { total }` gera exatamente `total` ocorrências, numeradas. Prazo de
  contrato vence o horizonte.
- **Datas.** Vencimento em dia 29, 30 ou 31 cai no último dia válido do mês.
  `CadaNMeses` e `Anual` cobrem a virada de ano.
- **Futuro não herda pagamento.** Cada ocorrência começa `Prevista`, e pago informado é
  sempre uma exceção daquela competência.
- **Estados:**
  - `EmAtraso` quando o vencimento já passou (`< hoje`) sem pago informado, conciliação
    ou dispensa;
  - `Parcial` quando o pago (informado ou conciliado) é menor que o previsto;
  - `Adiada` sem nova data continua devida e conta no em atraso depois do vencimento
    original.
- **Período:** entram as ocorrências cujo vencimento efetivo cai no período.
  `atrasadas_de_antes` traz as em atraso de antes que continuam devidas.
- **Resumo:**
  - **Saídas de caixa** somam `Natureza::Saida` e excluem `DescontoEmFolha` e `Passagem`.
  - **Descontos em folha** aparecem à parte e nunca saem do caixa de novo.
  - **Livre após compromissos** é o disponível agora, mais as entradas previstas até o fim
    do período, menos as saídas de caixa ainda não pagas, menos o em atraso, menos a
    passagem pendente.
  - Ele é marcado como estimativa sempre que houver valor estimado, sem valor ou saldo
    desatualizado. As lacunas dizem o que falta.
- **Desempenho.** Abrir janeiro/2050 não materializa nada; a projeção é por regra.
  `calendario(inicio, 360)` com 100 regras precisa ser medido e reportado.

### Importação da planilha (`sbf-planilha`)

- **Leitura** com `calamine`, só dos valores em cache. Nunca executa macro. Fórmula com
  erro (`#NAME?` etc.) vira `None` com a marca do erro, nunca zero.
- **Aba do mês** (por exemplo `AAAA.MM`):
  - a grade traz grupo, descrição, quantidade, unitário, mínimo, vencimento, meio,
    previsto, pago e observação;
  - o painel lateral traz o salário e as rubricas do contracheque, além do bloco das
    consignações;
  - a legenda dos meios diz que `Salário` = desconto em contracheque.
- **Prévia.** Mostra novos, já existentes, atualizáveis, conflitos, ignorados e dados
  faltantes, sem gravar nada. Confirmar aplica tudo pelo `Planejamento::aplicar` com
  `Autor::Importacao`.
- **Idempotência** por `(sha256 do arquivo, aba, linha)`. Reimportar o mesmo arquivo não
  cria nada; um arquivo novo gera diff, não sobrescreve decisão do usuário.
- **Mapeamento:**
  - meio `Salário` vira `DescontoEmFolha`;
  - o salário líquido vira `Entrada`;
  - o grupo e a ordem da planilha viram `grupo` e `posicao`;
  - a coluna "Valor Pago" vira `Excecao::PagoInformado` do mês importado;
  - vencimento muito antigo vira resto antigo (D14).
- **Itens ocultos.** Recorrência na Pluggy (fornecedor presente em vários ciclos, valor
  estável), ausente das regras: cria o recorrente estimado (D5), com fonte `Pluggy`.
- **Conciliação.** Para cada ocorrência, procura transação de valor igual (ou bem próximo)
  na janela do vencimento e grava `Conciliar { confirmada: false }`.
- **Saldos.** `FonteDeSaldos` sobre o bruto da Pluggy guardado no cofre.

### Janela

- **Controle mensal** é a primeira área.
- **Cabeçalho:**
  - ciclo ou mês civil, anterior e próximo, hoje e pular para um mês;
  - os cards de disponível agora (com a data), entradas e as duas datas do salário,
    saídas de caixa, desconto em folha, em atraso, a conferir e livre após compromissos;
  - a estimativa aparece sempre marcada.
- **Grade:**
  - em 3 níveis: grupo, subgrupo e linha, na ordem de prioridade;
  - colunas: origem, nome, vencimento, previsto, pago informado, conciliado, falta,
    estado, conta, centro e documentos;
  - a cor nunca é o único sinal;
  - o mês futuro mostra "projeção".
- **Ações:** editar (só este mês ou daqui em diante), pausar, retomar, cancelar, adiar,
  dispensar, informar pago, confirmar conciliação, mover/reordenar, arquivar, excluir e
  restaurar. A prévia mostra os "meses afetados" antes de confirmar.
- **Painel de propostas.** Lista o que o Claude propôs. Confirmar e recusar acontecem só
  aqui.
- **Detalhe da linha:** resumo, próximas ocorrências, versões, exceções e histórico.

### MCP (`sbf mcp`)

- **Leitura:** `listar_compromissos`, `resumo_de_caixa`, `projetar_compromissos` (até 360
  meses), `obter_compromisso`, `simular_mudanca` e `listar_propostas`.
- **Proposta:** `propor_compromisso`, `propor_alteracao`, `propor_pausa`,
  `propor_retomada`, `propor_cancelamento`, `propor_exclusao_logica` e
  `propor_conciliacao`. Todas usam `Planejamento::propor` com chave de idempotência.
- **Nunca no MCP:** `aplicar`, `confirmar_proposta`, SQL livre, pagamento ou cancelamento
  no fornecedor.
- **Nível de confiança.** O texto devolvido passa pelo nível do cliente (mascarado por
  padrão), e cada chamada grava uma linha em `saida_ia`, sem o conteúdo.

## Matriz de aceite

A numeração segue a da especificação original.

| ID    | Critério                                                                                              | Como comprova              |
| ----- | ----------------------------------------------------------------------------------------------------- | -------------------------- |
| AC-01 | App no Windows 11                                                                                     | **N/A** (D16)              |
| AC-02 | Claude Code acha o MCP, faz uma consulta e uma proposta que a janela aceita ou recusa                 | registro real + teste      |
| AC-03 | A prévia de uma aba fictícia reconhece as linhas, os grupos e os erros de fórmula, sem executar macro | teste com `.xlsx` fictício |
| AC-04 | Reimportar não duplica nada                                                                           | teste                      |
| AC-05 | Previsto, pago informado e conciliado são números distintos; pago vazio não é "não pago"              | teste + captura            |
| AC-06 | Desconto em folha não sai do caixa de novo                                                            | teste                      |
| AC-08 | O mês futuro nasce da regra e não herda pago                                                          | teste                      |
| AC-09 | Pausa e cancelamento preservam o histórico; adiar não some                                            | teste                      |
| AC-11 | As duas datas do salário aparecem; nada presume o crédito                                             | captura                    |
| AC-12 | Os totais são rastreáveis às linhas; valor desconhecido marca o livre como estimativa                 | teste                      |
| AC-15 | 360 meses consecutivos, sem faltar mês                                                                | teste                      |
| AC-16 | Janeiro/2050 consultável sem materializar nada; tempo medido                                          | teste + medida             |
| AC-17 | Pausa em 12/2026: dezembro e janeiro fora; novembro preservado                                        | teste                      |
| AC-18 | Retomada em 03/2027: março volta; dezembro a fevereiro seguem suprimidos                              | teste                      |
| AC-19 | Cancelamento em 02/2027: nada novo depois; o já devido continua                                       | teste                      |
| AC-20 | Exclusão lógica com motivo, autor e hora; restaurável                                                 | teste                      |
| AC-21 | Editar só este mês × daqui em diante, sem duplicar                                                    | teste                      |
| AC-22 | Reprojetar dá os mesmos ids e totais                                                                  | teste                      |
| AC-23 | Dia 31, fevereiro bissexto, trimestral, anual e virada de ano                                         | teste                      |
| AC-24 | 48 parcelas geram 48 ocorrências                                                                      | teste                      |
| AC-25 | Pago em outubro não aparece pago em novembro                                                          | teste                      |
| AC-26 | Pausa ou cancelamento não apaga cobrança devida nem conciliação                                       | teste                      |
| AC-27 | O exemplo das consignações: o livre não desconta de novo o desconto em folha                          | teste                      |
| AC-29 | Reimportação: zero duplicatas                                                                         | teste                      |
| AC-30 | O MCP só cria proposta; sem confirmação na janela, nada muda                                          | teste                      |
| AC-31 | Proposta com versão de base velha é recusada, sem alteração parcial                                   | teste                      |
| AC-33 | Cada total é rastreável; o desconhecido impede o livre "confirmado"                                   | teste                      |
| AC-34 | O calendário de 360 meses com 100 regras tem a latência medida e reportada                            | medida                     |
| AC-35 | Um evento futuro avulso aparece uma vez só                                                            | teste                      |
| AC-36 | Planilha, CSV e dados reais fora do repositório, dos logs e das mensagens automáticas                 | inspeção + teste           |

Ficam fora de hoje, por D1: AC-07 e AC-28 (fatura, itens do cartão e saque), AC-10
(histórico por fornecedor e documentos, que vem com a conciliação completa), AC-13
(sincronização) e AC-32 (offline e online com o servidor).

## Supervisão

- **Agentes.** Quatro agentes Sonnet trabalham em worktrees próprias, com as regras de
  `fatia3/REGRAS.md`: motor e banco; planilha, ocultos e conciliação; janela; MCP.
- **Integração.** O coordenador integra e roda a importação real no cofre. Esse passo nunca
  passa por agente.
- **Revisão.** Uma revisão final de privacidade e permissão cobre AC-30, AC-31 e AC-36.
