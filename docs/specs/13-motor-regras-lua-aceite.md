# Spec 13 — Relatório de aceite das regras de reconhecimento e scripts Luau

Data: 2026-11-17

Relatório para auditoria independente. O implementador não se declara aprovado; cada estado abaixo tem de ser conferido pelo auditor. A matriz é a da spec 13 (`docs/specs/13-motor-regras-lua.md`, §8, AC-LUA-01 a AC-LUA-14), com o texto resumido.

Estados: PASS (critério atendido e comprovado por teste ou inspeção), PARCIAL (atendido em parte, com a lacuna dita), FORA DO DIFF (comprovado por ensaio fora do repositório, não reproduzível pelo diff), FAIL (critério não atendido).

Termos do glossário (`CONTEXT.md`): **Regra de reconhecimento**, **Script**, **Caixa de entrada** (estado **Precisa revisar**), **Rastro**.

## Verificação geral

- `cargo fmt --all --check`: ok.
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 avisos.
- `cargo test --workspace`: 1671 testes, 0 falhas (eram 1508 antes da spec 13).
- Ambiente: Linux. Windows segue adiado (D16 da spec 11).

Os resultados acima foram informados pelo coordenador. Não foram reexecutados na redação deste relatório; o auditor deve reexecutá-los.

## Matriz AC-LUA

