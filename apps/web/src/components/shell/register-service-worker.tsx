"use client";

import { useEffect } from "react";

/**
 * Registra `/sw.js` assim que a página termina de carregar.
 *
 * Fica no layout raiz, não no shell do dashboard: o app inteiro (login
 * incluído) se beneficia do cache do shell (assets estáticos) e, uma vez
 * offline, da página de fallback explícita — ver `public/sw.js` e
 * `public/offline.html`.
 */
export function RegisterServiceWorker() {
  useEffect(() => {
    if (typeof window === "undefined" || !("serviceWorker" in navigator)) return;
    // Só em produção: em dev, um SW cacheando bundle é o tipo de coisa que
    // faz uma mudança sumir da tela e ninguém entender por quê.
    if (process.env.NODE_ENV !== "production") return;

    const registrar = () => {
      navigator.serviceWorker.register("/sw.js").catch((error: unknown) => {
        console.error("[pwa] falha ao registrar o service worker", error);
      });
    };

    if (document.readyState === "complete") {
      registrar();
      return;
    }
    window.addEventListener("load", registrar);
    return () => window.removeEventListener("load", registrar);
  }, []);

  return null;
}
