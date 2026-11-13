// Gera as saídas de `nomeNaVista`/`trechoDeNome` do TypeScript para o teste do Rust.
//   bun crates/sbf-estante/tests/gerar-nomes.ts > crates/sbf-estante/tests/fixtures/nomes-ts.json
// Os casos são dados fictícios.
import { nomeNaVista, trechoDeNome } from "../../../workers/inventario/src/vista-nome";

const textos: Array<string | null> = [
  null,
  "",
  "Nu Pagamentos S.A.",
  "Condomínio do Edifício / YES Ltda.",
  "ÁÉÍÓÚ àèìòù ãõ âêô ç ü ñ",
  "A".repeat(60),
  "  --Loja   da  Ana--  ",
  "São João & Filhos — Comércio",
  "İstanbul Ürün",
  "日本語 テスト",
  "Mercado 24h 100%",
  "é (acento combinante)",
  "x".repeat(39) + "-y-z",
  "ß straße",
  "K ELVIN",
];
const comprimentos = [40, 10, 1];
const trechos = textos.flatMap((t) =>
  comprimentos.map((max) => ({ entrada: t, max, saida: trechoDeNome(t, max) })),
);

const base = {
  data_chave: "2026-10-25" as string | null,
  supplier_name_raw: "Nu Pagamentos S.A." as string | null,
  valor_centavos: -123456 as number | null,
  content_hash: "ab12cd34ef567890",
  extensao: "pdf" as string | null,
  nome_original: "fatura.pdf" as string | null,
};
const variacoes = [
  {},
  { data_chave: null },
  { supplier_name_raw: null },
  { supplier_name_raw: "", nome_original: "Boleto Luz.final.PDF" },
  { supplier_name_raw: null, nome_original: null },
  { supplier_name_raw: null, nome_original: "semextensao" },
  { valor_centavos: null },
  { valor_centavos: 0 },
  { valor_centavos: 5 },
  { valor_centavos: 100 },
  { valor_centavos: 99999999999 },
  { valor_centavos: 123456 },
  { extensao: null },
  { extensao: "" },
  { data_chave: null, supplier_name_raw: null, nome_original: null, valor_centavos: null, extensao: null },
  { supplier_name_raw: "Padaria Pão Quente Ltda" },
];
const nomes = variacoes.map((v) => {
  const entrada = { ...base, ...v };
  return { entrada, saida: nomeNaVista(entrada) };
});

console.log(JSON.stringify({ trechos, nomes }, null, 1));