| ID        | Critério                                                                                       | Estado       | Evidência                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| --------- | ---------------------------------------------------------------------------------------------- | ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| AC-LUA-01 | Tipo novo por regra ativa no SQLite, reconhecido após reload, sem recompilar Rust              | PASS         | `crates/sbf-regras/tests/regras.rs::ac_lua_01_tipo_novo_por_configuracao_reconhece_apos_reload`. Ponta a ponta com o mesmo binário: `crates/sbf/tests/regras_cli.rs::prova_sem_recompilar_regra_nova_e_depois_a_v2_com_o_luau_alterado`. Demo para o dono: `scripts/desktop/demo-regras.sh` (interativo).                                                                                                                                                                                           |
| AC-LUA-02 | Regra específica supera a geral, com trilha; mudar um layout não afeta tipos antigos           | PASS         | `crates/sbf-regras/tests/regras.rs::ac_lua_02_regra_especifica_vence_a_geral_e_nao_mexe_em_tipo_antigo`.                                                                                                                                                                                                                                                                                                                                                                                            |
| AC-LUA-03 | Conflito entre regras não grava; sinaliza ambiguidade, motivo e candidatos                     | PASS         | `crates/sbf-regras/tests/regras.rs::ac_lua_03_empate_discordante_e_ambiguo_e_nao_grava_proposta`; `crates/sbf-reconhecimento/tests/regras.rs::ambiguo_vai_para_precisa_revisar_e_nao_grava_proposta`. Captura `docs/specs/capturas/13-reconhecimento-ambiguo.png`.                                                                                                                                                                                                                                  |
| AC-LUA-04 | Mesma entrada, mesma regra/versão e mesmos parâmetros dão os mesmos bytes e a mesma chave      | PASS         | `crates/sbf-regras/tests/regras.rs::ac_lua_04_mesma_entrada_mesmos_bytes_e_mesma_chave`; `crates/sbf-regras/tests/regras.rs::ac_lua_04_proptest_leitura_estavel`; `crates/sbf-script/tests/seguranca.rs::determinismo_cem_execucoes`. Nota: `duracao_us` é o único campo medido; não decide nada e é zerado na comparação.                                                                                                                                                                          |
| AC-LUA-05 | Script sem `io`/`os`/`package`/`debug`, sem rede, arquivos ou SQL; com limites; loop não trava | PASS         | `crates/sbf-script/tests/seguranca.rs` (globais proibidos, laço infinito, recursão, memória); `crates/sbf-script/tests/filho.rs` (filho morto pelo prazo, ambiente limpo, endurecimento). Nota: no kernel do teste (Linux 7.0), Landlock e rlimits ficaram totais (arquivo, rede e processo negados). Em kernel sem Landlock, o isolamento de SO é melhor esforço e a garantia vem da VM sem `io/os/package/debug/load`.                                                                            |
| AC-LUA-06 | Lua não marca pago nem cria lançamento; tudo passa por validação Rust                          | PASS         | `crates/sbf-reconhecimento/tests/regras.rs::ac_lua_06_regra_ativa_so_propoe_e_nada_grava_sozinho`; `ac_lua_06_consolidada_com_duas_provas_nao_e_automatica_e_com_tres_e`; `ac_lua_06_nao_ha_proposta_que_crie_gasto`. O tipo `Proposta` não tem variante que lance, pague ou crie movimentação.                                                                                                                                                                                                     |
| AC-LUA-07 | Editor salva rascunho e prova exemplos; IA não publica nem ativa; rollback preserva histórico  | PASS         | `crates/sbf-regras/tests/regras.rs::ac_lua_07_exemplo_falhando_bloqueia_a_ativacao`; `ac_lua_07_ia_e_sistema_nao_ativam_nem_consolidam_nem_revertem`; `ac_lua_07_rollback_preserva_todas_as_versoes`; `ac_lua_07_versao_e_imutavel_no_banco`; `crates/sbf-mcp/tests/regras.rs::nenhuma_ferramenta_ativa_consolida_desativa_ou_reverte`; `crates/sbf/tests/regras_cli.rs::ativar_sem_terminal_e_recusado_e_nada_muda`. Capturas `13-regras-pagina.png` (ativação bloqueada) e `13-regras-lista.png`. |
| AC-LUA-08 | Cada campo e associação rastreável a origem, trecho, regra e versão                            | PARCIAL      | `crates/sbf-regras/tests/regras.rs::ac_lua_08_todo_campo_tem_de_onde_e_o_trecho_aponta_para_o_texto_certo`: todo campo tem `DeOnde` (trecho em bytes, dica, leitor, script). Falta a PÁGINA do trecho (derivável das quebras de página do pdftotext, não feito). As `evidencias` do script são rótulos livres. Captura `13-reconhecimento-pela-regra.png` (Rastro).                                                                                                                                 |
| AC-LUA-09 | Documento repetido em Telegram e pasta é reconhecido uma vez; evento Pluggy não duplica        | PASS         | `crates/sbf-reconhecimento/tests/regras.rs::ac_lua_09_arrasto_e_depois_ja_no_cofre_e_uma_proposta_so`; `ac_lua_09_mesmo_conteudo_em_dois_caminhos_acumula_origens_e_uma_proposta`; `ac_lua_09_telegram_e_pasta_com_o_mesmo_sha_dao_a_mesma_chave`; `ac_lua_09_o_mesmo_registro_da_pluggy_lido_duas_vezes_e_uma_proposta`. Telegram e pasta simulados com o mesmo SHA. Telegram e Gmail reais ficaram fora (D1).                                                                                     |
| AC-LUA-10 | Sem dupla contagem entre itens do cartão, fatura e débito; contracheque sem dobrar caixa       | PASS         | `crates/sbf-reconhecimento/tests/regras.rs::ac_lua_10_fatura_mais_debito_concilia_a_ocorrencia_sem_gasto_novo`; `ac_lua_10_comprovante_prova_o_debito_que_ja_existe_sem_registro_novo`; `ac_lua_10_comprovante_sem_debito_unico_nao_inventa_um`; `ac_lua_10_contracheque_liquido_e_desconto_em_folha_nao_reduzem_o_caixa_duas_vezes`.                                                                                                                                                               |
| AC-LUA-11 | IRRF estimado indica tabela, competência e versão; alerta divergência; não mexe na rubrica     | PASS         | `crates/sbf-reconhecimento/tests/regras.rs::ac_lua_11_irrf_estimado_traz_a_tabela_e_alerta_sem_mexer_na_rubrica`; `ac_lua_11_com_a_v1_ativa_nao_ha_alerta_de_irrf`; `crates/sbf-script/tests/irrf.rs` (8 fronteiras calculadas à mão, tolerância de 1 centavo). Tabela oficial 2026 conferida em https://www.gov.br/receitafederal/pt-br/assuntos/meu-imposto-de-renda/tabelas/2026. Limite: sem "rendimento tributável" no contracheque, base até R$ 7.350 pede revisão (o redutor depende dele).  |
| AC-LUA-12 | Testes, fmt, clippy e build Linux documentados; job Rust na CI                                 | PARCIAL      | `.github/workflows/rust.yml` (fmt, clippy, test, build release) depende do secret `BANCADA_TOKEN`. Sem ele, o job avisa e pula (D9); a CI verde não prova nada até o dono rodar `scripts/desktop/configurar-token-da-bancada.sh`. Os comandos foram executados localmente (números na seção de verificação).                                                                                                                                                                                        |
| AC-LUA-13 | Ensaio real contra cópia do cofre, com contagens e discrepâncias, sem expor dados              | FORA DO DIFF | Ensaio do coordenador, em cópias do cofre, só contagens (ver abaixo). Não consta no repositório. A re-checagem no dado real, depois da tolerância de 1 centavo, NÃO foi feita: ativar a versão exige confirmação humana (D14).                                                                                                                                                                                                                                                                      |
| AC-LUA-14 | Controle mensal de 360 meses, pausas e conciliações iguais com e sem as regras                 | PASS         | `crates/sbf-reconhecimento/tests/regras.rs::ac_lua_14_com_e_sem_regras_o_resultado_atual_e_o_mesmo`; `ac_lua_14_se_o_catalogo_nao_abre_o_caminho_atual_roda_como_hoje`; mais o ensaio real (todos os contracheques do acervo, 0 diferenças). O `sbf-planejamento` não foi alterado pelo diff e a suíte dele segue verde. Não há teste novo "com e sem" do planejamento: a garantia é a ausência de mudança mais a suíte.                                                                            |

