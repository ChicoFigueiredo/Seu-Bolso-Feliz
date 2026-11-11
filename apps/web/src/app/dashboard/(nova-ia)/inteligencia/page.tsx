import type { Metadata } from "next";
import { carregarPainelInteligencia } from "@/app/actions/painel-inteligencia";
import { PainelInteligenciaView } from "@/components/inteligencia/painel-inteligencia-view";

export const metadata: Metadata = { title: "Inteligência" };

// Fila de confirmação e números de desempenho não podem vir de cache: uma
// contagem velha ("nada esperando confirmação") apresentada como atual é
// pior do que meio segundo de espera. O <SbfTheme> vem do layout do route
// group (nova-ia), e a navegação vem do AppShell — esta página renderiza só
// o conteúdo.
export const dynamic = "force-dynamic";

export default async function InteligenciaPage() {
  const dados = await carregarPainelInteligencia();
  return <PainelInteligenciaView dados={dados} />;
}
