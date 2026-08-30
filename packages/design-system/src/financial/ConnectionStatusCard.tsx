import type { CSSProperties } from "react";
import { Button } from "../primitives/Button";
import { Card } from "../primitives/Card";
import { Icon } from "../primitives/Icon";

export interface ConnectionStatusCardProps {
  /** Nome da fonte — `Gmail`, `Pasta local`, `MacBook do Chico`. */
  name: string;
  /** Glifo da fonte. Padrão `sync_alt`. */
  icon?: string;
  /** Estado do vinculo. */
  status: "conectado" | "desconectado" | "sincronizando" | "falha";
  /** Última sincronizacao — `hoje, 08:12`. */
  lastSync?: string;
  /** Itens encontrados na última varredura. */
  found?: number;
  /** Falha recente, em linguagem humana. */
  error?: string;
  onSync?: () => void;
  onConfigure?: () => void;
  style?: CSSProperties;
}

const dots: Record<string, string> = {
  conectado: "var(--success)",
  sincronizando: "var(--secondary)",
  desconectado: "var(--text-4)",
  falha: "var(--danger)",
};

/**
 * Uma fonte de documentos na central de conexoes.
 *
 * O worker local aparece aqui como cidadao de primeira classe, ao lado do
 * Gmail — não como detalhe técnico escondido. O ponto colorido nunca anda
 * sozinho: o estado também está escrito, porque cor não pode ser o único
 * indicador.
 */
export function ConnectionStatusCard({
  name,
  icon = "sync_alt",
  status,
  lastSync,
  found,
  error,
  onSync,
  onConfigure,
  style,
}: ConnectionStatusCardProps) {
  return (
    <Card padding={18} style={{ gap: 12, ...style }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          fontFamily: "var(--sbf-font-sans)",
        }}
      >
        <span
          style={{
            width: 36,
            height: 36,
            flex: "0 0 36px",
            borderRadius: "var(--sbf-radius-md)",
            background: "var(--secondary-soft)",
            color: "var(--secondary-ink)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Icon name={icon} size={20} />
        </span>
        <div style={{ flex: "1 1 auto", minWidth: 0 }}>
          <div style={{ fontSize: 14, fontWeight: 700, color: "var(--text)" }}>{name}</div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              fontSize: 12.5,
              color: "var(--text-3)",
              marginTop: 2,
            }}
          >
            <span
              style={{
                width: 8,
                height: 8,
                flex: "0 0 8px",
                borderRadius: "var(--sbf-radius-pill)",
                background: dots[status],
              }}
            />
            {status}
            {lastSync ? <span>· {lastSync}</span> : null}
          </div>
        </div>
      </div>

      {typeof found === "number" ? (
        <div style={{ fontSize: 12.5, color: "var(--text-3)", fontFamily: "var(--sbf-font-sans)" }}>
          {found === 0
            ? "Nada novo na última busca"
            : `${found} ${found === 1 ? "item encontrado" : "itens encontrados"}`}
        </div>
      ) : null}

      {error ? (
        <div
          style={{
            display: "flex",
            gap: 8,
            alignItems: "flex-start",
            background: "var(--warn-bg)",
            border: "1px solid var(--warn-line)",
            borderRadius: "var(--sbf-radius-sm)",
            padding: "10px 12px",
            fontSize: 12.5,
            color: "var(--warn)",
            lineHeight: 1.4,
            fontFamily: "var(--sbf-font-sans)",
          }}
        >
          <Icon name="warning" size={17} />
          <span>{error}</span>
        </div>
      ) : null}

      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        {status === "desconectado" ? (
          <Button variant="soft" size="sm" icon="link" onClick={onSync}>
            Conectar
          </Button>
        ) : (
          <Button
            variant="soft"
            size="sm"
            icon="refresh"
            onClick={onSync}
            disabled={status === "sincronizando"}
          >
            {status === "sincronizando" ? "Sincronizando…" : "Sincronizar agora"}
          </Button>
        )}
        <Button variant="outline" size="sm" onClick={onConfigure}>
          Configurar
        </Button>
      </div>
    </Card>
  );
}
