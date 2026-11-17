# Spec 12 — Desktop: salário com três provas, descontos em folha e empréstimos

**Status:** decidida (grill com o CEO em 2026-11-17) · **Depende de:** spec 11, ADR-019, ADR-020

Todo exemplo aqui é fictício. Os contracheques, contratos e evoluções reais ficam no cofre; os
leitores são testados com fixtures inventadas no mesmo layout.

## Objetivo

Um contracheque arrastado (ou já no cofre) liga, sem retrabalho, tudo o que ele prova:

- o **registro do tipo salário** no inventário, conciliado com o crédito do banco (Pluggy);
- o **Salário** do mês no planejamento, conciliado sozinho quando as três provas batem;
- cada **desconto em folha** do mês, com baixa pelo próprio contracheque;
- cada **empréstimo consignado**, com contrato, evolução, parcela paga e renovação.

## Decisões do CEO (2026-11-17)

| #   | Decisão                                                                                                                                                                                                                                                             |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D1  | **Registro do tipo salário:** o contracheque declara no inventário um registro tipo `salario` (o líquido, na competência), com as rubricas como sub-registros. O crédito do Pluggy passa a tipo `salario` e os dois registros ficam relacionados como o mesmo fato. |
| D2  | **Três provas → automática:** documento (contracheque), crédito no banco e conta batendo exatamente conciliam o Salário sozinho, com Desfazer. Qualquer diferença volta a ser sugestão de um clique.                                                                |
| D3  | **Baixa pelo contracheque:** o contracheque é a prova dos descontos em folha do mês; cada rubrica de desconto ligada a um compromisso concilia a ocorrência dele (sugestão de um clique para todas). Diferença de valor aparece com ⚠.                              |
| D4  | **Outros descontos viram compromissos:** INSS, IR, FUNCEF e Saúde CAIXA viram compromissos de desconto em folha, com o valor seguindo a rubrica do contracheque vigente.                                                                                            |
| D5  | **Cada consignação é um empréstimo:** um contrato próprio, com número, valor, taxa, sistema, prazo, evolução e renovação. Nomes iguais no contracheque não juntam contratos.                                                                                        |
| D6  | **Empréstimos locais primeiro** (ADR-020): no `planejamento.sqlite`, na forma da Dívida do livro-caixa mais número do contrato, evolução e renovação. Sobem ao servidor depois. A área Dívidas mostra.                                                              |
| D7  | **Fonte dos empréstimos:** leitor local dos contratos e evoluções da CAIXA (os do cofre e os arrastados) propõe; o CEO confirma; o que faltar, digita. Sem IA.                                                                                                      |
| D8  | **Evolução:** a do banco vale como fato; dali para frente, calculada pela taxa e pelo sistema (Price). Cada contracheque marca a parcela paga; diferença entre calculado e descontado aparece com ⚠.                                                                |
| D9  | **Renovação:** quando um contrato some e outro aparece na parcela 1, o sbf sugere a renovação (o antigo quitado por renovação, o novo ligado a ele); o CEO confirma.                                                                                                |
| D10 | **Linhas no Controle:** cada empréstimo gera a sua parcela do mês como desconto em folha. Os compromissos de consignação vindos da planilha são ligados ao empréstimo identificado (com confirmação) e param de projetar; o histórico deles fica.                   |
| D11 | **Casamento rubrica ↔ empréstimo:** pela parcela, com folga de 1% (mínimo R$ 1). Se dois cabem, pergunta e lembra a escolha nos meses seguintes.                                                                                                                    |
| D12 | **Histórico do cofre:** uma rodada local lê contratos, evoluções e contracheques antigos e propõe os empréstimos com as parcelas já pagas e as renovações; confirmação em lote.                                                                                     |
| D13 | **Prazo no contracheque:** no layout de 2026 a coluna Prazo é a parcela atual; nos antigos era quanto faltava. O fim do contrato vem do contrato ou da evolução.                                                                                                    |

## Fases

**Fase A** (D1–D4): o salário e os descontos em folha. Não depende de nenhum documento novo.

**Fase B** (D5–D13): empréstimos. Antes dela, o coordenador estuda (só ele vê dado real) os
layouts reais de contrato e de evolução da CAIXA e escreve fixtures fictícias no mesmo
formato.

## Arquitetura

### Inventário (`sbf-inventario`)

