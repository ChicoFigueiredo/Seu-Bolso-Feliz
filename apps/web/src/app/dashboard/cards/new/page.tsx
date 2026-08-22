import { getProdutosParaCartao } from "@/app/actions/cards";
import { CardForm } from "../card-form";

export const metadata = { title: "Novo cartão" };

export default async function NewCardPage() {
  const produtos = await getProdutosParaCartao();

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      <h1 className="text-2xl font-bold tracking-tight">Novo cartão</h1>
      <CardForm produtos={produtos} />
    </div>
  );
}
