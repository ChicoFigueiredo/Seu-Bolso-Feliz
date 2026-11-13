# Relatório de aceite da fatia 1 (rascunho)

Rascunho da branch `feat/desktop-empacotamento`, base `b3f7f36`. O integrador completa
às 06:00 de 2026-11-14 depois de juntar as branches. Estados: PASS, FAIL, UNVERIFIED, N/A.
"Pendente de integração" é UNVERIFIED até a branch do critério entrar.

Como reproduzir: `scripts/desktop/conferir.sh` (fmt, clippy, `cargo test --workspace`,
`bun test` do worker, build release).

| ID     | Estado                        | Evidência |
| ------ | ----------------------------- | --------- |
| INV-01 | PASS (núcleo)                 | `crates/sbf-inventario/tests/nucleo_banco.rs`: `inv01_abrir_e_ver_o_plano_nao_mudam_o_banco`, `inv01_nenhuma_decisao_antes_de_migrar`, `inv01_migrar_faz_a_copia_antes`, `inv01_depois_de_migrar_abrir_de_novo_nao_muda_nada`. O aviso na janela é pendente de integração. |
| INV-02 | PASS                          | `nucleo_banco.rs`: `inv02_banco_do_bun_2_vira_user_version_2`, `inv02_rust_recusa_banco_mais_novo_sem_tocar_nele`, `inv02_banco_vazio_e_banco_do_bun_terminam_com_o_mesmo_esquema`, mais duas recusas de banco estranho. A metade "o Bun recusa banco mais novo" não tem teste com id `inv02` nesta branch: UNVERIFIED (conferir em `workers/inventario`). |
| INV-03 | pendente de integração        | As 12 vistas ainda não estão nesta branch. |
| INV-04 | pendente de integração        | Visão geral e comparação com `relatorio.ts`. |
| INV-05 | PASS                          | `nucleo_decisoes.rs` (`inv05_*`, 17 testes, todas as Ações do §10) e `nucleo_reprojetar.rs::inv05_reprojetar_reproduz_o_estado` (propriedade). |
| INV-06 | PASS                          | `nucleo_bun.rs::inv06_e3_triar_do_bun_nao_muda_o_que_o_usuario_decidiu`. |
| INV-07 | PASS                          | `nucleo_banco.rs::inv07_decisao_recusa_update_e_delete`. |
| INV-08 | PASS                          | `crates/sbf-chaveiro/tests/importacao.rs`: `senha_errada_e_arquivo_adulterado_dao_o_mesmo_erro`, `importa_para_o_keyring_do_sistema`, `sem_keyring_importa_para_o_arquivo_de_fallback_com_a_mesma_senha_mestra`, `conferir_que_nenhum_arquivo_tem_valor` (busca pelo valor), `o_crate_nao_loga`. |
| INV-09 | PASS                          | `sbf-pdf/tests/rodada.rs::abre_pela_regra_com_o_texto_do_pdftotext_e_sem_senha_no_banco`, `n_falhas_viram_pendencia_e_as_tentativas_param`, `estrategia_padrao_vencedora_vira_proposta_e_so_grava_com_confirmacao`; `sem_vazamento.rs::a_senha_nao_aparece_em_argv_nem_ambiente_de_nenhum_processo` (/proc), `nenhum_arquivo_e_criado_na_abertura` (strace; `/usr/bin/strace` existe nesta máquina, então rodou de verdade), `o_crate_nao_loga`. |
| INV-10 | PARCIAL: PASS no núcleo       | `nucleo_lotes.rs`: `inv10_aprovar_lote_marca_os_registros_como_decisoes`, `inv10_enviar_ao_servidor_fica_desabilitado_com_o_motivo` e outros. O botão desabilitado na janela é pendente de integração. |
| INV-11 | pendente de integração        | Estante e coleta do Drive (`sbf-estante`). |
| INV-12 | pendente de integração        | Restauração da cópia cifrada. |
| INV-13 | pendente de integração        | Aviso de disco sem cifra. |
| INV-14 | pendente de integração        | Shell (`bancada-casca`) e janela. |
| INV-15 | UNVERIFIED                    | Linux: pendente da janela. Windows e macOS: sem VM; os scripts `empacotar-macos.sh` e `empacotar-windows.ps1` não foram executados. Cross-build para Windows não foi possível aqui (falta `x86_64-w64-mingw32-gcc` e o alvo no rustup; `cargo-xwin` está instalado, mas baixa o SDK da Microsoft, rede fora da regra). |
| INV-16 | UNVERIFIED                    | Exige o inventário real, que esta rodada não pode abrir; medir às 06:00 pelo usuário. |
| INV-17 | PASS com ressalvas            | Ver "Varredura INV-17" abaixo. |
| INV-18 | pendente de integração        | Respostas do §11. |
| INV-19 | pendente de integração        | Links de item e páginas. |
| INV-20 | pendente de integração        | Rastro e totais navegáveis. |

## Varredura INV-17

Comando (Python, sobre `git ls-files crates workers/inventario docs/specs docs/design`,
119 arquivos): CPF com dígitos verificadores válidos, número de 16 dígitos com Luhn,
e-mail fora de `example/teste/localhost`, e o literal `/mnt/d/Chico/financas`.

- **CPF válido: 17 ocorrências, todas em 2 valores**: `529.982.247-25` e `111.444.777-35`.
  São os CPFs de exemplo públicos e amplamente usados em material de validação, usados de
  propósito como fixture (`sbf-chaveiro/src/cpf.rs`, `tests/importacao.rs`, `sbf-pdf`,
  `workers/inventario/src/cpf.test.ts`, spike do PDF protegido). Ressalva: só o titular
  pode afirmar que nenhum dos dois é o dele. Julgamento: sintéticos.
- **Cartão (Luhn): 0.**
- **E-mail: 15 ocorrências, nenhuma pessoal.** `pdf-parse@1.1.1` é versão de pacote. O resto
  são remetentes genéricos de empresas em `extrair.test.ts`, `gmail-mensagem.test.ts` e
  `triagem.test.ts` (`fatura@banco.com.br`, `amigo@gmail.com`, `x@loja.com.br`...), sem
  nome de pessoa.
- **`/mnt/d/Chico/financas`: 6 ocorrências, todas em código do worker Bun, nenhuma em
  fixture de dados**: o padrão `RAIZ_PADRAO` (`cofre.ts`), uma regra de áreas ignoradas
  (`areas.ts`) e seu teste (`areas.test.ts`, só passa a string para `classeDe`), um texto
  de relatório (`relatorio-encerramento.ts`) e um comentário de uso (`scan-discos.ts`). É o
  caminho do cofre, não conteúdo dele; o teste não lê o disco. A decisão de trocar o
  caminho por configuração fica com o CEO.
- Não foram varridos os binários nem o `docs/` fora dos listados.

## Resultado do `conferir.sh` nesta branch

fmt, clippy `-D warnings`, `cargo test --workspace`, `bun test` do worker e build release:
todos PASS. `empacotar-linux.sh --sem-build` gerou o `.tar.gz` e o `.deb`
(`dpkg-deb -I` confere os campos); `gerar-licencas.sh` rodou com o `cargo-about` instalado.
O `sbf` não tem campo `license` no `Cargo.toml` (aviso do cargo-about).
