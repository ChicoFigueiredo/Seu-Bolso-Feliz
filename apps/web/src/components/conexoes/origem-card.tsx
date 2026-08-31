"use client";

/**
 * Gmail e pasta local: as duas origens de documento que existem além do Open
 * Finance. Aqui elas são **só leitura**.
 *
 * Nenhum botão de "sincronizar agora": quem varre a caixa de e-mail e a pasta
 * são os workers `gmail-scanner` e `local-scanner`, rodados fora do site
 * (`bun run worker:gmail` / `bun run worker:local`). Um botão que não dispara
 * nada seria pior do que a ausência dele — por isso o card mostra o rastro
 * real (`ingestion_runs`) e, quando não há rastro nenhum, diz o que falta
 * configurar em vez de fingir um estado.
 */

import { Card, Icon, Pill } from "@sbf/design-system";
import type { OrigemDocumentos } from "@/app/actions/painel-conexoes";
import { momentoRelativo, plural } from "./rotulos";

export interface OrigemCardProps {
  origem: OrigemDocumentos;
}

interface Descricao {
  nome: string;
  icone: string;
  /** Passos exatos para ligar a origem, quando ela nunca rodou. */
  comoLigar: string;
}

const DESCRICAO: Record<OrigemDocumentos["id"], Descricao> = {
  gmail: {
    nome: "Gmail",
    icone: "mail",
    comoLigar:
      "Nunca varreu a sua caixa. Para ligar: gere o token com `bun run get:gmail-token`, preencha GOOGLE_MAIL_CLIENT_ID, GOOGLE_MAIL_CLIENT_SECRET e GMAIL_REFRESH_TOKEN no .env, e rode `bun run worker:gmail`.",
  },
  local_file: {
    nome: "Pasta local",
    icone: "folder_open",
    comoLigar:
      "Nenhuma pasta varrida ainda. Para ligar: aponte WATCH_DIR para a pasta dos comprovantes e rode `bun run worker:local`.",
  },
};

function aparencia(origem: OrigemDocumentos): {
  rotulo: string;
  tom: "primary" | "secondary" | "danger" | "neutral";
  icone: string;
} {
  if (!origem.jaRodou)
    return { rotulo: "Não configurada", tom: "neutral", icone: "power_settings_new" };
  switch (origem.status) {
    case "running":
      return { rotulo: "Varrendo agora", tom: "secondary", icone: "hourglass_top" };
    case "failed":
      return { rotulo: "Falhou", tom: "danger", icone: "error" };
    case "cancelled":
      return { rotulo: "Interrompida", tom: "neutral", icone: "pause_circle" };
    default:
      return { rotulo: "Em dia", tom: "primary", icone: "check_circle" };
  }
}

export function OrigemCard({ origem }: OrigemCardProps) {
  const descricao = DESCRICAO[origem.id];
  const estado = aparencia(origem);
  const quando = momentoRelativo(origem.ultimaVarredura);

  return (
    <Card padding={18} style={{ gap: 10 }}>
      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <span
          style={{
            width: 36,
            height: 36,
            flex: "0 0 36px",
            borderRadius: "var(--sbf-radius-md)",
            background: "var(--muted)",
            color: "var(--text-2)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Icon name={descricao.icone} size={20} />
        </span>
        <div style={{ flex: "1 1 auto", minWidth: 0 }}>
          <div style={{ fontSize: 14, fontWeight: 700, color: "var(--text)" }}>
            {descricao.nome}
          </div>
          <div style={{ fontSize: 12.5, color: "var(--text-3)", marginTop: 2 }}>
            {quando ? `Última varredura ${quando}` : "Sem varredura registrada"}
          </div>
        </div>
        <Pill tone={estado.tom} icon={estado.icone} size="xs">
          {estado.rotulo}
        </Pill>
      </div>

      {origem.jaRodou ? (
        <div style={{ fontSize: 12.5, color: "var(--text-3)", lineHeight: 1.45 }}>
          {origem.documentos === 0
            ? "A última varredura não encontrou documento novo."
            : `${plural(origem.documentos, "documento", "documentos")} na última varredura.`}
          {origem.detalhe ? ` · ${origem.detalhe}` : ""}
        </div>
      ) : (
        <div
          style={{
            background: "var(--muted)",
            borderRadius: "var(--sbf-radius-sm)",
            padding: "10px 12px",
            fontSize: 12.5,
            color: "var(--text-3)",
            lineHeight: 1.5,
          }}
        >
          {descricao.comoLigar}
        </div>
      )}
    </Card>
  );
}
