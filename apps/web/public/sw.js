/* global self, caches, fetch, URL, console */
// Service worker do Seu Bolso Feliz — escrito à mão, sem workbox/next-pwa
// (incompatível com o App Router recente; ver CLAUDE.md).
//
// eslint.config.ts não declara globals de browser/worker para nenhum arquivo
// do repo (dá pra ver em qualquer .mjs de script por aí) — o comentário
// `global` acima é a forma da própria ESLint de resolver isso sem tocar num
// config compartilhado com outros 3 agentes trabalhando em paralelo.
//
// Estratégia, deliberada para um app financeiro:
//
//   - Navegações e qualquer outra coisa que carregue dado (RSC payload,
//     /api/* de leitura): network-first. Nunca serve número velho do cache
//     sem avisar — se a rede falhar numa navegação, a resposta é a página
//     estática de offline (offline.html), nunca uma versão em cache da tela
//     anterior fingindo estar atual.
//   - Assets estáticos do Next (/_next/static/*, ícones, manifest): o nome
//     do arquivo já carrega hash do conteúdo, então cache-first é seguro e
//     rápido — o mesmo nome nunca aponta para um conteúdo diferente.
//   - Qualquer requisição que não seja GET (POST de server action, /api/*
//     de escrita): passa direto pra rede, sem tocar em cache. Uma mutação
//     nunca ganha resposta inventada.
//
// Sobe a versão (v1 → v2) quando mudar a lista de assets pré-cacheados ou a
// estratégia — o `activate` apaga caches de versões antigas sozinho.

const VERSION = "v1";
const SHELL_CACHE = `sbf-shell-${VERSION}`;
const STATIC_CACHE = `sbf-static-${VERSION}`;
const OFFLINE_URL = "/offline.html";

const SHELL_ASSETS = [
  OFFLINE_URL,
  "/manifest.webmanifest",
  "/icons/icon-192.png",
  "/icons/icon-512.png",
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(SHELL_CACHE);
      // Um `cache.addAll` só falharia tudo se um asset não crítico (um ícone,
      // digamos) desse 404 — pré-cacheia cada um por conta própria para que a
      // página de offline, a que mais importa, não dependa da sorte das outras.
      await Promise.all(
        SHELL_ASSETS.map((url) =>
          cache.add(url).catch((error) => {
            console.warn("[sw] não consegui pré-cachear", url, error);
          }),
        ),
      );
      await self.skipWaiting();
    })(),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      const keys = await caches.keys();
      await Promise.all(
        keys
          .filter((key) => key !== SHELL_CACHE && key !== STATIC_CACHE)
          .map((key) => caches.delete(key)),
      );
      await self.clients.claim();
    })(),
  );
});

function isStaticAsset(url) {
  return (
    url.pathname.startsWith("/_next/static/") ||
    url.pathname.startsWith("/icons/") ||
    url.pathname === "/manifest.webmanifest" ||
    url.pathname === "/favicon.ico" ||
    url.pathname === "/icon.svg" ||
    url.pathname === "/apple-icon.png"
  );
}

async function networkFirst(request) {
  try {
    return await fetch(request);
  } catch (error) {
    if (request.mode === "navigate") {
      const cache = await caches.open(SHELL_CACHE);
      const offline = await cache.match(OFFLINE_URL);
      if (offline) return offline;
    }
    throw error;
  }
}

async function cacheFirst(request) {
  const cache = await caches.open(STATIC_CACHE);
  const cached = await cache.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (response.ok) cache.put(request, response.clone());
  return response;
}

self.addEventListener("fetch", (event) => {
  const { request } = event;

  // Mutação (POST de server action, /api/* de escrita) nunca passa pelo
  // cache — sempre rede, sempre resposta de verdade.
  if (request.method !== "GET") return;

  const url = new URL(request.url);
  // Fontes do Google e afins: deixa o navegador cuidar, o SW não entra.
  if (url.origin !== self.location.origin) return;

  if (isStaticAsset(url)) {
    event.respondWith(cacheFirst(request));
    return;
  }

  event.respondWith(networkFirst(request));
});
