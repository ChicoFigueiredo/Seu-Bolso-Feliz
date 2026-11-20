# Spec 14 — Caixa de entrada: os documentos que o reconhecimento viu chegar

Data: 2026-11-20 (revisada no mesmo dia). Estado: implementada; o aceite abaixo foi informado pelo implementador e deve ser reexecutado pelo auditor.

Termos do glossário (`CONTEXT.md`): **Caixa de entrada**, **Estados do documento**, **Canal**, **Sem padrão de reconhecimento**, **Origem**, **Rastro**.

## Objetivo

Os documentos que o reconhecimento vê chegar (Telegram, Gmail, pasta vigiada, arrastar na janela, `sbf reconhecer`) não ficavam disponíveis para o usuário decidir depois: o painel Reconhecimento do dock só mostra o que foi arrastado na sessão. A Caixa de entrada guarda o que chegou e o mostra na janela.

## Contrato

- **Onde fica.** Tabela `caixa_entrada` no `inventario.sqlite` (migração 4, `crates/sbf-inventario/sql/0004-caixa-de-entrada.sql`; módulo `sbf_inventario::caixa_de_entrada`). O conteúdo já é um achado do inventário, e o worker, a CLI e a janela já abrem esse banco. `IF NOT EXISTS` e `garantir_tabela` valem para quem grava antes de a janela migrar o banco. Nunca guarda conteúdo de documento: só nome, tipo, resumo curto e estado. `meta.caixa_entrada_desde` guarda quando a tabela nasceu neste banco.
- **Uma linha por `content_hash`.** Idempotente; `canal` e a chegada são os da primeira vez; `vezes` conta as rodadas. Só entra o que passa pelo reconhecimento depois da migração (não retroage nos achados antigos).
- **Canal, não Origem.** A coluna `canal` (`telegram`, `gmail`, `celular`, `pasta`, `arrastado`, `cli`) é por onde chegou da primeira vez; na interface, "Chegou por". Origem, no glossário, é cada lugar onde o conteúdo foi achado. A pasta vigiada cuja fonte no `worker.json` é `celular` chega como `celular`; as demais, como `pasta`. Código desconhecido no banco é erro de leitura, nunca vira outro valor.
- **Quem registra.** O `ReconhecedorDoCofre` (`com_caixa_de_entrada(canal, reabrir)`, `reconhecer_chegada`, ou o canal de `reconhecer_de`: Gmail e Telegram). Sem isso, nada é registrado (o cartão que a janela recalcula não mexe na Caixa).
- **Estados** (glossário): Processando (durante o reconhecimento), Pronto, Precisa revisar (regra ambígua, `revisar` da regra, avisos, sugestão da IA, **leitura que falhou**), Possível duplicidade (o mesmo conteúdo com duas ou mais origens fora a fonte `arrastado`), Protegido por senha, Não reconhecido. Falha de leitura (PDF que não abre, pdftotext ou OCR que erram, arquivo que sumiu, cancelamento) vira Precisa revisar com o resumo "a leitura falhou — Reconhecer de novo": fica na Caixa de entrada, **nunca** em "sem padrão de reconhecimento", onde o usuário cria regras. As listas do SQL saem do enum de estados (uma fonte da verdade).
- **Processando** não aparece em lista nenhuma nem no contador dos reconhecidos: é um indicador à parte ("N processando") no cabeçalho da Caixa de entrada.
- **Situação do usuário:** aberta, resolvida, dispensada. Resolver, Dispensar e Reabrir são **decisões do usuário no log** (`decisao`, alvo `achado`, campo `caixa_entrada`, forma `{"situacao": ...}`): entram no Rastro e o Desfazer de sempre as desfaz; a coluna `situacao` é a projeção. Não mudam `achado.estado` nem `decidido_por`. Só arrastar de novo e `sbf reconhecer` (ação explícita) reabrem; a chegada automática de conteúdo repetido não.
- **Reconhecer de novo** recalcula e só muda o item de lista (A triar ↔ Caixa de entrada): atualiza a linha que existe, não a cria e não reabre o que foi resolvido ou dispensado (`atualizando_a_caixa_de_entrada`).
- **Divisão em duas listas** (mesma tabela, o estado decide):
  - **Caixa de entrada** (Documentos): Pronto, Precisa revisar, Possível duplicidade. Contador no painel da esquerda = esses abertos. Filtros: Não resolvidos (padrão) ou Todos; por canal.
  - **A triar** (Inventário): seção "Sem padrão de reconhecimento" no topo, com Não reconhecido e Protegido por senha, mais recentes primeiro, com o mesmo filtro Não resolvidos / Todos. A tabela de achados de sempre não muda. O contador de A triar no painel continua o do inventário (os sem padrão já são achados a triar, então somar contaria duas vezes); a seção mostra o número deles.
