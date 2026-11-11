import type { Metadata } from "next";
import { carregarPainelHoje } from "@/app/actions/painel-hoje";
import { PainelHojeView } from "@/components/hoje/painel-hoje-view";

export const metadata: Metadata = { title: "Hoje" };

// Posição financeira não pode vir de cache: um saldo velho apresentado como
// atual é pior do que uma tela que demora meio segundo a mais. O <SbfTheme>
// vem do layout do route group (nova-ia).
export const dynamic = "force-dynamic";

export default async function HojePage() {
  const dados = await carregarPainelHoje();
  return <PainelHojeView dados={dados} />;
}
