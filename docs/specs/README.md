# docs/specs — Fonte da verdade do Seu Bolso Feliz

Specs autoritativos, **um por capacidade**, verificados contra o código (não contra docs).
Substituem a pilha de ~54 documentos antigos (agora em [`../_arquivo/`](../_arquivo/), histórico).

## Como ler

1. Comece sempre por **[`00-estado-real.md`](00-estado-real.md)** — o mapa único do que está
   pronto / parcial / faltando, com evidência de código.
2. Cada spec de capacidade segue o mesmo template:
   **Objetivo → Contrato (estado real verificado) → Gaps com critério de aceite → Referências de código.**

## Índice

| Spec                                                | Capacidade                        | Estado resumido                |
| --------------------------------------------------- | --------------------------------- | ------------------------------ |
| [00-estado-real](00-estado-real.md)                 | Mapa geral + saúde do código      | —                              |
| [01-ingestao](01-ingestao.md)                       | Pipeline de documentos (3 canais) | ✅ construído e testado        |
| [02-conciliacao](02-conciliacao.md)                 | Documento × registros             | ✅ motor · 🟡 UX               |
| [03-padroes-documentais](03-padroes-documentais.md) | Memória operacional               | ✅ modelo · 🟡 ligação parsing |
| [04-ia-e-chat](04-ia-e-chat.md)                     | Interface inteligente             | ✅ integrado                   |
| [05-dominio-financeiro](05-dominio-financeiro.md)   | Núcleo determinístico             | ✅ testado                     |

## Regras de manutenção (spec-driven)

- **O código vence.** Se um spec divergir do código, o spec está errado — corrija o spec ou o código no mesmo PR.
- **Nada vira ✅ sem evidência** (caminho de arquivo + teste passando ou verificação manual registrada).
- **Um item fechado atualiza `00-estado-real.md` no mesmo commit.**
- Specs descrevem **comportamento e contrato**, não atas de reunião. Sem roleplay.
