"use client";

import { useEffect, useState } from "react";
import { Icon } from "@sbf/design-system";

/**
 * Aviso explícito de offline, visível em qualquer tela do app.
 *
 * Isto é um app financeiro: nenhum número em cache pode parecer atual sem
 * avisar que a rede caiu. Reage aos eventos `online`/`offline` do navegador —
 * não ao service worker — porque uma navegação já carregada no cliente não
 * necessariamente passa pelo `fetch` handler de novo.
 */
export function OfflineBanner() {
  const [online, setOnline] = useState(true);

  useEffect(() => {
    setOnline(navigator.onLine);
    const goOnline = () => setOnline(true);
    const goOffline = () => setOnline(false);
    window.addEventListener("online", goOnline);
    window.addEventListener("offline", goOffline);
    return () => {
      window.removeEventListener("online", goOnline);
      window.removeEventListener("offline", goOffline);
    };
  }, []);

  if (online) return null;

  return (
    <div
      role="status"
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 8,
        padding: "8px 16px",
        background: "var(--warn-bg)",
        borderBottom: "1px solid var(--warn-line)",
        color: "var(--warn)",
        fontFamily: "var(--sbf-font-sans)",
        fontSize: 12.5,
        fontWeight: 700,
        textAlign: "center",
      }}
    >
      <Icon name="cloud_off" size={16} />
      Você está offline — os números na tela podem não ser os mais recentes.
    </div>
  );
}
