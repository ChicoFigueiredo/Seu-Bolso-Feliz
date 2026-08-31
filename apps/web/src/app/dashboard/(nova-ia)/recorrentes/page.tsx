import type { Metadata } from "next";
import { carregarPainelRecorrentes } from "@/app/actions/painel-recorrentes";
import { PainelRecorrentesView } from "@/components/recorrentes/painel-recorrentes-view";

export const metadata: Metadata = { title: "Recorrentes" };

// Compromisso mensal não pode vir de cache: um total comprometido calculado
// com templates velhos é pior do que uma tela que demora meio segundo a
// mais. O <SbfTheme> vem do layout do route group (nova-ia).
export const dynamic = "force-dynamic";

export default async function RecorrentesPage() {
  const dados = await carregarPainelRecorrentes();
  return <PainelRecorrentesView dados={dados} />;
}
