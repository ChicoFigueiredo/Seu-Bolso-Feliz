"use client";

import { useState, useTransition } from "react";
import {
  Card,
  Icon,
  Pill,
  SectionTitle,
  formatBRL,
  formatDate,
  type SbfTone,
} from "@sbf/design-system";
import { getStatementItems } from "@/app/actions/statement-cycles";
import type { CartaoDetalhe, FaturaResumo } from "@/app/actions/painel-contas";

type ItemFatura = Awaited<ReturnType<typeof getStatementItems>>[number];

// Mesmo vocabulário de apps/web/src/app/dashboard/statements/page.tsx
// (statusLabels), para não ter dois nomes para o mesmo status entre a tela
// antiga e esta.
const TOM_STATUS: Record<string, SbfTone> = {
  open: "neutral",
  closed: "warn",
  partial: "warn",
  overdue: "danger",
  paid: "primary",
};

const ROTULO_STATUS: Record<string, string> = {
  open: "Aberta",
  closed: "Fechada",
  partial: "Parcial",
  overdue: "Atrasada",
  paid: "Paga",
};

function mesReferencia(iso: string): string {
  const d = new Date(`${iso}T12:00:00`);
  const texto = d.toLocaleDateString("pt-BR", { month: "long", year: "numeric" });
  return texto.charAt(0).toUpperCase() + texto.slice(1);
}

/**
 * Ciclos de fatura do cartão selecionado — "o que já está lançado" de cada
 * um. Cada fatura é uma linha; abrir uma busca os itens sob demanda (via
 * `getStatementItems`, que já existe e já filtra por dono) e guarda em
 * memória para não buscar de novo ao fechar/abrir de novo.
 */
export function FaturasSection({ cartao }: { cartao: CartaoDetalhe }) {
  const [expandidoId, setExpandidoId] = useState<string | null>(null);
  const [itensPorCiclo, setItensPorCiclo] = useState<Record<string, ItemFatura[] | "carregando">>(
    {},
  );
  const [, startTransition] = useTransition();

  function alternar(fatura: FaturaResumo) {
    const proximo = expandidoId === fatura.id ? null : fatura.id;
    setExpandidoId(proximo);
    if (proximo && !(proximo in itensPorCiclo)) {
      setItensPorCiclo((atual) => ({ ...atual, [proximo]: "carregando" }));
      startTransition(async () => {
        const itens = await getStatementItems(fatura.id);
        setItensPorCiclo((atual) => ({ ...atual, [proximo]: itens }));
      });
    }
  }

  return (
    <section style={{ display: "grid", gap: 12 }}>
      <SectionTitle>{`Faturas de ${cartao.nome}`}</SectionTitle>

      {cartao.ciclos.length === 0 ? (
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
            <Icon name="receipt_long" size={18} />
            <span>Nenhuma fatura registrada ainda para este cartão.</span>
          </div>
        </Card>
      ) : (
        <Card variant="flush">
          {cartao.ciclos.map((fatura, indice) => {
            const aberta = expandidoId === fatura.id;
            const itens = itensPorCiclo[fatura.id];
            return (
              <div
                key={fatura.id}
                style={{ borderTop: indice === 0 ? "none" : "1px solid var(--line-soft)" }}
              >
                <button
                  type="button"
                  onClick={() => alternar(fatura)}
                  aria-expanded={aberta}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 12,
                    width: "100%",
                    minHeight: 44,
                    padding: "13px 16px",
                    border: 0,
                    background: "transparent",
                    cursor: "pointer",
                    textAlign: "left",
                    fontFamily: "var(--sbf-font-sans)",
                    color: "var(--text)",
                  }}
                >
                  <span style={{ flex: "1 1 auto", minWidth: 0 }}>
                    <span style={{ display: "block", fontSize: 13.5, fontWeight: 700 }}>
                      {mesReferencia(fatura.referenceMonth)}
                    </span>
                    <span
                      style={{
                        display: "block",
                        fontSize: 12,
                        color: "var(--text-3)",
                        marginTop: 2,
                      }}
                    >
                      {formatDate(fatura.cycleStart)} – {formatDate(fatura.cycleEnd)} · vence{" "}
                      {formatDate(fatura.dueDate)}
                    </span>
                  </span>
                  <Pill tone={TOM_STATUS[fatura.status] ?? "neutral"} size="xs">
                    {ROTULO_STATUS[fatura.status] ?? fatura.status}
                  </Pill>
                  <span
                    style={{ fontSize: 13.5, fontWeight: 800, minWidth: 84, textAlign: "right" }}
                  >
                    {formatBRL(fatura.total)}
                  </span>
                  <Icon
                    name={aberta ? "expand_less" : "expand_more"}
                    size={20}
                    style={{ color: "var(--text-4)" }}
                  />
                </button>

                {aberta ? (
                  <div style={{ padding: "0 16px 14px", display: "grid", gap: 8 }}>
                    {itens === "carregando" || itens === undefined ? (
                      <div style={{ fontSize: 12.5, color: "var(--text-3)" }}>
                        Carregando lançamentos…
                      </div>
                    ) : itens.length === 0 ? (
                      <div style={{ fontSize: 12.5, color: "var(--text-3)" }}>
                        Nenhum lançamento vinculado a esta fatura ainda.
                      </div>
                    ) : (
                      itens.map((item) => (
                        <div
                          key={item.id}
                          style={{
                            display: "flex",
                            justifyContent: "space-between",
                            gap: 10,
                            fontSize: 12.5,
                            borderTop: "1px solid var(--line-soft)",
                            paddingTop: 8,
                          }}
                        >
                          <span style={{ color: "var(--text-2)" }}>
                            {item.description ?? "Sem descrição"}
                            {item.installment_number && item.total_installments
                              ? ` (${item.installment_number}/${item.total_installments})`
                              : ""}
                          </span>
                          <span style={{ fontWeight: 700, whiteSpace: "nowrap" }}>
                            {formatBRL(item.amount)}
                          </span>
                        </div>
                      ))
                    )}
                  </div>
                ) : null}
              </div>
            );
          })}
        </Card>
      )}
    </section>
  );
}
