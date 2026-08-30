"use client";

import type { CSSProperties } from "react";
import { Card, Icon, Sheet } from "@sbf/design-system";
import { LOGOUT_ENTRY, MORE_SHEET_PRIMARY, SECONDARY_NAV, type NavEntry } from "./nav-config";
import styles from "./shell.module.css";

export interface MoreSheetProps {
  open: boolean;
  onClose: () => void;
  /** Navega e fecha a folha. */
  onNavigate: (href: string) => void;
  onSignOut: () => void;
}

const rowStyle: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 12,
  width: "100%",
  border: 0,
  borderTop: "1px solid var(--line)",
  background: "var(--surface)",
  padding: "14px 16px",
  fontFamily: "var(--sbf-font-sans)",
  fontSize: 14,
  fontWeight: 600,
  color: "var(--text-2)",
  textAlign: "left",
  cursor: "pointer",
};

function Row({ entry, onClick }: { entry: NavEntry; onClick: () => void }) {
  return (
    <button type="button" onClick={onClick} style={rowStyle}>
      <Icon name={entry.icon} size={20} style={{ color: "var(--text-3)" }} />
      <span style={{ flex: "1 1 auto" }}>{entry.label}</span>
      <Icon name="chevron_right" size={18} style={{ color: "var(--text-4)" }} />
    </button>
  );
}

/**
 * Folha "Mais" do mobile — o resto da navegação principal que não coube nas
 * quatro abas, mais os itens de segunda ordem do rodapé do desktop.
 *
 * `BottomNav` só tem espaço para quatro abas confortáveis; esta folha existe
 * para que nada da arquitetura de informação fique inalcançável no celular.
 */
export function MoreSheet({ open, onClose, onNavigate, onSignOut }: MoreSheetProps) {
  if (!open) return null;

  const go = (href: string) => {
    onNavigate(href);
    onClose();
  };

  return (
    <div className={styles.moreSheetOverlay}>
      {/* `height: 100%` (não só o minHeight que o Sheet já tem) é o que faz o
          véu escuro cobrir a tela inteira em vez de só a altura do cartão; o
          overflow é o que garante que os nove itens caibam mesmo numa tela
          baixa, com scroll em vez de conteúdo inalcançável. */}
      <Sheet title="Mais" onClose={onClose} style={{ height: "100%", overflowY: "auto" }}>
        <Card variant="flush">
          {MORE_SHEET_PRIMARY.map((entry) => (
            <Row key={entry.id} entry={entry} onClick={() => go(entry.href)} />
          ))}
        </Card>
        <Card variant="flush">
          {SECONDARY_NAV.map((entry) => (
            <Row key={entry.id} entry={entry} onClick={() => go(entry.href)} />
          ))}
        </Card>
        <Card variant="flush">
          <Row
            entry={LOGOUT_ENTRY}
            onClick={() => {
              onSignOut();
              onClose();
            }}
          />
        </Card>
      </Sheet>
    </div>
  );
}
