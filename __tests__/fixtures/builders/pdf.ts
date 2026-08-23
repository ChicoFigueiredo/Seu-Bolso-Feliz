/**
 * Gerador de PDF mínimo, sem dependências.
 *
 * Existe porque o repositório não tinha NENHUM fixture binário: toda entrada
 * de teste era uma string sintética escrita num `.csv` em tempo de execução,
 * de modo que o caminho do `pdf-parse` — o parser mais usado do sistema —
 * jamais foi exercitado por teste algum.
 *
 * Construir o PDF em código, em vez de commitar um binário, mantém o fixture
 * diffável e legível, e evita colocar um arquivo opaco no controle de versão.
 */

/** Escapa os caracteres que quebram um literal de string PDF. */
function escapePdfText(text: string): string {
  return text.replace(/\\/g, "\\\\").replace(/\(/g, "\\(").replace(/\)/g, "\\)");
}

/**
 * Monta um PDF de uma página com as linhas informadas, com a tabela xref
   correta — sem ela, leitores estritos recusam o arquivo.
 */
export function buildSimplePdf(lines: string[]): Buffer {
  const fontSize = 11;
  const leading = 15;
  const startY = 780;

  const textOps = lines
    .map(
      (line, i) =>
        `BT /F1 ${fontSize} Tf 50 ${startY - i * leading} Td (${escapePdfText(line)}) Tj ET`,
    )
    .join("\n");

  const objects: string[] = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
    `<< /Length ${textOps.length} >>\nstream\n${textOps}\nendstream`,
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
  ];

  let pdf = "%PDF-1.4\n";
  const offsets: number[] = [];

  objects.forEach((body, i) => {
    offsets.push(pdf.length);
    pdf += `${i + 1} 0 obj\n${body}\nendobj\n`;
  });

  const xrefStart = pdf.length;
  pdf += `xref\n0 ${objects.length + 1}\n`;
  pdf += "0000000000 65535 f \n";
  for (const off of offsets) {
    pdf += `${String(off).padStart(10, "0")} 00000 n \n`;
  }
  pdf += `trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${xrefStart}\n%%EOF\n`;

  return Buffer.from(pdf, "latin1");
}
