"use client";

/**
 * A tela Inteligência: onde a IA presta contas. Três perguntas, três seções,
 * nesta ordem — a do que pede ação primeiro, a do que ela aprendeu depois, a
 * de como ela está indo por último:
 *
 *   - O que ela ainda quer confirmar (fila, com confiança sempre à vista).
 *   - O que ela aprendeu com as correções (padrões, com taxa de acerto).
 *   - Como ela está indo (números honestos, sem maquiagem).
 *
 * Mesmo formato de painel-hoje-view.tsx: a página (Server Component) chama
 * `carregarPainelInteligencia()` e passa os dados prontos para cá. Nenhuma
 * seção lê o banco diretamente.
 */
import { SectionTitle } from "@sbf/design-system";
import type { PainelInteligencia } from "@/app/actions/painel-inteligencia";
import { DesempenhoIA } from "./desempenho-ia";
import { FilaConfirmacao } from "./fila-confirmacao";
import { PadroesAprendidos } from "./padroes-aprendidos";

export function PainelInteligenciaView({ dados }: { dados: PainelInteligencia }) {
  return (
    <div style={{ display: "grid", gap: 24, paddingBottom: 24 }}>
      <section style={{ display: "grid", gap: 12 }}>
        <SectionTitle>Esperando sua confirmação</SectionTitle>
        <FilaConfirmacao itensIniciais={dados.fila} totalPendente={dados.filaTotalPendente} />
      </section>

      <section style={{ display: "grid", gap: 12 }}>
        <SectionTitle>Padrões aprendidos</SectionTitle>
        <PadroesAprendidos padroesIniciais={dados.padroes} />
      </section>

      <section style={{ display: "grid", gap: 12 }}>
        <SectionTitle>Como a IA está indo</SectionTitle>
        <DesempenhoIA desempenho={dados.desempenho} />
      </section>
    </div>
  );
}
