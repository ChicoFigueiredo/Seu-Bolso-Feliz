# Spec 14 — Caixa de entrada: a fila do reconhecimento local

Data: 2026-11-20. Estado: implementada; o aceite abaixo foi informado pelo implementador e deve ser reexecutado pelo auditor.

Termos do glossário (`CONTEXT.md`): **Caixa de entrada**, **Estados do documento**, **Sem padrão de reconhecimento**, **Origem**, **Rastro**.

## Objetivo

Os documentos que o reconhecimento vê chegar (Telegram, Gmail, pasta vigiada, arrastar na janela, `sbf reconhecer`) não ficavam disponíveis para o usuário decidir depois: o painel Reconhecimento do dock só mostra o que foi arrastado na sessão. A Caixa de entrada guarda a fila e a mostra na janela.

## Contrato

- **Onde fica.** Tabela `caixa_entrada` no `inventario.sqlite` (migração 4, `crates/sbf-inventario/sql/0004-caixa-de-entrada.sql`; módulo `sbf_inventario::caixa_de_entrada`). O conteúdo já é um achado do inventário, e o worker, a CLI e a janela já abrem esse banco; a fila não precisa de um segundo arquivo. `IF NOT EXISTS` e `garantir_tabela` valem para quem grava antes de a janela migrar o banco. Nunca guarda conteúdo de documento: só nome, tipo, resumo curto e estado.
- **Uma linha por `content_hash`.** Idempotente; a origem e a chegada são as da primeira vez; `vezes` conta as rodadas. Só entra o que passa pelo reconhecimento depois da migração (não retroage nos achados antigos).
- **Origem:** `telegram`, `gmail`, `celular`, `pasta`, `arrastado`, `cli`. Quem registra é o `ReconhecedorDoCofre` (`com_fila(origem)`, ou o canal de `reconhecer_de`: Gmail e Telegram). Sem `com_fila`, nada é registrado (a releitura da janela não mexe na fila).
- **Estados** (glossário): Processando (durante o reconhecimento), Pronto, Precisa revisar (regra ambígua, `revisar` da regra, avisos, sugestão da IA), Possível duplicidade (o mesmo conteúdo com duas ou mais origens fora a fonte `arrastado`), Protegido por senha, Não reconhecido. Falha ou cancelamento depois de saber o conteúdo vira Não reconhecido, nunca fica em Processando.
- **Situação do usuário:** aberta, resolvida, dispensada. Só a chegada automática de conteúdo repetido não reabre; arrastar de novo e `sbf reconhecer` reabrem.
- **Divisão em duas listas** (mesma tabela, estado decide):
  - **Caixa de entrada** (Documentos): os reconhecidos (Pronto, Precisa revisar, Possível duplicidade, mais Processando). Contador no painel da esquerda = reconhecidos abertos. Filtros: Não resolvidos (padrão) ou Todos; por origem.
  - **A triar** (Inventário): seção "Sem padrão de reconhecimento" no topo, com Não reconhecido e Protegido por senha abertos, mais recentes primeiro, antes da tabela de achados de sempre (que não muda). O contador de A triar no painel continua o do inventário (os sem padrão já são achados a triar, então somar contaria duas vezes); a seção mostra o número deles.
- **Dock.** Escolher uma linha mostra o mesmo cartão do painel Reconhecimento, recalculado na hora pelo texto já guardado e sem IA (`reconhecer_conteudo`, lê a cópia do acervo), e a Prévia e o Texto extraído do achado. Confirmar uma sugestão segue o caminho de sempre (`confirmar_sugestao`: Rastro e Desfazer). O cartão de um item sem padrão põe "Criar regra a partir daqui" em destaque.
- **Ações da linha:** Resolver, Dispensar, Reabrir, Reconhecer de novo (roda as regras de agora e atualiza a linha: um item sem padrão que passa a casar sai de A triar e entra na Caixa de entrada). Confirmar ou ignorar todas as sugestões do cartão, com ao menos uma confirmada, resolve o item sozinho.
- **Pasta vigiada.** O worker reconhece só o que o vigia acabou de indexar (`Destino::Novo`); a reconciliação periódica não, para não enfileirar o acervo antigo.

## Aceite (informado pelo implementador)

| ID    | Critério                                                                                                                                                                                  | Evidência                                                                                                             |
| ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| CX-01 | Persistência idempotente, origem da chegada, contadores, filtros                                                                                                                          | `crates/sbf-inventario/src/caixa_de_entrada.rs` (testes); migração 4 em `crates/sbf-inventario/tests/nucleo_banco.rs` |
| CX-02 | O reconhecimento registra tipo, resumo, estado e origem; duplicidade; reabertura                                                                                                          | `crates/sbf-reconhecimento/tests/fila.rs`                                                                             |
| CX-03 | O worker reconhece o arquivo novo da pasta, não o da reconciliação                                                                                                                        | `crates/sbf-worker/tests/laco.rs::so_o_arquivo_que_o_vigia_acabou_de_indexar_passa_pelo_reconhecimento`               |
| CX-04 | A fila real lista, recalcula o cartão do acervo, reconhece de novo e resolve                                                                                                              | `crates/sbf/src/fila.rs::testes`                                                                                      |
| CX-05 | Áreas desenham; escolher a linha põe cartão e Prévia no dock; resolver/dispensar/reabrir; confirmar tudo resolve; sem padrão → Caixa ao reconhecer de novo; soltar arquivo devolve o dock | `crates/sbf-janela/tests/caixa_de_entrada.rs`                                                                         |

## Fora do escopo

- Origem `celular`: existe no vocabulário, mas nenhum caminho a grava ainda (o celular entrará pelo servidor).
- Cartão com IA: a releitura da Caixa de entrada nunca chama IA (o arrastar continua chamando quando os leitores locais não reconhecem).
- As conciliações automáticas de três provas só valem no arrastar; na Caixa de entrada o clique decide.
