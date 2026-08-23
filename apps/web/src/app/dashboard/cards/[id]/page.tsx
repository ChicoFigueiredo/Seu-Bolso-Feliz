import { notFound } from "next/navigation";
import { getCard, getProdutosParaCartao } from "@/app/actions/cards";
import { CardForm } from "../card-form";

export const metadata = { title: "Editar cartão" };

export default async function EditCardPage({ params }: { params: Promise<{ id: string }> }) {
  const { id } = await params;
  const [cartao, produtos] = await Promise.all([getCard(id), getProdutosParaCartao()]);

  if (!cartao) notFound();

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <h1 className="text-2xl font-bold tracking-tight">Editar cartão</h1>
      <CardForm produtos={produtos} cartao={cartao} />
    </div>
  );
}
