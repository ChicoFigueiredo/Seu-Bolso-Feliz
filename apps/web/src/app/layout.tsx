import type { Metadata, Viewport } from "next";
// Ordem de import importa e não é acidente: os dois arquivos definem
// `--primary`, `--muted`, `--secondary` e `--accent` no `:root`, com valores
// diferentes. CSS resolve empate de especificidade pela ordem — o último
// declarado vence no `:root`. Carregando tokens.css primeiro, globals.css
// (shadcn) fica por último e vence o empate para quem não está dentro de um
// <SbfTheme>, ou seja, as telas antigas. Quem está dentro de um <SbfTheme>
// (o shell novo e as rotas em packages/design-system) não depende disso: o
// atributo `data-sbf-theme` redeclara os tokens localmente e vence a herança
// vinda do `:root`, então os dois mundos coexistem sem vazamento em nenhuma
// direção. Não inverta esta ordem sem reler `.design-sync/conventions.md`.
import "@sbf/design-system/tokens.css";
import "../styles/globals.css";
import { Providers } from "@/components/providers";
import { OfflineBanner } from "@/components/shell/offline-banner";
import { RegisterServiceWorker } from "@/components/shell/register-service-worker";

export const metadata: Metadata = {
  title: { default: "Seu Bolso Feliz", template: "%s · Seu Bolso Feliz" },
  description: "Organização financeira pessoal inteligente",
  appleWebApp: {
    capable: true,
    statusBarStyle: "default",
    title: "Bolso Feliz",
  },
  // O Next 15 já gera `mobile-web-app-capable` a partir de `appleWebApp`
  // acima (o nome moderno, que o Chrome/Android também lê). O prefixo
  // `apple-` continua sendo o que iOS mais antigo procura, então mantemos
  // os dois.
  other: {
    "apple-mobile-web-app-capable": "yes",
  },
};

// `theme_color`/`background_color` do manifest.ts usam os mesmos hex — são o
// verde e o bege do tema salvia (`--primary` / `--bg` em
// packages/design-system/src/tokens.css), o tema padrão do produto.
export const viewport: Viewport = {
  width: "device-width",
  initialScale: 1,
  viewportFit: "cover",
  themeColor: "#245C4A",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="pt-BR" suppressHydrationWarning>
      <body className="min-h-screen bg-background text-foreground antialiased">
        <Providers>
          <OfflineBanner />
          {children}
        </Providers>
        <RegisterServiceWorker />
      </body>
    </html>
  );
}
