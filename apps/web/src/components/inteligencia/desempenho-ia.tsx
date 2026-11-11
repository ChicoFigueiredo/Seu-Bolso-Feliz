/**
 * "Como a IA está indo" — números honestos, sem maquiagem: quantos documentos
 * ela processou sozinha, quantos precisaram de correção, quantos foram
 * rejeitados. Nenhuma taxa aparece quando não há base para calculá-la (ver
 * `taxaSemCorrecaoPct: null` em painel-inteligencia.ts).
 *
 * Sem estado e sem handlers — por isso este arquivo não é "use client". Quem
 * importa isto (`painel-inteligencia-view.tsx`) é que decide o limite
 * servidor/cliente; um componente puro pode viver dos dois lados sem
 * problema.
 */
import { Card, Icon, MetricLabel, MetricValue, ProgressBar } from "@sbf/design-system";
import type { DesempenhoIA as DesempenhoIADados } from "@/app/actions/painel-inteligencia";

export function DesempenhoIA({ desempenho }: { desempenho: DesempenhoIADados }) {
  const temTaxa = desempenho.taxaSemCorrecaoPct != null;

  return (
    <div style={{ display: "grid", gap: 12 }}>
      <div
        style={{
          display: "grid",
          gap: 10,
          gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))",
        }}
      >
        <Card padding={16}>
          <MetricLabel>Processados</MetricLabel>
          <MetricValue size="medium">{desempenho.processados}</MetricValue>
          <div style={{ fontSize: 12, color: "var(--text-3)" }}>
            de {desempenho.totalDocumentos} documento{desempenho.totalDocumentos === 1 ? "" : "s"}{" "}
            no total
          </div>
        </Card>
        <Card padding={16}>
          <MetricLabel>Sem correção</MetricLabel>
          <MetricValue size="medium" color="var(--success)">
            {desempenho.semCorrecao}
          </MetricValue>
          <div style={{ fontSize: 12, color: "var(--text-3)" }}>a IA acertou sozinha</div>
        </Card>
        <Card padding={16}>
          <MetricLabel>Precisaram de ajuste</MetricLabel>
          <MetricValue size="medium">{desempenho.comCorrecao}</MetricValue>
          <div style={{ fontSize: 12, color: "var(--text-3)" }}>
            você corrigiu antes de confirmar
          </div>
        </Card>
        <Card padding={16}>
          <MetricLabel>Rejeitados</MetricLabel>
          <MetricValue size="medium">{desempenho.rejeitados}</MetricValue>
          <div style={{ fontSize: 12, color: "var(--text-3)" }}>sugestão descartada</div>
        </Card>
      </div>

      {temTaxa ? (
        <ProgressBar
          value={(desempenho.taxaSemCorrecaoPct ?? 0) / 100}
          label={`${desempenho.taxaSemCorrecaoPct}% sem nenhuma correção`}
          hint={`${desempenho.processados} documento${desempenho.processados === 1 ? "" : "s"} processado${
            desempenho.processados === 1 ? "" : "s"
          }`}
        />
      ) : (
        <div style={{ fontSize: 12.5, color: "var(--text-3)" }}>
          Ainda não há documentos processados o suficiente para calcular uma taxa de acerto.
        </div>
      )}

      {desempenho.erros24h > 0 ? (
        <div
          style={{
            display: "flex",
            gap: 8,
            alignItems: "center",
            fontSize: 12.5,
            color: "var(--warn)",
          }}
        >
          <Icon name="warning" size={16} />
          <span>
            {desempenho.erros24h} erro{desempenho.erros24h === 1 ? "" : "s"} de processamento nas
            últimas 24 horas.
          </span>
        </div>
      ) : null}
    </div>
  );
}
