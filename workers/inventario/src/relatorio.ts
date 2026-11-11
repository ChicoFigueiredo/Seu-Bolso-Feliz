/**
 * Gera o snapshot do painel de progresso (um JSON) a partir do índice. Só
 * contagens e estados: nenhum nome de arquivo, valor, conta ou documento.
 *
 *   bun run workers/inventario/src/relatorio.ts --saida /caminho/painel.json
 *
 * O estado que só eu sei (etapas abertas/fechadas, tarefas que você me disse
 * que fez, avisos) vive em <cofre>/painel/estado.json; o resto é medido.
 */
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { abrirBanco } from "./banco";
import { cicloDe, hojeIso } from "./ciclo";
import { caminhosDoCofre } from "./cofre";

interface Estado {
  etapas: Record<string, { status: string; detalhe?: string }>;
  tarefasFeitas: string[];
  avisos: Array<{ nivel: "info" | "atencao" | "problema"; aviso: string; desde: string }>;
}

const FERRAMENTAS = ["qpdf", "ocrmypdf", "exiftool", "readpst", "unrar", "rclone", "tesseract"];

function temComando(nome: string): boolean {
  return (process.env.PATH ?? "").split(":").some((dir) => dir && existsSync(join(dir, nome)));
}

function temArquivos(dir: string): boolean {
  try {
    return readdirSync(dir).some((n) => !n.startsWith("."));
  } catch {
    return false;
  }
}

function tesseractPortugues(): boolean {
  return ["/usr/share/tesseract-ocr/5/tessdata", "/usr/share/tesseract-ocr/4.00/tessdata"].some(
    (d) => existsSync(join(d, "por.traineddata")),
  );
}

function rcloneRemotos(): number {
  const conf = join(homedir(), ".config/rclone/rclone.conf");
  if (!existsSync(conf)) return 0;
  return (readFileSync(conf, "utf8").match(/^\[[^\]]+\]/gm) ?? []).length;
}

function thunderbirdOutlook(): boolean {
  const base = join(homedir(), "snap/thunderbird/common/.thunderbird");
  try {
    return readdirSync(base).some((perfil) => {
      const imap = join(base, perfil, "ImapMail");
      return existsSync(imap) && readdirSync(imap).some((s) => /outlook|office365/i.test(s));
    });
  } catch {
    return false;
  }
}

function lerEstado(arquivo: string): Estado {
  if (existsSync(arquivo)) return JSON.parse(readFileSync(arquivo, "utf8")) as Estado;
  return { etapas: {}, tarefasFeitas: [], avisos: [] };
}

