"use server";

/**
 * Busca os quatro recortes da tela Contas & Cartões.
 *
 * Não há consulta nova ao banco aqui: os recortes vêm de
 * `getFinancialProducts`, `getCards`, `getInstitutions` e `getStatementCycles`
 * — as actions que já existem, já fazem os JOINs certos e já filtram por
 * `ownerId`. O cruzamento dos quatro é função pura e mora em
 * `components/contas/montar-painel.ts`.
 */
import { getFinancialProducts } from "./financial-products";
import { getCards } from "./cards";
import { getInstitutions } from "./institutions";
import { getStatementCycles } from "./statement-cycles";
import { montarPainelContas, type PainelContas } from "@/components/contas/montar-painel";

// Reexportado por tipo (apagado na compilação, então não viola a regra de
// `"use server"` de só exportar função assíncrona): as telas já importam
// destes nomes daqui, e mudar o caminho em cinco arquivos não paga.
export type {
  ProdutoParaPainel,
  CartaoParaPainel,
  InstituicaoParaPainel,
  CicloParaPainel,
  InstituicaoResumo,
  ContaDetalhe,
  FaturaResumo,
  CartaoDetalhe,
  PainelContas,
} from "@/components/contas/montar-painel";

/** Busca os quatro recortes em paralelo e monta o modelo de leitura. */
export async function carregarPainelContas(): Promise<PainelContas> {
  const [produtos, cards, instituicoes, ciclos] = await Promise.all([
    getFinancialProducts(),
    getCards(),
    getInstitutions(),
    getStatementCycles(),
  ]);
  return montarPainelContas({ produtos, cards, instituicoes, ciclos });
}
