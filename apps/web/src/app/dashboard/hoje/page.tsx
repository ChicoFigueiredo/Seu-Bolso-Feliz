import { carregarPainelHoje } from "@/app/actions/painel-hoje";
import { PainelHojeView } from "@/components/hoje/painel-hoje-view";

export const metadata = { title: "Hoje — Seu Bolso Feliz" };

// Posição financeira não pode vir de cache: um saldo velho apresentado como
// atual é pior que uma tela que demora meio segundo a mais.
export const dynamic = "force-dynamic";

export default async function HojePage() {
  const dados = await carregarPainelHoje();
  return <PainelHojeView dados={dados} />;
}