function main(): void {
  const i = process.argv.indexOf("--saida");
  const cofre = caminhosDoCofre();
  const saida = i >= 0 ? process.argv[i + 1]! : join(cofre.raiz, "painel/painel.json");
  const estado = lerEstado(join(cofre.raiz, "painel/estado.json"));
  const db = abrirBanco(cofre.banco);
  const um = <T>(sql: string, ...p: Array<string | number>) => db.query(sql).get(...p) as T;
  const todos = <T>(sql: string, ...p: Array<string | number>) => db.query(sql).all(...p) as T[];
  const entrada = (sub: string) => join(cofre.entrada, sub);

  const feitoPorMim = (id: string) => estado.tarefasFeitas.includes(id);
  const ferramentasOk = FERRAMENTAS.every(temComando) && tesseractPortugues();
  const tarefas = [
    {
      id: "t01",
      tarefa: "Instalar ferramentas (apt)",
      etapa: "E0",
      feito: ferramentasOk,
      como: "! sudo apt install -y qpdf ocrmypdf tesseract-ocr-por libimage-exiftool-perl pst-utils unrar rclone",
    },
    {
      id: "t02",
      tarefa: "Criar o cofre de segredos (senha-mestra + CPF)",
      etapa: "E0",
      feito: existsSync(join(cofre.segredos, "cofre.enc")),
      como: "Num terminal seu: bun run inventario:segredos criar",
    },
    {
      id: "t03",
      tarefa: "Conectar OneDrive e Google Drive no rclone",
      etapa: "E2",
      feito: rcloneRemotos() >= 2,
      como: "rclone config: um remoto onedrive e um drive (somente leitura)",
    },
    {
      id: "t04",
      tarefa: "Copiar o celular por USB",
      etapa: "E2",
      feito: temArquivos(entrada("celular")),
      como: "WhatsApp/Media, DCIM, Pictures/Screenshots e Download → entrada/celular/",
    },
    {
      id: "t05",
      tarefa: "Backup de SMS em XML",
      etapa: "E2",
      feito: temArquivos(entrada("sms")),
      como: "App SMS Backup & Restore → arquivo .xml em entrada/sms/",
    },
    {
      id: "t06",
      tarefa: "Exportar o Telegram",
      etapa: "E2",
      feito: temArquivos(entrada("telegram")),
      como: "Telegram Desktop › Avançado › Exportar dados: pessoais + Salvas, fotos e arquivos, JSON, 12 meses → entrada/telegram/",
    },
    {
      id: "t07",
      tarefa: "Thunderbird com a conta outlook.com.br offline",
      etapa: "E2",
      feito: thunderbirdOutlook(),
      como: "Adicionar a conta (login no navegador) e marcar mensagens offline",
    },
    {
      id: "t08",
      tarefa: "Faturas dos 2 cartões, 12 meses",
      etapa: "E2",
      feito: temArquivos(entrada("bancos/faturas")),
      como: "PDFs do internet banking → entrada/bancos/faturas/",
    },
    {
      id: "t09",
      tarefa: "Consignados e empréstimos",
      etapa: "E2",
      feito: temArquivos(entrada("bancos/emprestimos")),
      como: "Contratos e saldo devedor → entrada/bancos/emprestimos/",
    },
    {
      id: "t10",
      tarefa: "Extratos OFX de CAIXA, C6 e Nubank",
      etapa: "E2",
      feito: temArquivos(entrada("bancos/ofx")),
      como: "OFX dos últimos 12 meses → entrada/bancos/ofx/",
    },
    {
      id: "t11",
      tarefa: "Extrato do VR/VA",
      etapa: "E2",
      feito: temArquivos(entrada("bancos/beneficios")),
      como: "Extrato do app do benefício → entrada/bancos/beneficios/",
    },
    {
      id: "t12",
      tarefa: "Pedir exportação LGPD aos apps de compra",
      etapa: "E2",
      feito: feitoPorMim("t12"),
      como: 'iFood, Uber, 99, Amazon, Mercado Livre, Shopee: pedir "meus dados"; me avise quando pedir',
    },
    {
      id: "t13",
      tarefa: "Sessão no portal de RH (contracheques, 12 meses)",
      etapa: "E2",
      feito: feitoPorMim("t13"),
      como: "Você faz login e eu navego, do mais recente para trás",
    },
  ].map(({ feito, ...t }) => ({ ...t, status: feito ? "feito" : "pendente" }));

  // Fontes: a varredura mais recente de cada uma.
  const fontes = todos<{
    id: string;
    descricao: string;
    escopo: string;
    motivo: string | null;
    status: string | null;
    vistos: number | null;
    novos: number | null;
    erros: number | null;
    resumo: string | null;
    quando: string | null;
  }>(
    `SELECT f.id, f.descricao, f.escopo, f.motivo, v.status, v.vistos, v.novos, v.erros, v.resumo,
            coalesce(v.concluida_em, v.iniciada_em) AS quando
       FROM fonte f
       LEFT JOIN varredura v ON v.id = (SELECT max(id) FROM varredura WHERE fonte_id = f.id)
      ORDER BY f.escopo, f.id`,
  ).map((f) => {
    const resumo = f.resumo ? (JSON.parse(f.resumo) as { copiados?: number }) : {};
    const status = f.escopo === "fora" ? "fora" : (f.status ?? "ausente");
    return {
      id: f.id,
      fonte: f.descricao,
      status,
      vistos: f.vistos ?? 0,
      novos: f.novos ?? 0,
      copiados: resumo.copiados ?? null,
      erros: f.erros ?? 0,
      atualizado: f.quando,
      nota: f.motivo,
    };
  });

  const areas = todos<{ motivo: string; areas: number; arquivos: number; bytes: number }>(
    `SELECT motivo, count(*) AS areas, sum(arquivos) AS arquivos, sum(bytes) AS bytes
       FROM area_excluida a
      WHERE a.varredura_id = (SELECT max(id) FROM varredura WHERE fonte_id = a.fonte_id AND etapa = 'E1')
      GROUP BY motivo ORDER BY arquivos DESC`,
  ).map((a) => ({ ...a, gb: Math.round((a.bytes / 1024 ** 3) * 10) / 10 }));

  const hoje = hojeIso();
  const cicloAtual = cicloDe(hoje);
  const ciclos = todos<{ id: string; inicio: string; fim: string }>(
    "SELECT id, inicio, fim FROM ciclo WHERE id <= ? ORDER BY id DESC LIMIT 13",
    cicloAtual,
  ).map((c) => ({
    ciclo: c.id,
    periodo: `${c.inicio.slice(8)}/${c.inicio.slice(5, 7)} a ${c.fim.slice(8)}/${c.fim.slice(5, 7)}/${c.fim.slice(0, 4)}`,
    documentos: um<{ n: number }>(
      "SELECT count(*) n FROM achado WHERE ciclo_id = ? AND estado = 'documento'",
      c.id,
    ).n,
    registros: um<{ n: number }>("SELECT count(*) n FROM registro WHERE ciclo_id = ?", c.id).n,
    situacao: c.id === cicloAtual ? "atual" : "completo",
  }));

  const discos = fontes.filter((f) => f.id.startsWith("disco:"));
  const discosConcluidos = discos.filter((f) => f.status === "concluida").length;
  const tarefaFeita = (id: string) => tarefas.find((t) => t.id === id)?.status === "feito";
  const etapasBase = [
    {
      etapa: "E0",
      nome: "Preparar",
      status: tarefaFeita("t01") && tarefaFeita("t02") ? "concluida" : "aguardando-voce",
      detalhe: "Cofre, índice e worker prontos; faltam ferramentas e cofre de segredos",
    },
    {
      etapa: "E1",
      nome: "Scan dos discos",
      status:
        discosConcluidos === discos.length
          ? "concluida"
          : discos.some((d) => d.status === "rodando")
            ? "rodando"
            : "aberta",
      detalhe: `${discosConcluidos} de ${discos.length} discos concluídos`,
    },
    { etapa: "E2", nome: "Fontes vivas", status: "aberta", detalhe: "" },
    { etapa: "E3", nome: "Organizar", status: "aguardando", detalhe: "" },
    { etapa: "E4", nome: "Fragmentar", status: "aguardando", detalhe: "" },
    {
      etapa: "E5",
      nome: "Auditoria e cobertura",
      status: "aguardando",
      detalhe: "Relatório às 06:00",
    },
  ];
  const etapas = etapasBase.map((e) => ({ ...e, ...estado.etapas[e.etapa] }));

  const avisos = [...estado.avisos];
  const comErro = fontes.filter((f) => f.erros > 0);
  if (comErro.length) {
    avisos.push({
      nivel: "atencao",
      aviso: `Erros de leitura em ${comErro.map((f) => f.id).join(", ")} (detalhes nos logs do cofre)`,
      desde: hoje,
    });
  }

  const acervoBytes = um<{ b: number | null }>(
    "SELECT sum(file_size_bytes) b FROM achado WHERE caminho_acervo IS NOT NULL",
  ).b;
  const resumo = {
    achados: um<{ n: number }>("SELECT count(*) n FROM achado").n,
    origens: um<{ n: number }>("SELECT count(*) n FROM origem").n,
    copiados: um<{ n: number }>("SELECT count(*) n FROM achado WHERE caminho_acervo IS NOT NULL").n,
    acervo_gb: Math.round(((acervoBytes ?? 0) / 1024 ** 3) * 100) / 100,
    documentos: um<{ n: number }>("SELECT count(*) n FROM achado WHERE estado = 'documento'").n,
    registros: um<{ n: number }>("SELECT count(*) n FROM registro").n,
    pendencias: um<{ n: number }>("SELECT count(*) n FROM pendencia WHERE resolvida_em IS NULL").n,
    fontes_concluidas: fontes.filter((f) => f.status === "concluida").length,
    fontes_dentro: fontes.filter((f) => f.status !== "fora").length,
    tarefas_pendentes: tarefas.filter((t) => t.status === "pendente").length,
    gerado_em: new Date().toISOString(),
  };
  db.close();

  mkdirSync(dirname(saida), { recursive: true });
  writeFileSync(
    saida,
    JSON.stringify({ resumo, etapas, tarefas, avisos, fontes, areas, ciclos }, null, 1),
  );
  console.log(
    `painel: ${resumo.achados} achados, ${resumo.origens} origens, ${resumo.copiados} copiados → ${saida}`,
  );
}

main();
