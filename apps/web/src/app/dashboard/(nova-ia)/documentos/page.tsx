import type { Metadata } from "next";
import { carregarCaixaDeDocumentos } from "@/app/actions/painel-documentos";
import { CaixaDeDocumentosView } from "@/components/documentos/caixa-de-documentos-view";

export const metadata: Metadata = { title: "Documentos" };

// A caixa de entrada não pode vir de cache: um documento que já foi aprovado
// aparecendo como "precisa revisar" faz a pessoa decidir duas vezes a mesma
// coisa — e um que acabou de chegar não aparecer faz parecer que o envio se
// perdeu. O <SbfTheme> vem do layout do route group (nova-ia).
export const dynamic = "force-dynamic";

export default async function DocumentosPage() {
  const inicial = await carregarCaixaDeDocumentos();
  return <CaixaDeDocumentosView inicial={inicial} />;
}
