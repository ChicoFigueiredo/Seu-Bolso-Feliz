/**
 * E3 (Pluggy): separa o que é gasto do que é só movimento entre contas.
 * Aplica classificarMovimento aos registros de 'conexao:pluggy' e grava
 * registro.tipo (só transferencia_interna, pagamento_fatura, investimento,
 * estorno) e fornecedor_normalizado. Idempotente: o resultado depende só da
 * descrição, do fornecedor e do sinal do valor, não do tipo já gravado.
 *
 *   bun run workers/inventario/src/e3-pluggy.ts --simular   # não grava
 *   bun run workers/inventario/src/e3-pluggy.ts
 *
 * O tipo de antes (compra, debito, credito) fica em tipo_original e a
 * classificação parte sempre dele: mudar uma regra e rodar de novo recalcula
 * tudo, inclusive o que deixou de ser reclassificado.
 */
import { abrirBanco, aplicarSchema } from "./banco";
import { caminhosDoCofre } from "./cofre";
import { classificarMovimento, type EntradaMovimento } from "./pluggy-normalizar";

interface Linha extends EntradaMovimento {
  id: number;
  /** tipo gravado hoje (pode já ser o reclassificado). */
  tipo_atual: string;
  fornecedor_normalizado: string | null;
}

const reais = (c: number) =>
  (c / 100).toLocaleString("pt-BR", { style: "currency", currency: "BRL" });

function main(): void {
  const simular = process.argv.includes("--simular");
  const db = abrirBanco(caminhosDoCofre().banco);
  aplicarSchema(db);

  const linhas = db
    .query(
      `SELECT id, descricao_original,
              coalesce(nullif(supplier_name_raw, ''), contraparte) AS supplier_name_raw,
              coalesce(tipo_original, tipo) AS tipo,
              tipo AS tipo_atual, valor_centavos, conta_provavel, categoria_provavel,
              fornecedor_normalizado
         FROM registro WHERE fonte_id = 'conexao:pluggy' AND tipo <> 'fatura_cartao'`,
    )
    .all() as Linha[];

  const porTipo = new Map<string, { n: number; saidas: number; entradas: number }>();
  const porFornecedor = new Map<string, { n: number; saidas: number }>();
  const mudancas: { id: number; tipo: string; original: string; fornecedor: string | null }[] = [];

  for (const l of linhas) {
    const c = classificarMovimento(l);
    const tipoFinal = c.tipo ?? l.tipo;
    if (c.tipo) {
      const t = porTipo.get(c.tipo) ?? { n: 0, saidas: 0, entradas: 0 };
      t.n++;
      if (l.valor_centavos < 0) t.saidas += -l.valor_centavos;
      else t.entradas += l.valor_centavos;
      porTipo.set(c.tipo, t);
    } else if (l.valor_centavos < 0) {
      const chave = c.fornecedor_normalizado ?? `(sem contraparte) ${l.descricao_original ?? ""}`;
      const f = porFornecedor.get(chave) ?? { n: 0, saidas: 0 };
      f.n++;
      f.saidas += -l.valor_centavos;
      porFornecedor.set(chave, f);
    }
    if (tipoFinal !== l.tipo_atual || c.fornecedor_normalizado !== l.fornecedor_normalizado) {
      mudancas.push({
        id: l.id,
        tipo: tipoFinal,
        original: l.tipo,
        fornecedor: c.fornecedor_normalizado,
      });
    }
  }

  console.log(`registros pluggy: ${linhas.length}; a alterar: ${mudancas.length}`);
  console.log("\nreclassificados por tipo (saídas | entradas):");
  for (const [tipo, t] of [...porTipo].sort()) {
    console.log(
      `  ${tipo.padEnd(22)} ${String(t.n).padStart(5)}  ${reais(t.saidas).padStart(16)} | ${reais(t.entradas)}`,
    );
  }
  console.log("\ntop 25 fornecedores por saída (sem movimento interno):");
  const top = [...porFornecedor].sort((a, b) => b[1].saidas - a[1].saidas).slice(0, 25);
  for (const [nome, f] of top) {
    console.log(`  ${reais(f.saidas).padStart(16)}  ${String(f.n).padStart(4)}x  ${nome}`);
  }

  if (simular) {
    console.log("\n--simular: nada gravado");
    db.close();
    return;
  }

  const atualizar = db.query(
    `UPDATE registro SET tipo = ?, tipo_original = coalesce(tipo_original, ?),
       fornecedor_normalizado = ? WHERE id = ?`,
  );
  db.transaction(() => {
    for (const m of mudancas) atualizar.run(m.tipo, m.original, m.fornecedor, m.id);
  })();
  // Reclassificar não traz registro novo: novos = 0, o que mudou vai no resumo.
  db.query(
    `INSERT INTO varredura (fonte_id, etapa, escopo, responsavel, status, concluida_em, vistos, novos, resumo)
     VALUES ('conexao:pluggy', 'E3', 'classificar movimentos', 'e3-pluggy@1', 'concluida',
             strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?, 0, ?)`,
  ).run(linhas.length, JSON.stringify({ alterados: mudancas.length }));
  console.log(`\ngravados ${mudancas.length} registros`);
  db.close();
}

if (import.meta.main) {
  try {
    main();
  } catch (e) {
    console.error(e instanceof Error ? e.message : String(e));
    process.exit(1);
  }
}
