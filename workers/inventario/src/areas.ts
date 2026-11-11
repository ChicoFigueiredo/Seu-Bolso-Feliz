/**
 * E1: em que camada cai cada pasta dos discos, e o que fazer com cada arquivo.
 * Regras confirmadas na abertura da E1 (2026-11-11).
 *
 *   A — áreas pessoais: hash de documentos e fotos; documentos copiados ao cofre.
 *   B — o resto: hash de documentos; fotos só contadas por pasta.
 *   C — fora de escopo, com motivo: só contagem. Um arquivo com nome financeiro
 *       (comprovante, fatura, contracheque…) é resgatado para B mesmo aqui.
 *
 * Precedência para uma pasta: regra pelo caminho exato > regra pelo nome da
 * pasta > a classe da pasta-mãe. Regra de nome A não tira ninguém de C.
 */
import { basename, extname } from "node:path";

export type Camada = "A" | "B" | "C";

export interface Regra {
  camada: Camada;
  motivo?: string;
  /** Não entra nem para contar (o próprio cofre). */
  ignorar?: boolean;
}

export interface Classe extends Regra {
  /** Pasta onde a classe foi decidida (para agregar a contagem de C). */
  raiz: string;
}

const A: Regra = { camada: "A" };

function c(motivo: string): Regra {
  return { camada: "C", motivo };
}

