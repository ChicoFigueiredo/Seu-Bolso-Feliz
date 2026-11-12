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

/** Remotos prontos: com tipo onedrive ou drive e, no OneDrive, o drive escolhido. */
function rcloneRemotos(): number {
  const conf = join(homedir(), ".config/rclone/rclone.conf");
  if (!existsSync(conf)) return 0;
  const blocos = readFileSync(conf, "utf8")
    .split(/^\[[^\]]+\]\s*$/m)
    .slice(1);
  return blocos.filter(
    (b) =>
      /^type = drive$/m.test(b) || (/^type = onedrive$/m.test(b) && /^drive_id = \S+/m.test(b)),
  ).length;
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
      id: "t14",
      tarefa: "Renovar o token do Gmail (o atual expirou)",
      etapa: "E2",
      feito: feitoPorMim("t14"),
      como: "Num terminal seu: bun run get:gmail-token, autorizar no navegador e trocar GMAIL_REFRESH_TOKEN no .env e no .env.local",
    },
    {
      id: "t03",
      tarefa: "Conectar OneDrive e Google Drive no rclone (somente leitura)",
      etapa: "E2",
      feito: rcloneRemotos() >= 2,
      como:
        "Num terminal: rclone config. ONEDRIVE: n → nome onedrive → tipo onedrive → Enter em client_id e client_secret → região global → " +
        "Edit advanced config? y → access_scopes: a opção de leitura (Files.Read…) → Enter no resto → navegador: login → OneDrive Personal → y. " +
        "GOOGLE DRIVE: n → nome gdrive → tipo drive → Enter em client_id e client_secret → scope 2 (drive.readonly) → Enter no resto → " +
        "advanced? n → navegador: login → shared drive? n → y. Depois q para sair. Os nomes precisam ser onedrive e gdrive.",
    },
    {
      id: "t04",
      tarefa: "Copiar o celular por USB",
      etapa: "E2",
      feito: feitoPorMim("t04"),
      como: "Outro agente está copiando para /mnt/bak/Pessoal/Chico/Celular/Xiomi; o scan pula essa pasta até você avisar que terminou",
    },
    {
      id: "t15",
      tarefa: "Exportar conversas do WhatsApp com dinheiro no meio",
      etapa: "E2",
      feito: temArquivos(entrada("whatsapp")),
      como: "Em cada conversa: ⋮ → Mais → Exportar conversa → Incluir mídia; o .zip vai para entrada/whatsapp/ (guia: Coleta do Inventário)",
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

  // Fontes: uma fonte pode ter várias rodadas (completa, retomada, só áreas A).
  // Situação = rodando se alguma roda, senão a da última; vistos = a rodada
  // mais completa; novos e copiados somam todas (rodadas não repetem origem).
  const fontes = todos<{
    id: string;
    descricao: string;
    escopo: string;
    motivo: string | null;
    status: string | null;
    vistos: number | null;
    novos: number | null;
    copiados: number | null;
    erros: number | null;
    quando: string | null;
  }>(
    `SELECT f.id, f.descricao, f.escopo, f.motivo,
            (SELECT status FROM varredura WHERE fonte_id = f.id
              ORDER BY (status = 'rodando') DESC, id DESC LIMIT 1) AS status,
            (SELECT max(vistos) FROM varredura WHERE fonte_id = f.id) AS vistos,
            (SELECT sum(novos) FROM varredura WHERE fonte_id = f.id) AS novos,
            (SELECT sum(json_extract(resumo, '$.copiados')) FROM varredura WHERE fonte_id = f.id) AS copiados,
            (SELECT erros FROM varredura WHERE fonte_id = f.id ORDER BY vistos DESC LIMIT 1) AS erros,
            (SELECT max(coalesce(concluida_em, iniciada_em)) FROM varredura WHERE fonte_id = f.id) AS quando
       FROM fonte f
      ORDER BY f.escopo, f.id`,
  ).map((f) => ({
    id: f.id,
    fonte: f.descricao,
    status: f.escopo === "fora" ? "fora" : (f.status ?? "ausente"),
    vistos: f.vistos ?? 0,
    novos: f.novos ?? 0,
    copiados: f.copiados,
    erros: f.erros ?? 0,
    atualizado: f.quando,
    nota: f.motivo,
  }));

  // Datas por fonte: o período que cada uma cobre (arquivos ou mensagens,
  // documentos, registros) e quando foi varrida pela última vez.
  const br = (iso: string | null) =>
    iso ? `${iso.slice(8, 10)}/${iso.slice(5, 7)}/${iso.slice(0, 4)}` : null;
  const periodo = (de: string | null, ate: string | null) =>
    de && ate ? (de === ate ? br(de) : `${br(de)} a ${br(ate)}`) : null;
  const datas = todos<{
    id: string;
    descricao: string;
    escopo: string;
    situacao: string | null;
    ultima: string | null;
    origens: number | null;
    arq_de: string | null;
    arq_ate: string | null;
    docs: number | null;
    doc_de: string | null;
    doc_ate: string | null;
    registros: number | null;
    reg_de: string | null;
    reg_ate: string | null;
  }>(
    `WITH o AS (
       SELECT fonte_id, count(*) AS origens,
              substr(min(coalesce(nullif(local_mtime, ''), gmail_date)), 1, 10) AS arq_de,
              substr(max(coalesce(nullif(local_mtime, ''), gmail_date)), 1, 10) AS arq_ate
         FROM origem WHERE coalesce(local_mtime, gmail_date) >= '1995' GROUP BY fonte_id),
     d AS (
       SELECT x.fonte_id, count(DISTINCT a.content_hash) AS docs, min(a.data_chave) AS doc_de,
              max(a.data_chave) AS doc_ate
         FROM origem x JOIN achado a ON a.content_hash = x.content_hash
        WHERE a.estado = 'documento' AND a.data_chave >= '1995' GROUP BY x.fonte_id),
     r AS (SELECT fonte_id, count(*) AS registros, min(data) AS reg_de, max(data) AS reg_ate
             FROM registro GROUP BY fonte_id),
     v AS (SELECT fonte_id, max(coalesce(concluida_em, iniciada_em)) AS ultima,
                  (SELECT status FROM varredura w WHERE w.fonte_id = v.fonte_id
                    ORDER BY (status = 'rodando') DESC, id DESC LIMIT 1) AS situacao
             FROM varredura v GROUP BY fonte_id)
     SELECT f.id, f.descricao, f.escopo, v.situacao, v.ultima, o.origens, o.arq_de, o.arq_ate,
            d.docs, d.doc_de, d.doc_ate, r.registros, r.reg_de, r.reg_ate
       FROM fonte f LEFT JOIN o ON o.fonte_id = f.id LEFT JOIN d ON d.fonte_id = f.id
       LEFT JOIN r ON r.fonte_id = f.id LEFT JOIN v ON v.fonte_id = f.id
      ORDER BY (v.situacao IS NULL), coalesce(o.arq_ate, r.reg_ate) DESC, f.id`,
  ).map((x) => ({
    id: x.id,
    fonte: x.descricao,
    situacao: x.escopo === "fora" ? "fora" : (x.situacao ?? "ausente"),
    ultima_varredura: x.ultima,
    itens: x.origens ?? x.registros ?? 0,
    periodo_itens: periodo(x.arq_de ?? x.reg_de, x.arq_ate ?? x.reg_ate),
    documentos: x.docs ?? 0,
    periodo_documentos: periodo(x.doc_de, x.doc_ate),
  }));

  const areas = todos<{ motivo: string; areas: number; arquivos: number; bytes: number }>(
    `SELECT motivo, count(*) AS areas, sum(arquivos) AS arquivos, sum(bytes) AS bytes
       FROM area_excluida a
      WHERE a.varredura_id = (SELECT id FROM varredura WHERE fonte_id = a.fonte_id AND etapa = 'E1'
                               ORDER BY vistos DESC LIMIT 1)
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
    JSON.stringify({ resumo, etapas, tarefas, avisos, fontes, datas, areas, ciclos }, null, 1),
  );
  console.log(
    `painel: ${resumo.achados} achados, ${resumo.origens} origens, ${resumo.copiados} copiados → ${saida}`,
  );
}

main();
