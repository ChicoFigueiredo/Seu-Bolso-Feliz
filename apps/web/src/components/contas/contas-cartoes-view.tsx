"use client";

/**
 * A tela Contas & Cartões. Responde três perguntas: quanto tenho em cada
 * conta (e no total), qual a situação de cada cartão (fatura, limite,
 * vencimento), e o que já está lançado em cada fatura.
 *
 * Segue a arquitetura de `components/hoje/painel-hoje-view.tsx`: a página
 * (Server Component) busca os dados prontos via `carregarPainelContas` e este
 * componente cuida só de estado de interface (qual folha está aberta, qual
 * cartão está selecionado). CRUD de conta/cartão/instituição abre em `Sheet`
 * — nunca em página nova — e chama direto as server actions que já existem
 * (`@/app/actions/cards`, `financial-products`, `institutions`); nenhuma
 * delas é reimplementada aqui.
 */
import { useState } from "react";
import { useRouter } from "next/navigation";
import {
  AccountBalanceCard,
  Button,
  Card,
  CreditCardSummary,
  EmptyState,
  Icon,
  MoneyMetricCard,
  Pill,
  SectionTitle,
  formatBRL,
} from "@sbf/design-system";
import type {
  CartaoDetalhe,
  ContaDetalhe,
  InstituicaoResumo,
  PainelContas,
} from "@/app/actions/painel-contas";
import { CartaoSheet } from "./cartao-sheet";
import { ContaSheet } from "./conta-sheet";
import { FaturasSection } from "./faturas-section";
import { InstituicaoSheet } from "./instituicao-sheet";
import { hojeIso as hojeIsoSP, somaDias } from "@/lib/datas";

type EstadoSheet =
  | { tipo: "fechado" }
  | { tipo: "instituicao"; instituicao?: InstituicaoResumo }
  | { tipo: "conta"; conta?: ContaDetalhe }
  | { tipo: "cartao"; cartao?: CartaoDetalhe };

const ROTULO_TIPO_CONTA: Record<string, string> = {
  checking_account: "Conta corrente",
  savings_account: "Poupança",
};

const ROTULO_TIPO_INSTITUICAO: Record<string, string> = {
  bank: "Banco",
  fintech: "Fintech",
  broker: "Corretora",
  other: "Outra",
};

function dataCurta(iso: string): string {
  const d = new Date(`${iso}T12:00:00`);
  return `${String(d.getDate()).padStart(2, "0")}/${String(d.getMonth() + 1).padStart(2, "0")}`;
}

function em7Dias(hojeIso: string): string {
  return somaDias(hojeIso, 7);
}