export const REGRAS_POR_CAMINHO: ReadonlyMap<string, Regra> = new Map<string, Regra>([
  // O próprio cofre e o repositório nunca são varridos como fonte.
  ["/mnt/d/Chico/financas", { camada: "C", motivo: "cofre", ignorar: true }],

  // A — áreas pessoais
  ["/mnt/e2/Pessoal/Chico", A],
  ["/mnt/e2/Pessoal/Pessoal", A],
  ["/mnt/p/OneDrive/Pessoal", A],
  ["/mnt/p/OneDrive/Documentos", A],
  ["/mnt/p/OneDrive/Mayara", A],
  ["/mnt/p2/$Nuvem/OneDrive", A],
  ["/mnt/p2/$Nuvem/GoogleDrive", A],
  ["/mnt/p2/$Nuvem/MegaSync", A],
  ["/mnt/p2/$Nuvem/DropBox", A],
  ["/mnt/p2/$Nuvem/Sync", A],
  ["/mnt/bak/$Nuvem/OneDrive", A],
  ["/mnt/bak/$Nuvem/GoogleDrive", A],
  ["/mnt/bak/$Nuvem/MegaSync", A],
  ["/mnt/bak/$Nuvem/DropBox", A],
  ["/mnt/bak/$Nuvem/Sync", A],
  ["/mnt/bak/Pessoal", A],
  ["/mnt/p2/zzz_delete_Pessoal", A],
  ["/home/chico/Downloads", A],
  ["/mnt/p2/Downloads", A],
  ["/mnt/e2/Downloads", A],

  // C — livros
  ["/mnt/e/Livros", c("livros")],
  ["/mnt/e2/Livros", c("livros")],
  ["/mnt/e2/Livros_Catalogação", c("livros")],
  ["/mnt/e2/Calibre-Bak", c("livros")],
  ["/mnt/bak/Livros", c("livros")],
  ["/mnt/bak/Livros_Catalogação", c("livros")],
  ["/mnt/bak/Calibre-Bak", c("livros")],
  ["/mnt/bak/Calibre-Bak-ANTIGO", c("livros")],
  ["/mnt/bak/AudioLivros", c("livros")],
  ["/mnt/p2/zzz_delete_Livros", c("livros")],
  ["/mnt/p/OneDrive/Livros", c("livros")],

  // C — estudo (o disco Estudo inteiro, menos as pastas de triagem)
  ["/mnt/e", c("estudo")],
  ["/mnt/e/_classifica", { camada: "B" }],
  ["/mnt/e/TEMP", { camada: "B" }],
  ["/mnt/e2/Estudar", c("estudo")],
  ["/mnt/p2/Cursos", c("estudo")],
  ["/mnt/bak/Cursos", c("estudo")],
  ["/mnt/p/Estudos", c("estudo")],
  ["/mnt/p/OneDrive/Cursos", c("estudo")],
  ["/mnt/p/OneDrive/Estudar", c("estudo")],

  // C — jogos
  ["/mnt/p2/Jogos", c("jogos")],
  ["/mnt/p2/SteamLibrary", c("jogos")],
  ["/mnt/p2/Emuladores", c("jogos")],
  ["/mnt/bak/Emuladores", c("jogos")],
  ["/mnt/p/PS5", c("jogos")],

  // C — código e máquinas virtuais
  ["/mnt/d/Chico", c("codigo")],
  ["/mnt/d/dev", c("codigo")],
  ["/mnt/d/ia", c("codigo")],
  ["/mnt/d/open-design", c("codigo")],
  ["/mnt/p/dev", c("codigo")],
  ["/mnt/p2/Dev", c("codigo")],
  ["/mnt/bak/Dev", c("codigo")],
  ["/mnt/p/OpenSource", c("codigo")],
  ["/mnt/p/ComfyUI", c("codigo")],
  ["/mnt/p/ComfyUI-audio", c("codigo")],
  ["/mnt/p/ai-toolkit", c("codigo")],
  ["/mnt/p/huggingface", c("codigo")],
  ["/mnt/p/IA", c("codigo")],
  ["/mnt/p/worker-siric", c("codigo")],
  ["/mnt/p2/LINUX", c("codigo")],
  ["/mnt/p/VirtualBox", c("codigo")],
  ["/mnt/e2/VHDX_EXT4_WSL", c("codigo")],
  ["/home/chico/snap", c("codigo")],

  // C — mídia
  ["/mnt/p2/Musica", c("midia")],
  ["/mnt/e2/Musica", c("midia")],
  ["/mnt/bak/Musica", c("midia")],
  ["/mnt/e2/Video", c("midia")],
  ["/mnt/bak/Video", c("midia")],
  ["/mnt/bak/Podcast", c("midia")],
  ["/mnt/p/OneDrive/Music", c("midia")],
  ["/mnt/p/OneDrive/Videos", c("midia")],

  // C — trabalho (contracheque e informe voltam pelo resgate de nome)
  ["/mnt/p/CAIXA", c("trabalho")],
  ["/mnt/d/CAIXA", c("trabalho")],
  ["/mnt/bak/CAIXA", c("trabalho")],
  ["/mnt/bak/Trabalho.CAIXA", c("trabalho")],
  ["/mnt/p2/$Nuvem/CAIXA", c("trabalho")],
  ["/mnt/p/OneDrive/CAIXA", c("trabalho")],
  ["/mnt/p/OneDrive/Trabalho", c("trabalho")],

  // C — dados de outra pessoa
  ["/mnt/p/Daiane", c("outra-pessoa")],
  ["/mnt/e2/Pessoal/Daiane", c("outra-pessoa")],
  ["/mnt/p2/$Nuvem/OneDrive.Dai", c("outra-pessoa")],
  ["/mnt/bak/$Nuvem/OneDrive.Dai", c("outra-pessoa")],
  ["/home/pipoka", c("outra-pessoa")],

  // C — sistema
  ["/mnt/p2/Program Files (x86)", c("sistema")],
  ["/mnt/p/OneDriveTemp", c("sistema")],
  ["/mnt/p2/OneDriveTemp", c("sistema")],
  ["/mnt/p/OneDrive/AppData", c("sistema")],
  ["/mnt/bak/Install", c("sistema")],
  ["/mnt/bak/Portable", c("sistema")],
  ["/mnt/p/_backup_credenciais", c("credenciais")],
]);

export const REGRAS_POR_NOME: ReadonlyMap<string, Regra> = new Map<string, Regra>([
  ["Mayara", A],
  ["Daiane", c("outra-pessoa")],
  ["OneDrive.Dai", c("outra-pessoa")],
  ...[
    "node_modules",
    ".git",
    ".svn",
    ".next",
    ".cache",
    "__pycache__",
    ".venv",
    "venv",
    "site-packages",
    "AppData",
    "Program Files",
    "Program Files (x86)",
    "Windows",
    "$RECYCLE.BIN",
    "System Volume Information",
    ".Trash-1000",
    ".npm",
    ".bun",
    ".cargo",
    ".rustup",
    ".gradle",
    ".m2",
    ".vscode-server",
    "steamapps",
  ].map((nome): [string, Regra] => [nome, c("sistema")]),
]);

