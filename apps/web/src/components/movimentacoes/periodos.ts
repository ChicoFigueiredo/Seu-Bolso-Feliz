/**
 * Tradução de uma chave de período ("este-periodo", "90-dias", ...) para o
 * intervalo de datas que a consulta usa.
 *
 * Mora fora de `./filtros.ts` porque importa `@sbf/domain` (o ciclo
 * financeiro do usuário, que não começa necessariamente no dia 1) e só a
 * server action precisa disso — deixar este import no módulo compartilhado
 * arrastaria o pacote inteiro de domínio para o bundle do navegador sem
 * nenhum uso lá.
 */
import { getCurrentPeriod } from "@sbf/domain";
import type { ChavePeriodo } from "./filtros";

export interface IntervaloPeriodo {
  /** `YYYY-MM-DD`, ou `null` para "sem limite". */
  inicio: string | null;
  fim: string | null;
  /** Como o intervalo se descreve para o usuário. */
  rotulo: string;
}

/**
 * `toISOString()` converteria para UTC e, em fuso positivo, jogaria a
 * meia-noite local para o dia anterior. As datas do banco são `date` puro,
 * sem hora — então a conversão certa é a leitura local dos componentes.
 */
function iso(data: Date): string {
  const mes = String(data.getMonth() + 1).padStart(2, "0");
  const dia = String(data.getDate()).padStart(2, "0");
  return `${data.getFullYear()}-${mes}-${dia}`;
}

function somarDias(data: Date, dias: number): Date {
  const nova = new Date(data);
  nova.setDate(nova.getDate() + dias);
  return nova;
}

/**
 * O intervalo de um período.
 *
 * "Este período" e "Período anterior" seguem o ciclo financeiro do usuário e
 * vão até o fim do ciclo, incluindo dias futuros — é lá que estão os
 * compromissos que ainda vão vencer. "90 dias" e os demais recortes de
 * calendário param em hoje ou no fim do ano, conforme o nome promete.
 */
export function intervaloDoPeriodo(
  chave: ChavePeriodo,
  hoje: Date,
  diaInicioCiclo: number,
): IntervaloPeriodo {
  switch (chave) {
    case "este-periodo": {
      const periodo = getCurrentPeriod({ startDay: diaInicioCiclo }, hoje);
      return { inicio: iso(periodo.startDate), fim: iso(periodo.endDate), rotulo: periodo.label };
    }
    case "periodo-anterior": {
      const atual = getCurrentPeriod({ startDay: diaInicioCiclo }, hoje);
      const anterior = getCurrentPeriod(
        { startDay: diaInicioCiclo },
        somarDias(atual.startDate, -1),
      );
      return {
        inicio: iso(anterior.startDate),
        fim: iso(anterior.endDate),
        rotulo: anterior.label,
      };
    }
    case "90-dias":
      return { inicio: iso(somarDias(hoje, -89)), fim: iso(hoje), rotulo: "Últimos 90 dias" };
    case "este-ano": {
      const ano = hoje.getFullYear();
      return { inicio: `${ano}-01-01`, fim: `${ano}-12-31`, rotulo: `Ano de ${ano}` };
    }
    case "tudo":
      return { inicio: null, fim: null, rotulo: "Todo o histórico" };
  }
}
