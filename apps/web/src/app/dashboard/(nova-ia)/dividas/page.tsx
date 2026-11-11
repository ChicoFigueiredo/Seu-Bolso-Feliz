import type { Metadata } from "next";
import { carregarPainelDividas } from "@/app/actions/painel-dividas";
import { DividasView } from "@/components/dividas/dividas-view";

export const metadata: Metadata = { title: "Dívidas" };

// Saldo devedor e cronograma não podem vir de cache: um saldo velho
// apresentado como atual é pior que meio segundo a mais de carregamento.
// O <SbfTheme> vem do layout do route group (nova-ia).
export const dynamic = "force-dynamic";

export default async function DividasPage() {
  const dados = await carregarPainelDividas();
  return <DividasView dados={dados} />;
}