const DENTRO_DE_IR = /\/(ir|irpf\d*|impostorenda|imposto[ _.-]de[ _.-]renda)\//i;

/** Classe de uma pasta, dada a da pasta-mãe (undefined na raiz do disco). */
export function classificarPasta(
  caminho: string,
  mae: Classe | undefined,
  porCaminho = REGRAS_POR_CAMINHO,
  porNome = REGRAS_POR_NOME,
): Classe {
  const exata = porCaminho.get(caminho);
  if (exata) return { ...exata, raiz: caminho };
  const pelaPasta = porNome.get(basename(caminho));
  // Comprovantes de dependente dentro do SEU imposto de renda são seus
  // (decisão da abertura da E2): a pasta herda a classe da mãe.
  if (pelaPasta?.motivo === "outra-pessoa" && mae?.camada === "A" && DENTRO_DE_IR.test(caminho)) {
    return mae;
  }
  if (pelaPasta && (pelaPasta.camada === "C" || mae?.camada !== "C")) {
    return { ...pelaPasta, raiz: caminho };
  }
  return mae ?? { camada: "B", raiz: caminho };
}

export const DOCUMENTOS = new Set([
  "pdf",
  "ofx",
  "ofc",
  "qif",
  "xls",
  "xlsx",
  "xlsm",
  "csv",
  "ods",
  "xml",
  "eml",
  "msg",
  "mbox",
  "docx",
  "doc",
  "odt",
  "rtf",
  "zip",
  "rar",
  "7z",
]);
/** Texto solto só interessa em área pessoal (exportações, notas, SMS). */
export const TEXTOS_PESSOAIS = new Set(["txt", "html", "htm", "json"]);
export const FOTOS = new Set([
  "jpg",
  "jpeg",
  "png",
  "heic",
  "heif",
  "webp",
  "gif",
  "bmp",
  "tif",
  "tiff",
]);

const NOME_FINANCEIRO =
  /(comprovante|fatura|extrato|boleto|recibo|nota[ _.-]?fiscal|\bnfs?-?e\b|danfe|contra[ _.-]?cheque|holerite|irpf|informes?[ _.-]*(de[ _.-]*)?rendimento|darf|iptu|ipva|condominio|pensao|consignad|emprestimo|financiamento|\bpix\b)/;

function semAcento(texto: string): string {
  return texto
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();
}

export function nomeFinanceiro(nomeDoArquivo: string): boolean {
  return NOME_FINANCEIRO.test(semAcento(nomeDoArquivo));
}

export function extensaoDe(nomeDoArquivo: string): string {
  return extname(nomeDoArquivo).slice(1).toLowerCase();
}

export type Tratamento =
  /** hash e cópia ao cofre */
  | "copiar"
  /** só hash; a cópia espera a E3 confirmar que é documento */
  | "hash"
  /** foto fora de área pessoal: contada por pasta */
  | "foto-contada"
  /** em área C: contado no agregado da área */
  | "contar"
  /** não é candidato (executável, áudio…): só entra no resumo da varredura */
  | "ignorar";

export function tratamentoDoArquivo(nome: string, classe: Classe): Tratamento {
  const ext = extensaoDe(nome);
  const candidato = DOCUMENTOS.has(ext) || FOTOS.has(ext) || TEXTOS_PESSOAIS.has(ext);
  if (classe.camada === "C") return candidato && nomeFinanceiro(nome) ? "hash" : "contar";
  if (classe.camada === "A") {
    if (DOCUMENTOS.has(ext) || TEXTOS_PESSOAIS.has(ext)) return "copiar";
    return FOTOS.has(ext) ? "hash" : "ignorar";
  }
  if (DOCUMENTOS.has(ext)) return "hash";
  if (candidato && nomeFinanceiro(nome)) return "hash";
  return FOTOS.has(ext) ? "foto-contada" : "ignorar";
}
