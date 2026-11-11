import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { carregarDetalheCentro } from "@/app/actions/painel-centros";
import { CentroDetalheView } from "@/components/centros/centro-detalhe-view";

export const metadata: Metadata = { title: "Centro" };

// Mesma razão da lista: os números mudam a cada rateio salvo nesta tela.
export const dynamic = "force-dynamic";

export default async function CentroPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  // `carregarDetalheCentro` devolve `null` tanto para um id que não existe
  // quanto para um centro de outro usuário — os dois merecem exatamente a
  // mesma resposta, senão o 404 vira um oráculo de "este id existe".
  const dados = await carregarDetalheCentro(id);
  if (!dados) notFound();
  return <CentroDetalheView dados={dados} />;
}