export function ContasCartoesView({ dados }: { dados: PainelContas }) {
  const router = useRouter();
  const [sheet, setSheet] = useState<EstadoSheet>({ tipo: "fechado" });
  const [cartaoSelecionadoId, setCartaoSelecionadoId] = useState<string | null>(
    dados.cartoes[0]?.id ?? null,
  );
  const [instituicoesAbertas, setInstituicoesAbertas] = useState(false);

  const hojeIso = hojeIsoSP();
  const avisoVencimento = em7Dias(hojeIso);

  function fechar() {
    setSheet({ tipo: "fechado" });
  }
  function aoSalvar() {
    fechar();
    router.refresh();
  }

  const semNada = dados.contas.length === 0 && dados.cartoes.length === 0;
  const contasAtivas = dados.contas.filter((c) => c.ativa);
  const cartaoSelecionado = dados.cartoes.find((c) => c.id === cartaoSelecionadoId) ?? null;

  return (
    <>
      {semNada ? (
        <EmptyState
          icon="account_balance"
          title="Nenhuma conta ou cartão ainda"
          description="Conecte um banco pelo Open Finance ou cadastre manualmente para começar a acompanhar seus saldos e faturas."
        >
          <div style={{ display: "flex", gap: 10, flexWrap: "wrap", justifyContent: "center" }}>
            <Button
              variant="primary"
              icon="link"
              onClick={() => router.push("/dashboard/conexoes")}
            >
              Conectar um banco
            </Button>
            <Button variant="outline" icon="add" onClick={() => setSheet({ tipo: "conta" })}>
              Cadastrar manualmente
            </Button>
          </div>
        </EmptyState>
      ) : (
        <div style={{ display: "grid", gap: 24, paddingBottom: 24 }}>
          <MoneyMetricCard
            variant="hero"
            label="Total em contas"
            value={formatBRL(dados.totalContas)}
            caption={`Em ${contasAtivas.length} ${contasAtivas.length === 1 ? "conta" : "contas"} ativa${contasAtivas.length === 1 ? "" : "s"}`}
          />

          {/* ── Contas ─────────────────────────────────────────────────── */}
          <section style={{ display: "grid", gap: 12 }}>
            <SectionTitle actionLabel="+ Nova conta" onAction={() => setSheet({ tipo: "conta" })}>
              Contas
            </SectionTitle>
            {dados.contas.length === 0 ? (
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
                  <Icon name="account_balance" size={18} />
                  <span>Nenhuma conta cadastrada ainda.</span>
                </div>
              </Card>
            ) : (
              <div
                style={{
                  display: "grid",
                  gap: 10,
                  gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
                }}
              >
                {dados.contas.map((c) => (
                  <AccountBalanceCard
                    key={c.id}
                    name={c.nome}
                    balance={formatBRL(c.saldo)}
                    caption={[
                      c.instituicao,
                      ROTULO_TIPO_CONTA[c.tipo] ?? c.tipo,
                      c.ativa ? null : "Inativa",
                    ]
                      .filter(Boolean)
                      .join(" · ")}
                    onClick={() => setSheet({ tipo: "conta", conta: c })}
                    style={{ opacity: c.ativa ? 1 : 0.55 }}
                  />
                ))}
              </div>
            )}
          </section>

          {/* ── Cartões ────────────────────────────────────────────────── */}
          <section style={{ display: "grid", gap: 12 }}>
            <SectionTitle actionLabel="+ Novo cartão" onAction={() => setSheet({ tipo: "cartao" })}>
              Cartões
            </SectionTitle>
            {dados.cartoes.length === 0 ? (
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
                  <Icon name="credit_card" size={18} />
                  <span>Nenhum cartão cadastrado ainda.</span>
                </div>
              </Card>
            ) : (
              <div
                style={{
                  display: "grid",
                  gap: 10,
                  gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))",
                }}
              >
                {dados.cartoes.map((c) => {
                  const selecionado = c.id === cartaoSelecionadoId;
                  const dueLabel = c.cicloAtual
                    ? `Vence ${dataCurta(c.cicloAtual.dueDate)}`
                    : c.dueDay
                      ? `Vence todo dia ${c.dueDay}`
                      : undefined;
                  const dueTone = c.cicloAtual
                    ? c.cicloAtual.dueDate < hojeIso
                      ? "danger"
                      : c.cicloAtual.dueDate <= avisoVencimento
                        ? "warn"
                        : "neutral"
                    : "neutral";
                  // Fração de 0 a 1 — CreditCardSummary repassa direto para
                  // ProgressBar, que espera fração, não porcentagem (achado ao
                  // ler painel-hoje-view.tsx, que multiplica por 100 antes de
                  // passar: com fatura > 0 a barra satura em 100% sempre).
                  const usado =
                    c.limiteTotal && c.limiteTotal > 0
                      ? Math.min(1, Math.max(0, c.faturaAtual / c.limiteTotal))
                      : 0;

                  return (
                    <button
                      key={c.id}
                      type="button"
                      aria-pressed={selecionado}
                      onClick={() => setCartaoSelecionadoId(c.id)}
                      style={{
                        all: "unset",
                        display: "block",
                        width: "100%",
                        cursor: "pointer",
                        borderRadius: "var(--sbf-radius-xl)",
                        boxShadow: selecionado ? "0 0 0 2px var(--primary)" : "none",
                        opacity: c.ativo ? 1 : 0.55,
                        boxSizing: "border-box",
                      }}
                    >
                      <CreditCardSummary
                        name={[c.nome, c.last4 ? `·${c.last4}` : null, c.ativo ? null : "(inativo)"]
                          .filter(Boolean)
                          .join(" ")}
                        invoice={formatBRL(c.faturaAtual)}
                        available={c.limiteLivre == null ? "—" : formatBRL(c.limiteLivre)}
                        limit={c.limiteTotal == null ? "—" : formatBRL(c.limiteTotal)}
                        used={usado}
                        dueLabel={dueLabel}
                        dueTone={dueTone}
                      />
                    </button>
                  );
                })}
              </div>
            )}
            {cartaoSelecionado ? (
              <div style={{ display: "flex", justifyContent: "flex-end" }}>
                <Button
                  variant="quiet"
                  size="sm"
                  icon="edit"
                  onClick={() => setSheet({ tipo: "cartao", cartao: cartaoSelecionado })}
                >
                  {`Editar ${cartaoSelecionado.nome}`}
                </Button>
              </div>
            ) : null}
          </section>

          {/* ── Faturas do cartão selecionado ──────────────────────────── */}
          {cartaoSelecionado ? <FaturasSection cartao={cartaoSelecionado} /> : null}

          {/* ── Instituições ───────────────────────────────────────────── */}
          <section>
            <Card padding={18} style={{ gap: 0 }}>
              <button
                type="button"
                onClick={() => setInstituicoesAbertas((v) => !v)}
                aria-expanded={instituicoesAbertas}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 10,
                  width: "100%",
                  minHeight: 44,
                  border: 0,
                  background: "transparent",
                  cursor: "pointer",
                  padding: 0,
                  textAlign: "left",
                  fontFamily: "var(--sbf-font-sans)",
                  color: "var(--text)",
                }}
              >
                <Icon name="domain" size={21} style={{ color: "var(--text-3)" }} />
                <span style={{ fontSize: 15.5, fontWeight: 700, flex: "1 1 auto" }}>
                  Instituições{" "}
                  <span style={{ color: "var(--text-3)", fontWeight: 600 }}>
                    ({dados.instituicoes.length})
                  </span>
                </span>
                <Icon
                  name={instituicoesAbertas ? "expand_less" : "expand_more"}
                  size={22}
                  style={{ color: "var(--text-3)" }}
                />
              </button>

              {instituicoesAbertas ? (
                <div style={{ display: "flex", flexDirection: "column", marginTop: 12 }}>
                  {dados.instituicoes.map((inst) => (
                    <button
                      key={inst.id}
                      type="button"
                      onClick={() => setSheet({ tipo: "instituicao", instituicao: inst })}
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 11,
                        minHeight: 44,
                        padding: "11px 0",
                        border: 0,
                        borderTop: "1px solid var(--line-soft)",
                        background: "transparent",
                        cursor: "pointer",
                        textAlign: "left",
                        width: "100%",
                        fontFamily: "var(--sbf-font-sans)",
                        color: "var(--text)",
                        opacity: inst.ativa ? 1 : 0.55,
                      }}
                    >
                      <span
                        style={{ flex: "1 1 auto", minWidth: 0, fontSize: 13.5, fontWeight: 600 }}
                      >
                        {inst.nome}
                        {inst.ativa ? "" : " (inativa)"}
                      </span>
                      <Pill tone="neutral" size="xs">
                        {ROTULO_TIPO_INSTITUICAO[inst.tipo] ?? inst.tipo}
                      </Pill>
                      <Icon name="chevron_right" size={19} style={{ color: "var(--text-5)" }} />
                    </button>
                  ))}
                  <Button
                    variant="outline"
                    size="sm"
                    icon="add"
                    style={{ marginTop: 12, alignSelf: "flex-start" }}
                    onClick={() => setSheet({ tipo: "instituicao" })}
                  >
                    Nova instituição
                  </Button>
                </div>
              ) : null}
            </Card>
          </section>
        </div>
      )}

      {sheet.tipo !== "fechado" ? (
        <div style={{ position: "fixed", inset: 0, zIndex: 50 }}>
          {sheet.tipo === "instituicao" ? (
            <InstituicaoSheet instituicao={sheet.instituicao} onClose={fechar} onSaved={aoSalvar} />
          ) : null}
          {sheet.tipo === "conta" ? (
            <ContaSheet
              conta={sheet.conta}
              instituicoes={dados.instituicoes}
              onClose={fechar}
              onSaved={aoSalvar}
              onPrecisaInstituicao={() => setSheet({ tipo: "instituicao" })}
            />
          ) : null}
          {sheet.tipo === "cartao" ? (
            <CartaoSheet
              cartao={sheet.cartao}
              instituicoes={dados.instituicoes}
              onClose={fechar}
              onSaved={aoSalvar}
              onPrecisaInstituicao={() => setSheet({ tipo: "instituicao" })}
            />
          ) : null}
        </div>
      ) : null}
    </>
  );
}
