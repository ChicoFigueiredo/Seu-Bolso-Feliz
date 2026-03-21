import type { Metadata } from "next";
import "../styles/globals.css";
import { Providers } from "@/components/providers";

export const metadata: Metadata = {
  title: "Seu Bolso Feliz",
  description: "Organização financeira pessoal inteligente",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="pt-BR" suppressHydrationWarning>
      <body className="min-h-screen bg-background text-foreground antialiased">
        <Providers>{children}</Providers>
      </body>
    </html>
  );
}
