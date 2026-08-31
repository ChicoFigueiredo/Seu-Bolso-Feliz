"use client";

/**
 * As regras que o sistema deduziu ("todo débito com 'CEMIG' é conta de luz na
 * categoria X") — `document_patterns`, geridos hoje pela tela em shadcn
 * `/dashboard/ingestion/patterns` (não editada nem removida por este
 * trabalho). Esta view não duplica aquela tela: reaproveita a mesma fórmula
 * de taxa de sucesso e o mesmo critério de "em risco" (via
 * `carregarPainelInteligencia`), e chama `deactivatePattern`/
 * `reactivatePattern` — as Server Actions já existentes em
 * `@/app/actions/patterns` — diretamente, sem duplicar a lógica de posse que
 * elas já fazem.
 */
import { useState } from "react";
import { Button, Card, ChipGroup, Icon, Pill, ProgressBar } from "@sbf/design-system";
import { deactivatePattern, reactivatePattern } from "@/app/actions/patterns";
import type { PadraoAprendido } from "@/app/actions/painel-inteligencia";
import { AvisoInline } from "./aviso-inline";

export interface PadroesAprendidosProps {
  padroesIniciais: PadraoAprendido[];
}

type Filtro = "ativos" | "risco" | "inativos" | "todos";

export function PadroesAprendidos({ padroesIniciais }: PadroesAprendidosProps) {
  const [padroes, setPadroes] = useState(padroesIniciais);
  const [filtro, setFiltro] = useState<Filtro>("ativos");
  const [carregandoId, setCarregandoId] = useState<string | null>(null);
  const [erro, setErro] = useState<string | null>(null);

  if (padroes.length === 0) {
    return (
      <Card variant="quiet">
        <div
          style={{
            display: "flex",
            gap: 10,
            alignItems: "center",
            fontSize: 13,
            color: "var(--text-3)",
          }}
        >
          <Icon name="school" size={18} />
          <span>
            A IA ainda não deduziu nenhum padrão. Isso acontece conforme você confirma e corrige
            documentos — cada correção ensina uma regra nova.
          </span>
        </div>
      </Card>
    );
  }

  const contagens = {
    ativos: padroes.filter((p) => p.ativo).length,
    risco: padroes.filter((p) => p.emRisco).length,
    inativos: padroes.filter((p) => !p.ativo).length,
  };

  const visiveis = padroes.filter((p) => {
    if (filtro === "todos") return true;
    if (filtro === "ativos") return p.ativo;
    if (filtro === "risco") return p.emRisco;
    return !p.ativo;
  });

  async function alternar(padrao: PadraoAprendido) {
    setErro(null);
    setCarregandoId(padrao.id);
    try {
      if (padrao.ativo) await deactivatePattern(padrao.id);
      else await reactivatePattern(padrao.id);
      setPadroes((prev) =>
        prev.map((p) =>
          p.id === padrao.id ? { ...p, ativo: !p.ativo, emRisco: p.ativo ? false : p.emRisco } : p,
        ),
      );
    } catch (e) {
      setErro(e instanceof Error ? e.message : "Não foi possível atualizar o padrão.");
    } finally {
      setCarregandoId(null);
    }
  }

  return (
    <div style={{ display: "grid", gap: 12 }}>
      <ChipGroup
        scroll
        value={filtro}
        onChange={(v) => setFiltro(v as Filtro)}
        options={[
          { value: "ativos", label: "Ativos", count: contagens.ativos },
          { value: "risco", label: "Em risco", count: contagens.risco },
          { value: "inativos", label: "Inativos", count: contagens.inativos },
          { value: "todos", label: "Todos", count: padroes.length },
        ]}
      />

      {erro ? <AvisoInline>{erro}</AvisoInline> : null}

      {visiveis.length === 0 ? (
        <div style={{ fontSize: 13, color: "var(--text-3)", padding: "8px 4px" }}>
          Nenhum padrão neste filtro.
        </div>
      ) : (
        <div style={{ display: "grid", gap: 10 }}>
          {visiveis.map((p) => (
            <Card key={p.id} padding={16} style={{ gap: 10 }}>
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  gap: 10,
                  alignItems: "flex-start",
                }}
              >
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontSize: 14.5, fontWeight: 700, color: "var(--text)" }}>
                    {p.nome}
                  </div>
                  <div style={{ fontSize: 12, color: "var(--text-3)", marginTop: 2 }}>
                    {p.tipoDocumento}
                  </div>
                </div>
                {p.emRisco ? (
                  <Pill tone="danger" icon="warning" size="xs">
                    Em risco
                  </Pill>
                ) : !p.ativo ? (
                  <Pill tone="neutral" size="xs">
                    Inativo
                  </Pill>
                ) : (
                  <Pill tone="primary" icon="check_circle" size="xs">
                    Ativo
                  </Pill>
                )}
              </div>

              <ProgressBar
                value={p.taxaSucessoPct / 100}
                fill={p.emRisco ? "var(--danger)" : "var(--primary)"}
                labelColor={p.emRisco ? "var(--danger)" : "var(--primary-ink)"}
                label={`${p.taxaSucessoPct}% de acerto`}
                hint={`${p.acertos} acerto${p.acertos === 1 ? "" : "s"} · ${p.correcoes} correç${
                  p.correcoes === 1 ? "ão" : "ões"
                }`}
              />

              <Button
                variant={p.ativo ? "outline" : "quiet"}
                size="sm"
                onClick={() => alternar(p)}
                disabled={carregandoId === p.id}
              >
                {carregandoId === p.id
                  ? "Aguarde…"
                  : p.ativo
                    ? "Desativar padrão"
                    : "Reativar padrão"}
              </Button>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