## Verificações com os dados reais (só contagens)

Informadas pelo coordenador, em cópias do cofre; nenhum valor, nome ou conta consta deste relatório:

- Todos os contracheques reais do acervo rodados com e sem as regras: o mesmo resultado em cada arquivo, 0 diferenças. (As contagens ficam fora do repositório público.)
- Os não reconhecidos (nos dois casos) são layouts antigos que o leitor nativo já não lia. Isso é pré-existente e é um bom alvo para regras com script.
- IRRF no contracheque de 2026: o valor estimado pela tabela oficial difere do retido em 1 centavo, porque a folha trunca. Daí a tolerância de 1 centavo.

## Primeira prova obrigatória (spec §7, P0 e P1)

Cadastrar um comprovante novo e trocar o Luau sem recompilar, pelo terminal (`<cofre>` é a pasta do cofre de teste, `<regra>` o identificador da regra):

1. `sbf regras --inventario <cofre> importar <regra.json> --script <regra-v1.luau> --motivo "..."`: entra em rascunho.
2. `sbf regras --inventario <cofre> exemplo <regra> <documento.txt> --esperado <esperado.json>` (ou `--nao-se-aplica`): grava o exemplo que a versão precisa cumprir.
3. `sbf regras --inventario <cofre> testar <regra> 1`: roda os exemplos.
4. `sbf regras --inventario <cofre> simular <regra> 1 <documento.txt>`: mostra o que a regra faria, sem gravar.
5. `sbf regras --inventario <cofre> ativar <regra> 1`: exige um terminal interativo e a confirmação digitada (D14).
6. Trocar o Luau: `importar` de uma nova versão (v2), `testar`, `simular`; a v2 entra em rascunho até ser ativada.
7. `sbf regras --inventario <cofre> reverter <regra>`: volta à versão anterior, preservando todas.

Demo pronta, interativa e com dados fictícios: `scripts/desktop/demo-regras.sh`.

Pela janela:

