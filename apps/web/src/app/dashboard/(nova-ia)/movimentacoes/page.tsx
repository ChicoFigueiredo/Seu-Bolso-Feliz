import type { Metadata } from "next";
import {
  carregarMovimentacoes,
  carregarOpcoesMovimentacoes,
} from "@/app/actions/painel-movimentacoes";
import { lerFiltrosDaBusca } from "@/components/movimentacoes/filtros";
import { PainelMovimentacoesView } from "@/components/movimentacoes/painel-movimentacoes-view";

export const metadata: Metadata = { title: "Movimentações" };

// Dado financeiro não pode vir de cache: uma lista velha apresentada como
// atual é pior do que meio segundo de espera. O <SbfTheme> vem do layout do
// route group (nova-ia), e a navegação vem do AppShell — esta página
// renderiza só o conteúdo.
export const dynamic = "force-dynamic";

export default async function MovimentacoesPage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  // A tela Hoje linka para cá com `?filtro=vencidas` e afins; `lerFiltrosDaBusca`
  // honra esses links além dos parâmetros próprios da tela.
  const filtros = lerFiltrosDaBusca(await searchParams);

  const [dados, opcoes] = await Promise.all([
    carregarMovimentacoes(filtros),
    carregarOpcoesMovimentacoes(),
  ]);

  return (
    <PainelMovimentacoesView dadosIniciais={dados} filtrosIniciais={filtros} opcoes={opcoes} />
  );
}
