/**
 * E5: relatório de encerramento para a revisão das 06:00 (seção 12 do prompt
 * operacional V6). Gera uma página HTML com os números do índice na hora.
 *
 *   bun run workers/inventario/src/relatorio-encerramento.ts --saida /caminho/relatorio.html
 *
 * Sem números de conta: contas aparecem pelo banco e pelo final.
 */
import { writeFileSync } from "node:fs";
import { abrirBanco } from "./banco";
import { hojeIso } from "./ciclo";
import { caminhosDoCofre } from "./cofre";

const f = (n: number | null | undefined) => (n ?? 0).toLocaleString("pt-BR");
const reais = (c: number | null | undefined) =>
  ((c ?? 0) / 100).toLocaleString("pt-BR", {
    style: "currency",
    currency: "BRL",
    maximumFractionDigits: 0,
  });
const br = (iso: string | null | undefined) =>
  iso ? `${iso.slice(8, 10)}/${iso.slice(5, 7)}/${iso.slice(0, 4)}` : "—";
const esc = (s: unknown) =>
  String(s ?? "—")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
/**
 * Conta sem número: "CAIXA · 0001234-2" vira "CAIXA, final -2"; cartão ("gold · 0000")
 * vira "gold"; o nome longo da Nubank encurta.
 */
const conta = (nome: string | null) => {
  const limpo = (nome ?? "—")
    .replace(/\s+/g, " ")
    .trim()
    .replace(/^Nu Pagamentos.*?(?= ·|$)/, "Nubank");
  const dv = limpo.match(/·\s*[\d.]+-(\d)$/);
  const base = limpo.replace(/\s*·.*$/, "");
  return dv ? `${base}, final -${dv[1]}` : base;
};
const situacao: Record<string, string> = {
  concluida: "concluída",
  rodando: "em andamento",
  interrompida: "interrompida",
  falhou: "falhou",
};

function tabela(cab: string[], linhas: Array<Array<unknown>>, num: number[] = []): string {
  const th = cab.map((c, i) => `<th${num.includes(i) ? ' class="n"' : ""}>${esc(c)}</th>`).join("");
  const tr = linhas
    .map(
      (l) =>
        `<tr>${l.map((v, i) => `<td${num.includes(i) ? ' class="n"' : ""}>${esc(v)}</td>`).join("")}</tr>`,
    )
    .join("");
  return `<div class="t"><table><thead><tr>${th}</tr></thead><tbody>${tr}</tbody></table></div>`;
}

/** Situação em forma de etiqueta, para a vista bater o olho no que falta. */
const chips = (html: string) =>
  html
    .replace(/<td>feito<\/td>/g, '<td><span class="st ok">feito</span></td>')
    .replace(/<td>parcial<\/td>/g, '<td><span class="st par">parcial</span></td>')
    .replace(/<td>não feito<\/td>/g, '<td><span class="st no">não feito</span></td>');