- Documentos › Regras de reconhecimento: lista, página com editor, exemplos, diff da simulação, versões e rollback.
- Na Caixa de entrada, em um documento: "Testar com este documento" (roda as regras sobre ele) e "Criar regra a partir daqui" (rascunho montado a partir da correção, sem IA, D7).

## Capturas

Arquivos em `docs/specs/capturas/`. Descrição pelo nome; o auditor deve abrir cada uma e confirmar que mostra dados fictícios.

- `13-criar-regra.png`: formulário "Criar regra a partir daqui" (D7).
- `13-reconhecimento-ambiguo.png`: documento em Precisa revisar, com as regras em conflito listadas e nada gravado (AC-LUA-03).
- `13-reconhecimento-pela-regra.png`: Rastro do reconhecimento, com a regra e a versão que decidiram (AC-LUA-08).
- `13-regras-lista.png`: lista de regras, com estado e versão ativa (AC-LUA-07).
- `13-regras-pagina.png`: página da regra com a ativação bloqueada por exemplo que falha (AC-LUA-07).
- `13-regras-pagina-exemplos.png`: página da regra com os exemplos e o resultado de cada um.
- `13-regras-simulacao.png`: diff da simulação entre versões.

## Limites conhecidos e decisões de revisão

1. **D14, ativação.** A confirmação digitada num terminal interativo barra MCP, pipelines e scripts. Mas um agente com shell consegue emular um terminal, então essa confirmação não é uma barreira contra um agente. A barreira real é o MCP não ter nenhuma ferramenta de ativação, consolidação, desativação ou reversão (teste `nenhuma_ferramenta_ativa_consolida_desativa_ou_reverte`). No ensaio, o coordenador ativou uma vez a v2 do contracheque numa CÓPIA descartável, emulando o terminal. O agente do CLI fez o mesmo na demo fictícia; na segunda tentativa, a ativação foi bloqueada pelo controle de permissões, corretamente. Nada foi ativado no cofre real.
2. **Três provas.** As sugestões de regra de consolidação seguem o modelo do salário: documento que identifica o alvo, valor exato, conta ou assinatura.
3. **Fábrica.** `semear_fabrica` roda a cada leitura. É idempotente: a leitura grava as regras de fábrica que faltam.
4. **Revisão de código.** Dois achados foram corrigidos depois da revisão: o rebaixamento da regra consolidada em rejeição ou correção, e a categoria do erro sem texto na tabela de execuções. (Correções da revisão; o coordenador completa a referência do commit.)
5. **Commits do PR #26.** Os commits `docs(spec)` do PR #26 são do dono e usam um escopo fora da lista de escopos; o merge por squash resolve.

## Como ativar no seu cofre

Para o dono, pela janela: Documentos › Regras de reconhecimento → a v2 do contracheque (IRRF) está em rascunho → "Testar" → "Ativar".

Antes de ativar no cofre real, a re-checagem no dado real com a tolerância de 1 centavo ainda não foi feita (AC-LUA-13). Ativação exige confirmação humana (D14).

## Comandos rodados

- Por este redator: somente leitura. Conferidos com `grep` os nomes de teste, as capturas, a CLI (`crates/sbf/src/regras.rs`), a janela (`crates/sbf-janela/src/app.rs`), a CI (`.github/workflows/rust.yml`) e o termo do glossário. `cargo` não foi reexecutado.
- Pelo coordenador (não reexecutado aqui): `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (1671 testes, 0 falhas).

## O que falta para o auditor

- **AC-LUA-08 (PARCIAL).** Página do trecho no `DeOnde`. Evidências do script são rótulos livres.
- **AC-LUA-12 (PARCIAL).** A CI só prova algo depois que o dono configurar o secret `BANCADA_TOKEN`.
- **AC-LUA-13 (FORA DO DIFF).** Re-checagem no dado real, depois da tolerância de 1 centavo, não feita. Conferir as capturas listadas acima.
- **Telegram e Gmail reais (D1).** Ficaram fora deste PR; só os testes fictícios do AC-LUA-09 existem.
- **Windows (D16 da spec 11).** Não verificado.