- **Dock.** Escolher uma linha mostra o mesmo cartão do painel Reconhecimento, recalculado na hora **só com o texto já guardado**, sem IA, sem extração e sem OCR (`so_com_texto_guardado`, lê a cópia do acervo): roda na thread da interface. Sem texto guardado, o cartão diz "sem texto guardado" e "Reconhecer de novo" extrai, **numa thread à parte**. Mostra também a Prévia e o Texto extraído do achado. Confirmar uma sugestão segue o caminho de sempre (`confirmar_sugestao`: Rastro e Desfazer). O cartão de um item sem padrão põe "Criar regra a partir daqui" em destaque.
- **Ações da linha:** Resolver, Dispensar, Reabrir, Reconhecer de novo. Confirmar ou ignorar todas as sugestões do cartão, com ao menos uma confirmada, resolve o item sozinho.
- **Pasta vigiada.** O vigia reconhece o arquivo que acabou de indexar (`Destino::Novo`). A reconciliação periódica reconhece só os **achados novos** que indexou: visto a partir de `caixa_entrada_desde` e sem linha na Caixa de entrada (o que chegou com o vigia parado), até 50 por pasta e por volta. Nunca o acervo antigo. Se o original sumiu, usa a cópia do acervo; sem ela, registra a falha (Precisa revisar) e não tenta de novo.

## Aceite (informado pelo implementador)

| ID    | Critério                                                                                                                                                                                                                  | Evidência                                                                                                             |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| CX-01 | Persistência idempotente, canal da chegada, contadores, Processando à parte, filtros, código desconhecido é erro, listas do SQL do enum, pendentes só os novos sem linha                                                  | `crates/sbf-inventario/src/caixa_de_entrada.rs` (testes); migração 4 em `crates/sbf-inventario/tests/nucleo_banco.rs` |
| CX-02 | Resolver/Dispensar/Reabrir no log com Desfazer, sem tocar `achado.estado` nem `decidido_por`                                                                                                                              | `caixa_de_entrada::testes::decidir_*`                                                                                 |
| CX-03 | O reconhecimento registra tipo, resumo, estado e canal; duplicidade; reabertura só por ação do usuário; Reconhecer de novo só atualiza; falha de leitura vai para Precisa revisar; só texto guardado não extrai           | `crates/sbf-reconhecimento/tests/caixa_de_entrada.rs`                                                                 |
| CX-04 | O vigia reconhece o arquivo novo; a reconciliação reconhece o que chegou com o worker parado e não o acervo antigo; canal `celular` pela fonte                                                                            | `crates/sbf-worker/tests/laco.rs`; `Canal::da_fonte_da_pasta` em `caixa_de_entrada::testes`                           |
| CX-05 | A Caixa real lista, recalcula o cartão do acervo, reconhece de novo sem reabrir e resolve pelo log                                                                                                                        | `crates/sbf/src/caixa_de_entrada.rs::testes`                                                                          |
| CX-06 | Áreas desenham; escolher a linha põe cartão e Prévia no dock; resolver/dispensar/reabrir; confirmar tudo resolve; sem padrão → Caixa ao reconhecer de novo (thread); filtro Todos na seção; soltar arquivo devolve o dock | `crates/sbf-janela/tests/caixa_de_entrada.rs`                                                                         |

## Fora do escopo

- Origem `celular` só aparece se o `worker.json` tiver uma pasta de fonte `celular`; nada além disso grava esse canal ainda.
- A releitura da Caixa de entrada nunca chama IA (o arrastar continua chamando quando os leitores locais não reconhecem).
- As conciliações automáticas de três provas só valem no arrastar; na Caixa de entrada o clique decide.
