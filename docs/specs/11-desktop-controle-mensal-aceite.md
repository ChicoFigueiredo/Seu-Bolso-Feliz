# Spec 11 — Relatório de aceite do controle mensal

Data: 2026-11-16

Relatório para auditoria independente. O implementador não se declara aprovado; cada estado abaixo tem de ser conferido pelo auditor.

Estados: PASS (critério atendido e comprovado por teste ou inspeção), FAIL (critério não atendido), UNVERIFIED (depende de ação ou medição não feita), N/A (fora do escopo por decisão registrada).

Quando o critério da matriz da spec (`docs/specs/11-desktop-controle-mensal.md`) diverge do texto usado na rodada, a linha diz qual foi aplicado.

| ID | Critério | Estado | Evidência |
| --- | --- | --- | --- |
| AC-01 | App no Windows 11 | N/A | Decisão D16 da spec: Windows fica fora por ora; tudo roda no Linux da estação. |
| AC-02 | Claude Code acha o MCP, faz uma consulta e uma proposta que a janela aceita ou recusa | UNVERIFIED | Testes: `crates/sbf-mcp/tests/planejamento.rs::lista_de_ferramentas_so_tem_planejamento_quando_ligado`, `proposta_e_idempotente_pela_chave`; `crates/sbf-planejamento/tests/aceite.rs::ac_30_proposta_nao_muda_nada_e_so_a_confirmacao_aplica`. Falta o registro real (`claude mcp add`), que depende do usuário, e a aceitação/recusa na janela. A chamada stdio informada listou 25 ferramentas e respondeu a duas consultas; não houve proposta aplicada. |
| AC-03 | Prévia de uma aba reconhece linhas, grupos e erros de fórmula, sem executar macro | PASS | `crates/sbf-planilha/src/importacao.rs::previa_reconhece_linhas_grupos_e_erros_sem_gravar`; `crates/sbf-planilha/src/leitura.rs::grupo_propaga_e_subtotal_nao_e_linha`; `crates/sbf-planilha/src/leitura.rs::erro_de_formula_vira_ausente_com_marca_e_nunca_zero`. |
| AC-04 | Reimportar não duplica nada | PASS | `crates/sbf-planilha/src/importacao.rs::confirmar_e_reimportar_nao_duplica`. |
| AC-05 | Previsto, pago informado e conciliado são números distintos; pago vazio não é "não pago" | PASS | `crates/sbf-planejamento/tests/aceite.rs::conciliacao_confirmada_e_o_unico_pago_e_sugestao_nunca_rebaixa_a_confirmada`; `crates/sbf-janela/src/controle.rs::sem_valor_nao_vira_zero_nos_totais`; `crates/sbf-janela/src/controle.rs::informar_pago_e_adiar_viram_excecao`. |
| AC-06 | Desconto em folha não sai do caixa de novo | PASS | `crates/sbf-planilha/src/conciliacao.rs::desconto_em_folha_e_entrada_com_sinal_certo`; `crates/sbf-planilha/src/importacao.rs::consignacao_e_desconto_em_folha_e_salario_entra_liquido`; `crates/sbf-planejamento/tests/aceite.rs::ac_27_o_livre_nao_desconta_de_novo_o_desconto_em_folha`. |
| AC-07 | Fatura paga não é nova despesa; saque não é gasto; principal não é juros | FAIL | Só a compra no cartão está coberta: `crates/sbf-janela/src/controle.rs::compra_no_cartao_fica_fora_das_saidas_e_vai_a_parte`. Fatura × itens × banco e amortização dependem da conciliação completa (fora de hoje, D1). |
| AC-08 | Mês futuro nasce da regra e não herda pago | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_25_pago_em_outubro_nao_aparece_pago_em_novembro` (confere meses seguintes, inclusive distante). |
| AC-09 | Pausa e cancelamento preservam o histórico; adiar não some | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_09_adiar_muda_o_vencimento_sem_sumir_e_adiar_sem_data_continua_devida`. |
| AC-10 | Histórico por fornecedor, banco/cartão e documentos; fatura não é quitação | FAIL | Documentos vinculados existem: `crates/sbf-planejamento/tests/composicao_e_documentos.rs::documentos_da_ocorrencia_incluem_os_da_regra_mais_recentes_primeiro`, `vincular_e_idempotente_e_desvincular_marca`. Histórico por fornecedor e por banco/cartão não existe. Fora de hoje pela D1. |
| AC-11 | As duas datas do salário aparecem; nada presume o crédito | PASS | `crates/sbf-janela/src/controle.rs::vencimento_mostra_as_duas_datas_do_salario` (confere a data de crédito e a de disponibilidade). A captura pedida na matriz não foi conferida. Aviso de feriado bancário não é exigido pela spec (D4 só pede as duas datas); se for exigido, o critério muda para FAIL. |
| AC-12 | Totais rastreáveis às linhas; desconhecido marca o livre como estimativa | PASS | `crates/sbf-janela/src/controle.rs::totais_por_grupo_batem_com_as_linhas_e_com_o_rodape`; `crates/sbf-planejamento/tests/aceite.rs::saldo_ausente_ou_com_mais_de_um_dia_marca_o_livre_como_estimativa`. |
| AC-13 | Sincronização e conflitos pelo protocolo da estação | N/A | ADR-019: planejamento local primeiro; sincronização é fatia própria. |
| AC-14 | Vídeo/captura, teste Rust, migrações, rollback, checklist | UNVERIFIED | Não consta na matriz da spec 11; critério usado é o da rodada. Existem capturas, testes, migrações versionadas e backups antes de cada reimportação. Faltam o vídeo e um checklist formal. |
| AC-15 | 360 meses consecutivos, sem faltar mês | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_15_trezentos_e_sessenta_meses_consecutivos_sem_faltar_mes`. |
| AC-16 | Janeiro/2050 consultável sem materializar nada; tempo medido | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_16_janeiro_de_2050_sem_materializar_nada`; tempo na seção de medidas. |
| AC-17 | Pausa em 12/2026: dezembro e janeiro fora; novembro preservado | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_17_pausa_em_dezembro_tira_dezembro_e_janeiro_e_preserva_novembro`. |
| AC-18 | Retomada em 03/2027: março volta; dezembro a fevereiro seguem suprimidos | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_18_retomada_em_marco_volta_marco_e_mantem_dezembro_a_fevereiro_suprimidos`. |
| AC-19 | Cancelamento em 02/2027: nada novo depois; o já devido continua | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_19_cancelamento_em_fevereiro_nao_gera_nada_depois_e_o_ja_devido_continua`. |
| AC-20 | Exclusão lógica com motivo, autor e hora; restaurável | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_20_exclusao_logica_com_motivo_autor_e_hora_e_restauravel`; `excluir_sem_motivo_e_recusado`. |
| AC-21 | Editar só este mês × daqui em diante, sem duplicar | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_21_editar_so_este_mes_ou_daqui_em_diante_sem_duplicar`. |
| AC-22 | Reprojetar dá os mesmos ids e totais | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_22_reprojetar_da_os_mesmos_ids_e_totais`. |
| AC-23 | Dia 31, fevereiro bissexto, trimestral, anual e virada de ano | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_23_dia_31_bissexto_trimestral_anual_e_virada_de_ano`. |
| AC-24 | 48 parcelas geram 48 ocorrências | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_24_quarenta_e_oito_parcelas_geram_quarenta_e_oito_ocorrencias`. |
| AC-25 | Pago em outubro não aparece pago em novembro | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_25_pago_em_outubro_nao_aparece_pago_em_novembro`. |
| AC-26 | Pausa ou cancelamento não apaga cobrança devida nem conciliação | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_26_pausa_ou_cancelamento_nao_apaga_cobranca_devida_nem_conciliacao`. |
| AC-27 | O livre não desconta de novo o desconto em folha (consignações) | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_27_o_livre_nao_desconta_de_novo_o_desconto_em_folha`. |
| AC-28 | Fatura × itens × banco não dobram | FAIL | Parcial: `crates/sbf-planejamento/tests/aceite.rs::ac28_compra_no_cartao_nao_conta_duas_vezes_no_caixa`. A fatura, os itens e o banco não estão conciliados. Fora de hoje pela D1. |
| AC-29 | Reimportação com zero duplicatas | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_29_a_mesma_origem_duas_vezes_devolve_o_recorrente_existente`; `crates/sbf-planilha/src/importacao.rs::confirmar_e_reimportar_nao_duplica`. |
| AC-30 | O MCP só cria proposta; sem confirmação na janela, nada muda | PASS | `crates/sbf-mcp/tests/planejamento.rs::ac30_so_propor_cria_proposta_e_nada_mais_muda`; `crates/sbf-mcp/tests/planejamento.rs::ac30_nenhuma_ferramenta_expoe_aplicar_confirmar_recusar_ou_sql`; `crates/sbf-planejamento/tests/aceite.rs::ac_30_recusar_grava_o_motivo_e_nada_muda`. |
| AC-31 | Proposta com versão de base velha é recusada, sem alteração parcial | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_31_proposta_com_base_velha_e_recusada_sem_alteracao_parcial`; `aplicar_com_base_velha_nao_muda_nada`. |
| AC-32 | Offline e online com o servidor | N/A | ADR-019: fora de hoje. |
| AC-33 | Cada total é rastreável; o desconhecido impede o livre "confirmado" | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_33_cada_total_e_rastreavel_as_linhas_e_o_desconhecido_impede_o_livre_confirmado`. O texto da spec pede rastreabilidade, não drill-down clicável; se o clique até documento/regra for exigido, o critério muda para FAIL (a parte do desconhecido passa). |
| AC-34 | O calendário de 360 meses com 100 regras tem a latência medida e reportada | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_34_calendario_de_360_meses_com_100_regras_tem_a_latencia_medida`; valores na seção de medidas. |
| AC-35 | Um evento futuro avulso aparece uma vez só | PASS | `crates/sbf-planejamento/tests/aceite.rs::ac_35_evento_futuro_avulso_aparece_uma_vez_so`; `crates/sbf-planejamento/tests/simular_novo.rs::simular_compromisso_novo_mostra_os_meses_e_nao_grava`. |
| AC-36 | Planilha, CSV e dados reais fora do repositório, dos logs e das mensagens automáticas | PASS | Inspeção: `git ls-files` sem `.xlsx`, `.xlsm`, `.csv`, `.db` ou `.sqlite`. Nas crates do motor, da importação e do MCP, o único `eprintln!` (`crates/sbf-mcp/src/servidor.rs`, `responder`) grava só o nome da ferramenta e o tamanho da resposta. Teste: `crates/sbf-mcp/tests/planejamento.rs::cada_chamada_grava_uma_linha_em_saida_ia_sem_conteudo`. |

## Comandos rodados

- Por este auditor: somente leitura (`git status`, `git log`, `grep`, `ls`, `sed`, `git ls-files`). Não rodei `cargo`, conforme a instrução. Não acessei a pasta de dados reais.
- Pelo coordenador (não reexecutado aqui): `cargo test -p sbf-planejamento -p sbf-planilha -p sbf-mcp -p sbf-janela -p sbf` (436 testes, 0 falhas) e `cargo clippy ... --all-targets -- -D warnings` (limpo).
- Confirmado por este auditor: todos os testes citados existem nos arquivos indicados, com os nomes da tabela.

## Verificações com os dados reais (só contagens)

Informadas pelo coordenador; não verificadas por este auditor:

- Prévia da planilha real: 63 linhas, 9 grupos, 4 linhas marcadas com erro de fórmula.
- Reimportação: segunda importação com 0 novos e 69 existentes; exceções 15 antes e 15 depois.

## Medidas

Informadas pelo coordenador; o teste `ac_34_...` mede, mas não foi reexecutado aqui:

- Calendário de 360 meses com 100 regras: cerca de 10 ms.
- Controle de janeiro/2050: cerca de 0,2 ms.

## O que falta

Itens FAIL ou UNVERIFIED, com a razão:

- **AC-02 (UNVERIFIED).** O registro do MCP no Claude Code depende do usuário (`claude mcp add`). Falta também a proposta aceita e a recusada pela janela, em uso real.
- **AC-07 (FAIL).** Só a compra no cartão está coberta. Fatura, itens, banco e amortização dependem da conciliação completa, que a D1 deixou fora de hoje. Decisão a confirmar: manter FAIL ou tratar como N/A por D1.
- **AC-10 (FAIL).** Documentos vinculados existem. Histórico por fornecedor e por banco/cartão não existe. Mesma questão de D1.
- **AC-14 (UNVERIFIED).** Não consta na matriz da spec. Falta o vídeo e um checklist formal. As capturas, os testes, as migrações versionadas e os backups existem.
- **AC-28 (FAIL).** Só a parte da compra no cartão está coberta. Fatura × itens × banco não estão conciliados. Mesma questão de D1.

Diferenças de critério entre a rodada do coordenador e a spec, que este auditor não aplicou como FAIL:

- **AC-11.** O coordenador marcou FAIL por falta de aviso de feriado bancário. A spec (D4) pede só que as duas datas apareçam, e o teste confirma isso. Se o aviso de feriado for exigido, vira FAIL.
- **AC-33.** O coordenador marcou FAIL por falta de totais clicáveis até o documento. A spec pede rastreabilidade, e o teste confirma as somas e a marca de estimativa. Se o clique for exigido, vira FAIL.
