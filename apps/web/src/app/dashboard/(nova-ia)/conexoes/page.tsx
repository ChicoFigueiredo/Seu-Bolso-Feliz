import type { Metadata } from "next";
import { carregarPainelConexoes } from "@/app/actions/painel-conexoes";
import { PainelConexoesView } from "@/components/conexoes/painel-conexoes-view";

export const metadata: Metadata = { title: "Conexões" };

// Estado de conexão não sobrevive a cache: uma tela dizendo "conectada" sobre
// um vínculo que caiu há uma hora faz o usuário esperar por dado que não vem.
// O <SbfTheme> vem do layout do route group (nova-ia).
export const dynamic = "force-dynamic";

export default async function ConexoesPage() {
  const dados = await carregarPainelConexoes();
  return <PainelConexoesView dados={dados} />;
}
