import type { MetadataRoute } from "next";

/**
 * Manifesto do PWA. Next serve isto em `/manifest.webmanifest` e já liga o
 * `<link rel="manifest">` sozinho — não precisa declarar em `metadata`.
 *
 * `theme_color`/`background_color` vêm do tema salvia (o padrão do produto):
 * `--primary` (#245C4A) e `--bg` (#F6F7F2) em
 * packages/design-system/src/tokens.css. Como são hex fixos copiados de um
 * arquivo que este pacote não importa, se o salvia mudar de tom este arquivo
 * precisa ser atualizado à mão.
 */
export default function manifest(): MetadataRoute.Manifest {
  return {
    name: "Seu Bolso Feliz",
    short_name: "Bolso Feliz",
    description: "Central pessoal de operação financeira: dinheiro, compromissos e documentos.",
    // Vai direto para dentro do app: quem abre o PWA instalado já está logado
    // ou cai no /login pelo próprio requireUser() do layout do dashboard.
    start_url: "/dashboard/hoje",
    scope: "/",
    display: "standalone",
    orientation: "portrait",
    background_color: "#F6F7F2",
    theme_color: "#245C4A",
    lang: "pt-BR",
    icons: [
      { src: "/icons/icon-192.png", sizes: "192x192", type: "image/png", purpose: "any" },
      { src: "/icons/icon-512.png", sizes: "512x512", type: "image/png", purpose: "any" },
      {
        src: "/icons/icon-maskable-192.png",
        sizes: "192x192",
        type: "image/png",
        purpose: "maskable",
      },
      {
        src: "/icons/icon-maskable-512.png",
        sizes: "512x512",
        type: "image/png",
        purpose: "maskable",
      },
    ],
  };
}
