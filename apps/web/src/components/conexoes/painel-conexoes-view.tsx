"use client";

/**
 * A tela Conexões — o caminho de entrada do produto.
 *
 * Sem esta tela o app não recebe dado nenhum, então a ordem das seções é a
 * ordem do que impede o dinheiro de entrar: primeiro o que está bloqueando
 * (credencial faltando, conta sem vínculo), depois os bancos, e só no fim as
 * origens de documento, que são complemento e não caminho principal.
 *
 * Duas regras que esta tela leva a sério:
 *
 *   - **Nenhum botão que não faz nada.** A sincronização é rodada pelo worker
 *     `pluggy-sync`, fora do site; não existe "sincronizar agora" aqui porque
 *     não existe o que ele chamaria. O que existe é a data da última
 *     sincronização, que é fato.
 *
 *   - **Erro não é aviso passageiro.** Confirmação sai em `Toast` e some;
 *     falha fica numa faixa fixa até o usuário fechá-la, porque falha exige
 *     decisão (é a própria regra escrita no Toast do design system).
 */

import { useCallback, useEffect, useState, type ReactNode } from "react";
import { useRouter } from "next/navigation";
import { Button, Card, EmptyState, Icon, SectionTitle, Toast } from "@sbf/design-system";
import type { Conexao, PainelConexoes } from "@/app/actions/painel-conexoes";
import { ConectarBanco } from "./conectar-banco";
import { ConexaoCard } from "./conexao-card";
import { DesconectarSheet } from "./desconectar-sheet";
import { OrigemCard } from "./origem-card";
import { plural } from "./rotulos";

/** Comando exato do backfill — ver workers/pluggy-sync/src/index.ts. */
const COMANDO_BACKFILL = "bun run workers/pluggy-sync/src/index.ts --backfill";

const estiloCodigo = {
  fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
  fontSize: 12,
  background: "var(--surface)",
  border: "1px solid var(--line)",
  borderRadius: "var(--sbf-radius-sm)",
  padding: "8px 10px",
  display: "block",
  overflowX: "auto" as const,
  whiteSpace: "pre" as const,
};

