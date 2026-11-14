// Gera o ouro de `areas.ts` para o teste do Rust (dados fictícios; os caminhos são os da tabela
// de regras, que é código, e variações inventadas).
//   bun crates/sbf-coleta/tests/gerar-ouro.ts > crates/sbf-coleta/tests/fixtures/areas-ts.json
import { classeDoCaminho, REGRAS_POR_CAMINHO, tratamentoDoArquivo, nomeFinanceiro, extensaoDe } from "../../../workers/inventario/src/areas";

const sufixos = [
  "",
  "/Docs",
  "/node_modules/x",
  "/Daiane/ir/2024",
  "/Daiane/outros",
  "/Mayara/Contas",
  "/IRPF2024/Daiane/x",
  "/Imposto de Renda/Daiane",
  "/.git/objects",
  "/AppData/Local",
  "/OneDrive.Dai/x",
  "//dupla//barra",
  "/Pasta com espaço/ç",
];
const raizes = [...REGRAS_POR_CAMINHO.keys(), "/mnt/zzz/qualquer", "/home/pessoa/Documentos", "/", "/tmp/x"];
const caminhos = raizes.flatMap((r) => sufixos.map((s) => (r === "/" ? s || "/" : r + s)));

const nomes = [
  "fatura.pdf", "Fatura Cartão.PDF", "extrato-2026.ofx", "foto.jpg", "foto-comprovante.jpg",
  "musica.mp3", "app.exe", "notas.txt", "notas-fiscais.txt", "boleto.html", "arquivo.zip",
  "NFe 123.xml", "nf-e.pdf", "nfse.pdf", "nfs-e.pdf", "pix.png", "pixel.png", "o pix.jpg",
  "Condomínio março.pdf", "CONTRA-CHEQUE.pdf", "contra cheque.pdf", "holerite.docx",
  "informe de rendimentos.pdf", "Informes_Rendimento.pdf", "IRPF2025.zip", "darf.pdf",
  "sem-extensao", ".bashrc", ".bashrc.pdf", "a.tar.gz", "termina.", "x.PDF", "emprestimo.csv",
  "Pensão alimentícia.pdf", "consignado.pdf", "financiamento.doc", "recibo.heic",
  "comprovante^x.pdf", "fa`tura.pdf", "ÁGUA e esgôto fatura.pdf", "iptu.png", "ipva.jpeg",
  "nota_fiscal.rtf", "nota.fiscal.rtf", "nota-fiscal.7z", "pensao.tif", "danfe.mbox",
];

// Um caminho por (camada, motivo, ignorar): o tratamento só depende da classe.
const vistos = new Set<string>();
const representantes = caminhos.filter((c) => {
  const k = classeDoCaminho(c);
  const chave = `${k.camada}|${k.motivo ?? ""}|${k.ignorar ?? false}`;
  if (vistos.has(chave)) return false;
  vistos.add(chave);
  return true;
});

const saida = {
  classes: caminhos.map((c) => {
    const k = classeDoCaminho(c);
    return { caminho: c, camada: k.camada, motivo: k.motivo ?? null, ignorar: k.ignorar ?? false, raiz: k.raiz };
  }),
  nomes: nomes.map((n) => ({ nome: n, ext: extensaoDe(n), financeiro: nomeFinanceiro(n) })),
  tratamentos: representantes.flatMap((c) =>
    nomes.map((n) => [c, n, tratamentoDoArquivo(n, classeDoCaminho(c))] as const),
  ),
};
console.log(JSON.stringify(saida));
