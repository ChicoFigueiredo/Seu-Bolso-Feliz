import type { Metadata } from "next";
import { carregarPainelContas } from "@/app/actions/painel-contas";
import { ContasCartoesView } from "@/components/contas/contas-cartoes-view";

export const metadata: Metadata = { title: "Contas & Cartões" };

// Saldo de conta e fatura de cartão não podem vir de cache — um número velho
// aqui é o tipo de erro que corrói confiança num app financeiro. O <SbfTheme>
// vem do layout do route group (nova-ia).
export const dynamic = "force-dynamic";

export default async function ContasPage() {
  const dados = await carregarPainelContas();
  return <ContasCartoesView dados={dados} />;
}