function Faixa({
  tom,
  icone,
  children,
  aoFechar,
}: {
  tom: "warn" | "danger";
  icone: string;
  children: ReactNode;
  aoFechar?: () => void;
}) {
  const cor = tom === "danger" ? "var(--danger)" : "var(--warn)";
  return (
    <div
      style={{
        display: "flex",
        gap: 10,
        alignItems: "flex-start",
        background: "var(--warn-bg)",
        border: "1px solid var(--warn-line)",
        borderRadius: "var(--sbf-radius-md)",
        padding: "12px 14px",
        fontFamily: "var(--sbf-font-sans)",
        fontSize: 13,
        color: cor,
        lineHeight: 1.5,
      }}
    >
      <Icon name={icone} size={19} style={{ flex: "0 0 19px", marginTop: 1 }} />
      <div style={{ flex: "1 1 auto", minWidth: 0 }}>{children}</div>
      {aoFechar ? (
        <button
          type="button"
          onClick={aoFechar}
          aria-label="Fechar aviso"
          style={{
            border: 0,
            background: "transparent",
            color: "inherit",
            cursor: "pointer",
            padding: 0,
            width: 24,
            height: 24,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Icon name="close" size={18} />
        </button>
      ) : null}
    </div>
  );
}

export function PainelConexoesView({ dados }: { dados: PainelConexoes }) {
  const router = useRouter();
  const [aviso, setAviso] = useState<{ texto: string; icone: string } | null>(null);
  const [erro, setErro] = useState<string | null>(null);
  const [paraDesconectar, setParaDesconectar] = useState<Conexao | null>(null);

  const avisar = useCallback((texto: string, icone = "check_circle") => {
    setErro(null);
    setAviso({ texto, icone });
  }, []);

  const falhar = useCallback((mensagem: string) => {
    setAviso(null);
    setErro(mensagem);
  }, []);

  const recarregar = useCallback(() => router.refresh(), [router]);

  useEffect(() => {
    if (!aviso) return;
    const id = setTimeout(() => setAviso(null), 4500);
    return () => clearTimeout(id);
  }, [aviso]);

  const temConexao = dados.conexoes.length > 0;

  return (
    <div style={{ display: "grid", gap: 24, paddingBottom: 24 }}>
      {erro ? (
        <Faixa tom="danger" icone="error" aoFechar={() => setErro(null)}>
          {erro}
        </Faixa>
      ) : null}

      {!dados.pluggyConfigurado ? (
        <Faixa tom="warn" icone="key_off">
          <strong>Falta a credencial da Pluggy neste servidor.</strong> Conectar banco só funciona
          depois de criar um aplicativo em Meu Pluggy e colocar o Client ID e o Client Secret em{" "}
          <code>PLUGGY_CLIENT_ID</code> e <code>PLUGGY_CLIENT_SECRET</code> no <code>.env</code> do
          site — e reiniciar o servidor. Até lá o botão fica desligado de propósito: sem isso, o
          clique só produziria um erro.
        </Faixa>
      ) : null}

      {/* ── Bancos ───────────────────────────────────────────────────── */}
      <section style={{ display: "grid", gap: 12 }}>
        <SectionTitle>Bancos</SectionTitle>

        {dados.contasAguardandoVinculo > 0 ? (
          <Faixa tom="warn" icone="pending">
            {plural(
              dados.contasAguardandoVinculo,
              "conta do banco espera",
              "contas do banco esperam",
            )}{" "}
            você dizer a que conta daqui ela corresponde. Enquanto isso, as movimentações delas
            chegam sem saber onde entrar.
          </Faixa>
        ) : null}

        {temConexao ? (
          <>
            <div style={{ maxWidth: 360 }}>
              <ConectarBanco
                rotulo="Conectar outro banco"
                bloco
                desabilitado={!dados.pluggyConfigurado}
                aoConcluir={(mensagem) => {
                  avisar(mensagem);
                  recarregar();
                }}
                aoFalhar={falhar}
              />
            </div>

            {dados.conexoes.map((conexao) => (
              <ConexaoCard
                key={conexao.id}
                conexao={conexao}
                produtos={dados.produtos}
                pluggyConfigurado={dados.pluggyConfigurado}
                aoAvisar={avisar}
                aoFalhar={falhar}
                aoMudar={recarregar}
                aoPedirDesconexao={setParaDesconectar}
              />
            ))}
          </>
        ) : (
          <Card>
            <EmptyState
              icon="account_balance"
              title="Nenhum banco conectado ainda"
              description="Conectar pelo Open Finance é o caminho mais curto para o app ter o que mostrar: as movimentações passam a chegar sozinhas, e você só revisa."
            >
              <ConectarBanco
                rotulo="Conectar banco"
                desabilitado={!dados.pluggyConfigurado}
                aoConcluir={(mensagem) => {
                  avisar(mensagem);
                  recarregar();
                }}
                aoFalhar={falhar}
              />
            </EmptyState>
          </Card>
        )}

        {temConexao ? (
          <Card variant="quiet" style={{ gap: 8 }}>
            <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
              <Icon name="history" size={18} style={{ color: "var(--text-3)" }} />
              <span style={{ fontSize: 13, fontWeight: 700, color: "var(--text-2)" }}>
                Trazer 12 meses de histórico
              </span>
            </div>
            <span style={{ fontSize: 12.5, color: "var(--text-3)", lineHeight: 1.5 }}>
              Esta tela conecta e vincula as contas. O histórico longo vem de uma rodada única do
              worker de sincronização — janela de 365 dias, retomável se cair no meio. Vincule as
              contas primeiro: a movimentação de uma conta sem vínculo chega sem saber onde entrar.
            </span>
            <code style={estiloCodigo}>{COMANDO_BACKFILL}</code>
          </Card>
        ) : null}

        {dados.movimentacoesParaRevisar > 0 ? (
          <Card variant="quiet" style={{ gap: 10 }}>
            <span style={{ fontSize: 13, color: "var(--text-2)", lineHeight: 1.45 }}>
              {plural(
                dados.movimentacoesParaRevisar,
                "movimentação do Open Finance espera",
                "movimentações do Open Finance esperam",
              )}{" "}
              sua revisão.
            </span>
            <div>
              <Button
                variant="soft"
                size="lg"
                icon="rule"
                onClick={() => router.push("/dashboard/ingestion/review")}
                style={{ minHeight: 44 }}
              >
                Revisar agora
              </Button>
            </div>
          </Card>
        ) : null}
      </section>

      {/* ── Outras origens ───────────────────────────────────────────── */}
      <section style={{ display: "grid", gap: 12 }}>
        <SectionTitle>Outras origens de documento</SectionTitle>
        <span style={{ fontSize: 12.5, color: "var(--text-3)", lineHeight: 1.5 }}>
          Estas duas são varridas pelos workers, fora do site — por isso aqui só aparece o estado
          delas, sem botão de disparar.
        </span>
        <div
          style={{
            display: "grid",
            gap: 12,
            gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))",
          }}
        >
          {dados.origens.map((origem) => (
            <OrigemCard key={origem.id} origem={origem} />
          ))}
        </div>
      </section>

      {paraDesconectar ? (
        <DesconectarSheet
          conexao={paraDesconectar}
          aoFechar={() => setParaDesconectar(null)}
          aoConcluir={(mensagem) => {
            avisar(mensagem, "link_off");
            recarregar();
          }}
          aoFalhar={falhar}
        />
      ) : null}

      {aviso ? (
        <div
          style={{
            position: "fixed",
            left: 0,
            right: 0,
            bottom: "calc(96px + env(safe-area-inset-bottom))",
            display: "flex",
            justifyContent: "center",
            padding: "0 16px",
            pointerEvents: "none",
            zIndex: 60,
          }}
        >
          <Toast icon={aviso.icone}>{aviso.texto}</Toast>
        </div>
      ) : null}
    </div>
  );
}