- O registro do contracheque é `registro` com `content_hash` = sha do PDF, `tipo = 'salario'`,
  `data_tipo = 'pagamento'`, `valor_centavos` = líquido, `extraido_por = 'contracheque@1'`, e
  um `sub_registro` por rubrica (`tipo = 'rubrica'`, `codigo`, `parcela_numero` quando
  houver). Idempotente pelo sha.
- O par com o crédito vai para `relacao_registro` (`tipo = 'conciliacao'`, o mesmo das sugestões de conciliação do inventário), e o crédito do
  Pluggy muda de `tipo` para `salario` — ambos pelo log de decisões, nunca por `UPDATE`
  solto, para o Rastro e o desfazer valerem.
- Quem chama: o reconhecimento cria o registro; a conciliação do Salário (automática ou
  confirmada) liga os dois registros.

### Planejamento (`sbf-planejamento`)

- `Autor::Regra { regra }` marca o que uma regra fez sozinha (`"tres-provas"`).
- `Alteracao::Desconciliar { ocorrencia, movimento }` desfaz uma conciliação (exclusão
  lógica, com histórico).
- Movimento de uma baixa pelo contracheque: `contracheque:<sha>:<índice da rubrica>`.
- Compromisso de desconto em folha que segue a rubrica: o previsto do mês vem da rubrica
  ligada (`Rubrica::recorrente`) na composição vigente; sem composição, o último valor.
- Fase B: tabelas de empréstimo, evolução e renovação (migração própria); o empréstimo
  projeta as ocorrências de desconto em folha das suas parcelas.

### Janela (`sbf-janela`)

- Cartão do Reconhecimento: "Criar compromissos de desconto em folha" (D4) e "Dar baixa nos N
  descontos em folha" (D3).
- Detalhe: "Conciliado automaticamente · Desfazer" (D2); na aba Movimentos de um desconto em
  folha, a prova pelo contracheque.
- Fase B: área Dívidas com os empréstimos locais, página do empréstimo (evolução, documentos,
  cadeia de renovação), confirmação em lote das propostas do histórico e a escolha no
  empate do casamento.

## Matriz de aceite

| AC    | Critério                                                                                                                               | Prova                     |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------- | ------------------------- |
| AC-01 | Arrastar um contracheque cria, uma vez só, o registro `salario` com as rubricas como sub-registros                                     | teste + ensaio numa cópia |
| AC-02 | Conciliar o Salário (confirmado ou automático) relaciona o registro do PDF ao do crédito e muda o crédito para `salario`, com desfazer | teste                     |
| AC-03 | Três provas exatas conciliam sozinho com `Autor::Regra`; uma diferença de 1 centavo volta a sugestão                                   | teste                     |
| AC-04 | Desfazer uma conciliação automática a devolve a sugestão e deixa histórico                                                             | teste                     |
| AC-05 | "Dar baixa" concilia cada desconto em folha ligado, com ⚠ quando o valor difere                                                        | teste                     |
| AC-06 | "Criar compromissos" cria INSS, IR, FUNCEF e Saúde como desconto em folha, ligados às rubricas, e o previsto segue a rubrica vigente   | teste + captura           |
| AC-07 | Nada muda no planejamento sem clique, exceto a conciliação de três provas                                                              | teste                     |
| AC-08 | (B) O leitor de contrato e o de evolução leem as fixtures fictícias                                                                    | teste                     |
| AC-09 | (B) Empréstimo gera a parcela do mês como desconto em folha e o compromisso antigo ligado para de projetar                             | teste                     |
| AC-10 | (B) Casamento por parcela com folga de 1%; empate pergunta e lembra                                                                    | teste                     |
| AC-11 | (B) Renovação sugerida quando um contrato some e outro aparece na parcela 1                                                            | teste                     |
| AC-12 | (B) Rodada do histórico propõe empréstimos com parcelas pagas; confirmação em lote                                                     | teste + ensaio numa cópia |
| AC-13 | (B) Evolução: banco como fato, Price para frente, ⚠ na diferença                                                                       | teste                     |

## Supervisão

Mesmo esquema das specs 10 e 11: o Opus escreve as costuras e integra; Sonnet implementa com
TDD em worktrees próprios; só fixtures fictícias nos testes; ensaios com dados reais só pelo
coordenador e só em cópias.
