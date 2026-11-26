# ADR-021 — Regras de reconhecimento versionadas, com scripts Luau num processo filho

**Data:** 2026-11-17 · **Status:** aceita · **Spec:** [13](../specs/13-motor-regras-lua.md)

## Contexto e decisão

Cada tipo novo de documento hoje exige código Rust e uma versão nova do `sbf`: o leitor do
contracheque é escolhido por um `contains("demonstrativo de pagamento")` fixo em
`sbf-reconhecimento`. O CEO quer cadastrar um comprovante novo, ou mudar o tratamento de
um tipo, sem recompilar, e quer que a correção feita na caixa de entrada vire uma regra.

O CEO decidiu (grill de 2026-11-17):

- **Regra de reconhecimento** é dado versionado num `regras.sqlite` próprio do cofre, do
  crate `sbf-regras`. Inventário e planejamento não ganham migração; sem o arquivo, tudo
  funciona como antes.
- A parte declarativa (detectar, extrair, propor) é JSON. O que é algoritmo vai num
  **script** em **Luau**, executado num **processo filho** do `sbf`: sem rede, sem arquivo,
  sem segredo, com limite de memória, de tempo e de instruções, e encerrado no timeout.
- Uma regra **só propõe**. Depois de 5 propostas aceitas seguidas sem correção, o sbf
  sugere torná-la **consolidada**, e só o usuário aprova; consolidada, ela concilia sozinha
  apenas com as três provas exatas, com Desfazer. Rejeição, Desfazer ou versão nova a
  devolvem a ativa.
- Ativar, consolidar e reverter é só na janela ou num terminal interativo. O MCP e a IA
  criam e refinam rascunhos, nunca os ativam.
- O formato do desktop é o canônico; o `document_patterns` do web fica mapeado em documento,
  sem sincronização nem remoção do TypeScript.

## Alternativas descartadas

- **Lua 5.4 no próprio processo**: mais rápido, mas um escape da sandbox montada por nós
  rodaria com o acesso do `sbf` ao cofre e ao chaveiro.
- **Lua 5.4 num processo filho**: inteiros nativos, mas a sandbox seria nossa; Luau já foi
  desenhado para código não confiável (ambiente somente leitura, sem bytecode externo,
  interrupção cooperativa). Os centavos voltam como número e o Rust recusa o que não for
  inteiro.
- **Tabelas no `inventario.sqlite`**: aproveitaria o log de decisões, mas mexeria no banco
  mais valioso do cofre por uma funcionalidade nova.
- **Regras absorvendo as assinaturas da spec 11**: um catálogo só, mas troca o que já casa
  registros do banco com compromissos. As regras extraem campos; as assinaturas seguem
  casando.

## Consequências

- **(2026-11-18, D15) O MCP tem controle total.** Ele ativa, desativa, reverte e consolida
  regras (`Quem::Mcp`, gravado em `historico_de_estado`) e aplica direto no planejamento, com
  autor `mcp` no Rastro e Desfazer. Isso substitui a parte da D14 que proibia o MCP de ativar.
  A D12 vale para todos (exemplo falhando bloqueia, inclusive o MCP). Antes da primeira
  escrita de cada banco por sessão há backup pela API de backup do SQLite; se falha, a escrita
  é recusada.
- **Nenhuma escrita financeira sai de uma regra.** O tipo da proposta não tem variante para
  lançar, pagar ou criar movimentação; quem aplica é o Rust, pelas APIs do inventário e do
  planejamento, depois do clique (ou das três provas, na consolidada).
- **Determinismo auditável.** A mesma entrada, com a mesma versão e os mesmos parâmetros,
  dá os mesmos bytes e a mesma chave de idempotência; cada campo aponta para o trecho, a
  regra e a versão de onde veio, e entra no Rastro.
- **Custo de um processo por lote**, não por documento: o filho fica vivo durante uma
  rodada e é reiniciado depois de qualquer erro de limite.
- **Dependência C++** (o Luau é compilado junto, via `mlua`). Windows segue adiado (D16 da
  spec 11).
- **A CI ganha um job Rust**, que baixa a bancada (privada) com um token de leitura.
