import type { Metadata } from "next";
import { carregarPainelCentros } from "@/app/actions/painel-centros";
import { PainelCentrosView } from "@/components/centros/painel-centros-view";

export const metadata: Metadata = { title: "Projetos & Centros" };

// Saldo de centro e total não alocado mudam a cada rateio salvo — inclusive
// dentro desta mesma tela. Uma versão em cache mostraria "R$ 0,00 sem centro"
// logo depois de o usuário criar um rateio que ainda não existe. O <SbfTheme>
// vem do layout do route group (nova-ia).
export const dynamic = "force-dynamic";

export default async function CentrosPage() {
  const dados = await carregarPainelCentros();
  return <PainelCentrosView dados={dados} />;
}