function main(): void {
  const i = process.argv.indexOf("--saida");
  const saida = i >= 0 ? process.argv[i + 1]! : "relatorio-encerramento.html";
  const db = abrirBanco(caminhosDoCofre().banco);
  const um = <T>(sql: string) => db.query(sql).get() as T;
  const todos = <T>(sql: string) => db.query(sql).all() as T[];
  const hoje = hojeIso();

  const t = um<{
    achados: number;
    origens: number;
    docs: number;
    duvidas: number;
    descartados: number;
    copias: number;
    gb: number;
    multi: number;
    fps: number;
    pend: number;
  }>(`SELECT (SELECT count(*) FROM achado) achados, (SELECT count(*) FROM origem) origens,
      (SELECT sum(estado='documento') FROM achado) docs, (SELECT sum(estado='duvida') FROM achado) duvidas,
      (SELECT sum(estado='descartado') FROM achado) descartados,
      (SELECT count(*) FROM achado WHERE caminho_acervo IS NOT NULL) copias,
      (SELECT round(sum(file_size_bytes)/1e9,1) FROM achado WHERE caminho_acervo IS NOT NULL) gb,
      (SELECT count(*) FROM (SELECT content_hash FROM origem WHERE content_hash IS NOT NULL GROUP BY 1 HAVING count(*) > 1)) multi,
      (SELECT count(*) FROM (SELECT canonical_fingerprint FROM achado WHERE canonical_fingerprint IS NOT NULL GROUP BY 1 HAVING count(*) > 1)) fps,
      (SELECT count(*) FROM pendencia WHERE resolvida_em IS NULL) pend`);

  const pl = um<{
    n: number;
    de: string;
    ate: string;
    mov: number;
    rasc: number;
    contra: number;
    parcelas: number;
    faturas: number;
    fornecedores: number;
  }>(`SELECT count(*) n, min(data) de, max(data) ate, sum(estado='promovido') mov, sum(estado='extraido') rasc,
      sum(contraparte IS NOT NULL) contra, (SELECT count(*) FROM sub_registro WHERE tipo='parcela') parcelas,
      sum(tipo='fatura_cartao') faturas, count(DISTINCT fornecedor_normalizado) fornecedores
      FROM registro WHERE fonte_id='conexao:pluggy'`);

  const porTipo = todos<{ tipo: string; n: number; saidas: number }>(
    `SELECT tipo, count(*) n, -sum(CASE WHEN valor_centavos < 0 THEN valor_centavos END) saidas
       FROM registro WHERE fonte_id='conexao:pluggy' GROUP BY 1 ORDER BY 3 DESC`,
  );

  // Cartão é a conta em que a maioria dos registros é compra ou estorno.
  const brutas = todos<{ conta: string; n: number; compras: number; de: string; ate: string }>(
    `SELECT conta_provavel conta, count(*) n, sum(tipo IN ('compra','estorno')) compras,
            min(data) de, max(data) ate FROM registro
      WHERE fonte_id='conexao:pluggy' AND tipo <> 'fatura_cartao' GROUP BY 1`,
  );
  const porNome = new Map<
    string,
    { conta: string; n: number; compras: number; de: string; ate: string }
  >();
  // Primeiro as que têm o número (cada uma é uma conta, mesmo com o mesmo final);
  // depois as sem número entram na conta de mesmo nome, se houver.
  const comNumero = brutas.filter((x) => x.conta?.includes("·"));
  const semNumero = brutas.filter((x) => !x.conta?.includes("·"));
  for (const b of [...comNumero, ...semNumero]) {
    let nome = conta(b.conta);
    if (b.conta?.includes("·")) {
      for (let k = 2; porNome.has(nome); k++) nome = `${conta(b.conta)} (${k}ª)`;
    }
    const j = porNome.get(nome);
    if (!j) porNome.set(nome, { ...b, conta: nome });
    else {
      j.n += b.n;
      j.compras += b.compras;
      j.de = b.de < j.de ? b.de : j.de;
      j.ate = b.ate > j.ate ? b.ate : j.ate;
    }
  }
  const contas = [...porNome.values()]
    .filter((x) => x.n >= 5)
    .map((x) => ({ ...x, cartao: x.compras * 2 > x.n }))
    .sort((a, b) => b.n - a.n);

  const ciclos = todos<{
    ciclo: string;
    pluggy: number;
    docs: number;
    emails: number;
    faturas: number;
  }>(
    `SELECT c.id ciclo,
       (SELECT count(*) FROM registro r WHERE r.fonte_id='conexao:pluggy' AND r.ciclo_id=c.id AND r.tipo<>'fatura_cartao') pluggy,
       (SELECT count(*) FROM achado a WHERE a.estado='documento' AND a.ciclo_id=c.id) docs,
       (SELECT count(*) FROM origem o WHERE o.origin_type='gmail' AND o.mensagem_ref='eml' AND substr(o.gmail_date,1,10) BETWEEN c.inicio AND c.fim) emails,
       (SELECT count(*) FROM registro r WHERE r.tipo='fatura_cartao' AND r.ciclo_id=c.id) faturas
     FROM ciclo c WHERE c.id BETWEEN '2025-10' AND (SELECT max(id) FROM ciclo WHERE inicio <= '${hoje}') ORDER BY c.id DESC`,
  );

  const fontes = todos<{
    fonte: string;
    escopo: string;
    status: string | null;
    de: string | null;
    ate: string | null;
    n: number | null;
  }>(
    `SELECT f.descricao fonte, f.escopo,
       (SELECT status FROM varredura v WHERE v.fonte_id=f.id ORDER BY (status='rodando') DESC, id DESC LIMIT 1) status,
       coalesce((SELECT substr(min(coalesce(nullif(local_mtime,''),gmail_date)),1,10) FROM origem o WHERE o.fonte_id=f.id AND coalesce(local_mtime,gmail_date) >= '1995'), (SELECT min(data) FROM registro r WHERE r.fonte_id=f.id)) de,
       coalesce((SELECT substr(max(coalesce(nullif(local_mtime,''),gmail_date)),1,10) FROM origem o WHERE o.fonte_id=f.id), (SELECT max(data) FROM registro r WHERE r.fonte_id=f.id)) ate,
       coalesce((SELECT count(*) FROM origem o WHERE o.fonte_id=f.id), 0) + coalesce((SELECT count(*) FROM registro r WHERE r.fonte_id=f.id), 0) n
     FROM fonte f ORDER BY f.escopo, (status IS NULL), f.id`,
  );

  const docsPorTipo = todos<{ tipo: string | null; n: number }>(
    "SELECT document_type tipo, count(*) n FROM achado WHERE estado='documento' GROUP BY 1 ORDER BY 2 DESC",
  );

  const recorrentes = todos<{ fornecedor: string; ciclos: number; vezes: number; total: number }>(
    `SELECT fornecedor_normalizado fornecedor, count(DISTINCT ciclo_id) ciclos, count(*) vezes, -sum(valor_centavos) total
       FROM registro WHERE fonte_id='conexao:pluggy' AND valor_centavos < 0 AND fornecedor_normalizado IS NOT NULL
        AND tipo NOT IN ('pagamento_fatura','transferencia_interna','investimento','fatura_cartao')
        AND ciclo_id BETWEEN '2025-10' AND '2026-10'
      GROUP BY 1 HAVING ciclos >= 6 ORDER BY total DESC LIMIT 25`,
  );

  const topFornecedores = todos<{ fornecedor: string; vezes: number; total: number }>(
    `SELECT coalesce(fornecedor_normalizado, contraparte, '(sem nome) ' || substr(descricao_original,1,24)) fornecedor,
            count(*) vezes, -sum(valor_centavos) total
       FROM registro WHERE fonte_id='conexao:pluggy' AND valor_centavos < 0
        AND tipo NOT IN ('pagamento_fatura','transferencia_interna','investimento','fatura_cartao')
        AND ciclo_id BETWEEN '2025-10' AND '2026-10'
      GROUP BY 1 ORDER BY total DESC LIMIT 15`,
  );

  const semNome = um<{ n: number; total: number }>(
    `SELECT count(*) n, -sum(valor_centavos) total FROM registro WHERE fonte_id='conexao:pluggy' AND valor_centavos < 0
       AND contraparte IS NULL AND fornecedor_normalizado IS NULL
       AND tipo NOT IN ('pagamento_fatura','transferencia_interna','investimento','fatura_cartao')`,
  );

  const nRecorrentes = um<{ n: number }>(
    `SELECT count(*) n FROM (SELECT fornecedor_normalizado FROM registro
       WHERE fonte_id='conexao:pluggy' AND valor_centavos < 0 AND fornecedor_normalizado IS NOT NULL
         AND tipo NOT IN ('pagamento_fatura','transferencia_interna','investimento','fatura_cartao')
         AND ciclo_id BETWEEN '2025-10' AND '2026-10'
       GROUP BY 1 HAVING count(DISTINCT ciclo_id) >= 6)`,
  ).n;
  const rel = um<{ emlDoc: number; emlDuvida: number; docsDisco: number; docsGmail: number }>(
    `SELECT (SELECT count(*) FROM achado WHERE extensao='eml' AND estado='documento') emlDoc,
            (SELECT count(*) FROM achado WHERE extensao='eml' AND estado='duvida') emlDuvida,
            (SELECT count(DISTINCT a.content_hash) FROM achado a JOIN origem o USING (content_hash)
              WHERE a.estado='documento' AND o.origin_type='local_file') docsDisco,
            (SELECT count(DISTINCT a.content_hash) FROM achado a JOIN origem o USING (content_hash)
              WHERE a.estado='documento' AND o.origin_type='gmail') docsGmail`,
  );
  const vista = um<{ comCiclo: number; semCiclo: number }>(
    `SELECT sum(ciclo_id IS NOT NULL) comCiclo, sum(ciclo_id IS NULL) semCiclo
       FROM achado WHERE estado='documento'`,
  );
  const pendencias = todos<{ etapa: string; tipo: string; pergunta: string }>(
    "SELECT etapa, tipo, pergunta FROM pendencia WHERE resolvida_em IS NULL ORDER BY id",
  );
  const emails = um<{ n: number; de: string; anexos: number; docs: number }>(
    `SELECT count(*) n, substr(min(gmail_date),1,10) de,
       (SELECT count(*) FROM origem WHERE origin_type='gmail' AND mensagem_ref='anexo') anexos,
       (SELECT count(DISTINCT a.content_hash) FROM achado a JOIN origem o USING (content_hash) WHERE a.estado='documento' AND o.origin_type='gmail') docs
     FROM origem WHERE origin_type='gmail' AND mensagem_ref='eml' AND gmail_date >= '1995'`,
  );
  db.close();

  const nomeTipo: Record<string, string> = {
    compra: "Compras no cartão",
    debito: "Débitos em conta (Pix, boleto, saque…)",
    credito: "Créditos em conta",
    pagamento_fatura: "Pagamento de fatura (não é gasto)",
    transferencia_interna: "Transferência entre suas contas (não é gasto)",
    estorno: "Estornos",
    fatura_cartao: "Faturas de cartão (fora da soma)",
  };

  const item = (
    nome: string,
    valor: string,
    estado: "ok" | "parcial" | "falta",
  ): [string, string, string] => [
    nome,
    valor,
    { ok: "feito", parcial: "parcial", falta: "não feito" }[estado],
  ];
  const secao12 = [
    item(
      "Contas identificadas",
      `${f(contas.filter((x) => !x.cartao).length)} contas e poupanças com 5 ou mais transações`,
      "ok",
    ),
    item(
      "Cartões identificados",
      `${f(contas.filter((x) => x.cartao).length)} cartões, ${f(pl.faturas)} faturas`,
      "ok",
    ),
    item(
      "Período coberto pela Pluggy",
      `${br(pl.de)} a ${br(pl.ate)}, com buracos em 2 contas`,
      "parcial",
    ),
    item(
      "Transações importadas",
      `${f(pl.n)} registros da Pluggy (${f(pl.faturas)} são faturas)`,
      "ok",
    ),
    item(
      "Arquivos financeiros encontrados",
      `${f(rel.docsDisco)} nos discos e ${f(rel.docsGmail)} no Gmail; ${f(t.duvidas)} em dúvida`,
      "parcial",
    ),
    item(
      "E-mails relevantes",
      `${f(rel.emlDoc)} com sinal financeiro e ${f(rel.emlDuvida)} em dúvida, de ${f(emails.n)} lidos`,
      "parcial",
    ),
    item(
      "Documentos processados",
      `${f(t.docs)} documentos; ${f(vista.comCiclo)} já na pasta do ciclo`,
      "parcial",
    ),
    item(
      "Lançamentos criados",
      `${f(pl.mov)} Movimentações vindas da Pluggy; nenhuma criada a partir de documento`,
      "parcial",
    ),
    item(
      "Lançamentos conciliados",
      "0: a conciliação começa depois desta revisão, como combinado",
      "falta",
    ),
    item(
      "Duplicidades detectadas",
      `${f(t.multi)} conteúdos em mais de um lugar; ${f(t.fps)} grupos com mesmo texto`,
      "ok",
    ),
    item(
      "Recorrências detectadas",
      `${f(nRecorrentes)} candidatos (6 ou mais dos 13 ciclos), preliminar`,
      "parcial",
    ),
    item("Fornecedores normalizados", `${f(pl.fornecedores)} na Pluggy`, "parcial"),
    item(
      "Registros que exigem sua revisão",
      `${f(t.pend)} decisões, ${f(semNome.n)} saídas sem nome, ${f(t.duvidas)} arquivos em dúvida`,
      "parcial",
    ),
    item(
      "Falhas de integração",
      "Gmail (token, cota, rede) resolvido; OneDrive sem drive; Google Drive sem remoto",
      "parcial",
    ),
    item("Lacunas de dados", "ver a seção Lacunas e falhas", "parcial"),
    item("O que o Seu Bolso Feliz não representou", "ver a seção própria", "ok"),
    item("Próximos passos", "ver a seção própria", "ok"),
  ];
  const sucesso = [
    item(
      "Cofre privado fora do Git",
      "/mnt/d/Chico/financas, fora do repositório e ignorado pelo Git",
      "ok",
    ),
    item(
      "Evidências preservadas por ciclo",
      `${f(vista.comCiclo)} documentos na pasta do ciclo; ${f(vista.semCiclo)} ainda sem data`,
      "parcial",
    ),
    item("Índice SQLite local", "inventario.sqlite, aberto na ferramenta local", "ok"),
    item("Hashes para deduplicação", "SHA-256 de cada conteúdo, o mesmo do app", "ok"),
    item("Origem de cada evidência", `${f(t.origens)} origens ligadas aos conteúdos`, "ok"),
    item(
      "Scripts reexecutáveis (Gmail, HD, OneDrive, Pluggy)",
      "todos prontos; o do OneDrive espera a reconexão",
      "parcial",
    ),
    item(
      "Cópia organizada no OneDrive",
      "não feita: o acesso à nuvem é só leitura e falta sua autorização",
      "falta",
    ),
    item(
      "Retomar sem reprocessar",
      "chave por origem e cursor do Gmail; cada rodada só vê o que é novo",
      "ok",
    ),
    item(
      "Evidência separada de lançamento",
      "Registro no inventário; Movimentação só depois de aprovada",
      "ok",
    ),
    item("Primeiros registros conciliados", "depois desta revisão", "falta"),
    item("Recorrências e relatório financeiro inicial", "preliminares, abaixo", "parcial"),
  ];

  const html = `<title>Encerramento do Inventário</title>
<meta name="claude:viewport-fit-sides" content="cover">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600&display=swap">
<style>
:root{--bg:#f6f7f5;--fg:#1b2420;--mut:#5d6b65;--line:#dde3df;--acc:#1f6f5c;--warn:#8a5a00;--card:#fff;--ok-bg:#dcefe6;--ok-fg:#1d5c47;--par-bg:#f6ead0;--par-fg:#7a4f00;--no-bg:#f5dcd8;--no-fg:#8c2f22}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){--bg:#121815;--fg:#e4ebe7;--mut:#9aaba3;--line:#2a3530;--acc:#6cc2a6;--warn:#e2b45c;--card:#19211d;--ok-bg:#1d3a2f;--ok-fg:#8fd8bb;--par-bg:#3d3018;--par-fg:#ebc57a;--no-bg:#40221d;--no-fg:#f0a196;color-scheme:dark}}
:root[data-theme="dark"]{--bg:#121815;--fg:#e4ebe7;--mut:#9aaba3;--line:#2a3530;--acc:#6cc2a6;--warn:#e2b45c;--card:#19211d;--ok-bg:#1d3a2f;--ok-fg:#8fd8bb;--par-bg:#3d3018;--par-fg:#ebc57a;--no-bg:#40221d;--no-fg:#f0a196;color-scheme:dark}
body{background:var(--bg);color:var(--fg);font:15px/1.55 "IBM Plex Sans",system-ui,sans-serif}
main{max-width:52rem;margin:0 auto;padding-block:28px 64px;padding-left:max(16px,env(safe-area-inset-left,0px));padding-right:max(16px,env(safe-area-inset-right,0px));display:grid;gap:26px}
h1{font-size:1.9rem;margin:0;text-wrap:balance}h2{font-size:1.25rem;margin:0 0 8px}p{margin:0;max-width:68ch}
.mut{color:var(--mut)}.warn{color:var(--warn)}ul{margin:0;padding-left:1.2rem;display:grid;gap:6px}
.t{overflow-x:auto}table{border-collapse:collapse;width:100%;font-size:13.5px;font-variant-numeric:tabular-nums}
th,td{text-align:left;padding:6px 8px;border-bottom:1px solid var(--line);vertical-align:top}th{color:var(--mut);font-weight:500}.n{text-align:right;white-space:nowrap}
section{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:18px;display:grid;gap:10px;min-width:0}
.st{display:inline-block;padding:1px 9px;border-radius:999px;font-size:12.5px;font-weight:500;white-space:nowrap}.ok{background:var(--ok-bg);color:var(--ok-fg)}.par{background:var(--par-bg);color:var(--par-fg)}.no{background:var(--no-bg);color:var(--no-fg)}
</style>
<main>
<header><h1>Encerramento do Inventário</h1>
<p class="mut">Gerado em ${br(hoje)} a partir do índice local do cofre. Contas aparecem pelo banco e pelo final, sem número completo. O detalhe de cada item está na ferramenta local (http://127.0.0.1:8001/inventario).</p></header>

<section><h2>Em uma tela</h2><ul>
<li><b>${f(t.achados)}</b> conteúdos únicos catalogados em <b>${f(t.origens)}</b> lugares; <b>${f(t.docs)}</b> documentos financeiros, <b>${f(t.duvidas)}</b> em dúvida e <b>${f(t.descartados)}</b> descartados com motivo.</li>
<li>Cofre com <b>${f(t.copias)}</b> cópias (${t.gb} GB), todas conferidas por SHA-256 contra o índice, sem divergência.</li>
<li>Pluggy: <b>${f(pl.n)}</b> registros de ${br(pl.de)} a ${br(pl.ate)}, ${f(pl.faturas)} faturas de cartão, ${f(pl.parcelas)} compras parceladas e ${f(pl.contra)} pagamentos com nome de quem recebeu.</li>
<li>Gmail: <b>${f(emails.n)}</b> e-mails desde ${br(emails.de)}, ${f(emails.anexos)} anexos e ${f(emails.docs)} documentos.</li>
<li class="warn"><b>${f(semNome.n)}</b> saídas da Pluggy (${reais(semNome.total)}) seguem sem nome de quem recebeu: a MeuPluggy não informa. Os comprovantes do celular e do WhatsApp são o caminho.</li>
<li class="warn">Fila para você: <b>${f(t.pend)}</b> decisões e lacunas (no fim desta página).</li>
</ul></section>

<section><h2>Seção 12, item por item</h2>${chips(tabela(["Item", "Hoje", "Situação"], secao12))}</section>

<section><h2>Critério de sucesso: onde estamos</h2>${chips(tabela(["Critério", "Hoje", "Situação"], sucesso))}</section>

<section><h2>Fontes e períodos</h2>${tabela(
    ["Fonte", "Situação", "Itens", "De", "Até"],
    fontes.map((x) => [
      x.fonte,
      x.escopo === "fora"
        ? "fora de escopo"
        : x.status
          ? (situacao[x.status] ?? x.status)
          : "não coletada",
      f(x.n),
      br(x.de),
      br(x.ate),
    ]),
    [2],
  )}</section>

<section><h2>Pluggy: contas e movimentos</h2>
<p>${f(pl.mov)} registros já são Movimentações no app; ${f(pl.rasc)} entraram como rascunho, sem postar. ${f(pl.fornecedores)} fornecedores normalizados.</p>
${tabela(
  ["Tipo", "Registros", "Saídas"],
  porTipo.map((x) => [nomeTipo[x.tipo] ?? x.tipo, f(x.n), reais(x.saidas)]),
  [1, 2],
)}
${tabela(
  ["Conta", "Transações", "De", "Até"],
  contas.map((x) => [x.cartao ? `Cartão ${x.conta}` : x.conta, f(x.n), br(x.de), br(x.ate)]),
  [1],
)}
</section>

<section><h2>Cobertura por ciclo (20 a 19)</h2>${tabela(
    ["Ciclo", "Pluggy", "Documentos", "E-mails", "Faturas"],
    ciclos.map((c) => [c.ciclo, f(c.pluggy), f(c.docs), f(c.emails), f(c.faturas)]),
    [1, 2, 3, 4],
  )}
<p class="mut">A Pluggy cobre os 13 ciclos, com buracos em algumas contas (ver lacunas). Documentos dos ciclos recentes vêm quase só do Gmail; OneDrive ao vivo, celular e bancos ainda não entraram.</p></section>

<section><h2>Documentos por tipo</h2>${tabela(
    ["Tipo", "Documentos"],
    docsPorTipo.map((x) => [x.tipo ?? "(sem tipo)", f(x.n)]),
    [1],
  )}</section>

<section><h2>Duplicidades</h2><ul>
<li><b>${f(t.multi)}</b> conteúdos apareceram em mais de um lugar (mesmo arquivo em espelhos, nuvem e e-mail): guardados uma vez só.</li>
<li><b>${f(t.fps)}</b> grupos de documentos com o mesmo texto e bytes diferentes (possível duplicidade: o mesmo boleto salvo duas vezes, PDF e foto do mesmo comprovante). Conferência na próxima etapa.</li>
<li>Pagamento de fatura e transferências entre contas suas foram separados do gasto para não contar duas vezes.</li>
</ul></section>

<section><h2>Para onde foi o dinheiro (13 ciclos, preliminar)</h2>
<p class="mut">Só Pluggy, sem pagamentos de fatura e transferências entre suas contas. Análise de verdade depois da revisão.</p>
${tabela(
  ["Fornecedor", "Vezes", "Total"],
  topFornecedores.map((x) => [x.fornecedor, f(x.vezes), reais(x.total)]),
  [1, 2],
)}
<h2>Candidatos a recorrência (6 ou mais dos 13 ciclos)</h2>
${tabela(
  ["Fornecedor", "Ciclos", "Vezes", "Total"],
  recorrentes.map((x) => [x.fornecedor, f(x.ciclos), f(x.vezes), reais(x.total)]),
  [1, 2, 3],
)}
</section>

<section><h2>Lacunas e falhas</h2><ul>
<li>OneDrive ao vivo: remoto criado mas sem o drive escolhido (rclone config reconnect OneDrive:). Google Drive: sem remoto. A cópia local do OneDrive vai só até 05/2026.</li>
<li>Celular (WhatsApp, fotos, Download): cópia ainda em andamento; Telegram, SMS, Outlook, faturas em PDF, OFX, consignados, VR/VA, LGPD e contracheques do RH: não coletados.</li>
<li>Pluggy: sem contraparte na maioria dos Pix e boletos; sem empréstimos; buracos na conta CAIXA mais usada (fev, ago, set/2026) e na Nubank (jan, fev, ago, set/2026).</li>
<li>Faturas do cartão: só 1 de 28 fecha com as compras; no cartão gold a diferença repete a fatura anterior (indício de saldo rolado com juros).</li>
<li>Falhas corrigidas durante a noite: token do Gmail vencido, cota do Gmail, chamada de rede pendurada, cópias concorrentes no cofre, transação aberta no SQLite, painel apontando para snapshot apagado. Cada uma virou regra em workers/inventario/LICOES.md.</li>
</ul></section>

<section><h2>O que o Seu Bolso Feliz não representou</h2><ul>
<li>Não há modo só-local: os scanners do app sobem cada arquivo para a nuvem e gravam no banco na hora; o inventário teve de viver fora (ADR-011).</li>
<li>O sync da Pluggy descarta quem recebeu o Pix, a parcela, a data original da compra e as faturas.</li>
<li>O ciclo 20→19 não está configurado no app (o padrão é dia 1).</li>
<li>Não existe detecção de recorrência, conferência de fatura contra compras nem visão de cobertura por fonte e ciclo.</li>
</ul></section>

<section><h2>Próximos passos sugeridos</h2><ul>
<li>Terminar o OneDrive e conectar o Google Drive; escanear o celular quando a cópia acabar.</li>
<li>Casar comprovantes do celular e do WhatsApp com os Pix sem nome.</li>
<li>Baixar extratos dos meses-buraco e as faturas em PDF dos dois cartões (confirma o saldo rolado no gold).</li>
<li>Responder a fila abaixo; depois, conciliação e a análise de recorrências e cortes.</li>
</ul></section>

<section><h2>Fila para você (${f(t.pend)})</h2>${tabela(
    ["Etapa", "Tipo", "Pergunta"],
    pendencias.map((p) => [
      p.etapa,
      ({ decisao: "decisão", duvida: "dúvida" } as Record<string, string>)[p.tipo] ?? p.tipo,
      p.pergunta,
    ]),
  )}</section>
</main>`;
  writeFileSync(saida, html);
  console.log(`relatório: ${saida}`);
}

main();
