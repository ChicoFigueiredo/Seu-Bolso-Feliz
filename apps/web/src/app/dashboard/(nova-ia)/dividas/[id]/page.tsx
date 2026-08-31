import type { Metadata } from "next";
import { redirect } from "next/navigation";
import { carregarDetalheDivida } from "@/app/actions/painel-dividas";
import { DividaDetalheView } from "@/components/dividas/divida-detalhe-view";

export const metadata: Metadata = { title: "Dívida" };

// Mesmo motivo da lista: saldo devedor e cronograma não podem vir de cache.
export const dynamic = "force-dynamic";

export default async function DividaDetalhePage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const divida = await carregarDetalheDivida(id);

  // Mesmo padrão de apps/web/src/app/dashboard/liabilities/[id]/page.tsx:
  // dívida inexistente (ou de outro usuário — `carregarDetalheDivida` já
  // filtra por ownerId) volta pra lista em vez de mostrar um 404 seco.
  if (!divida) redirect("/dashboard/dividas");

  return <DividaDetalheView divida={divida} />;
}
