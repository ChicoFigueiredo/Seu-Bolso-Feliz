/**
 * Cofre de segredos do inventário (ADR-011): um arquivo cifrado com o CPF e as
 * senhas de PDF que não derivam dele. A chave sai de uma senha-mestra via
 * scrypt; o conteúdo é AES-256-GCM. Nada aqui imprime, loga ou devolve um
 * segredo em texto — o SQLite guarda só a estratégia por fornecedor.
 *
 * Uso, num terminal interativo (o segredo nunca vem por argumento nem pipe):
 *   bun run workers/inventario/src/segredos.ts criar
 *   bun run workers/inventario/src/segredos.ts verificar
 *   bun run workers/inventario/src/segredos.ts adicionar <rotulo>
 */
import { createCipheriv, createDecipheriv, randomBytes, scryptSync } from "node:crypto";
import { existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { caminhosDoCofre } from "./cofre";
import { cpfDigitos, cpfValido } from "./cpf";

export interface Segredos {
  cpf: string;
  /** rótulo → senha, para PDFs cuja senha não deriva do CPF. */
  senhas: Record<string, string>;
}

interface ArquivoCifrado {
  versao: 1;
  kdf: { alg: "scrypt"; N: number; r: number; p: number; sal: string };
  iv: string;
  tag: string;
  dados: string;
}

const AAD = Buffer.from("sbf-inventario-segredos-v1");
const SCRYPT = { N: 2 ** 17, r: 8, p: 1 };
const TAMANHO_MINIMO_SENHA_MESTRA = 12;

function derivarChave(senhaMestra: string, sal: Buffer, kdf = SCRYPT): Buffer {
  return scryptSync(senhaMestra, sal, 32, { ...kdf, maxmem: 256 * 1024 * 1024 });
}

export function cifrar(segredos: Segredos, senhaMestra: string): ArquivoCifrado {
  const sal = randomBytes(16);
  const iv = randomBytes(12);
  const cifra = createCipheriv("aes-256-gcm", derivarChave(senhaMestra, sal), iv);
  cifra.setAAD(AAD);
  const dados = Buffer.concat([cifra.update(JSON.stringify(segredos), "utf8"), cifra.final()]);
  return {
    versao: 1,
    kdf: { alg: "scrypt", ...SCRYPT, sal: sal.toString("base64") },
    iv: iv.toString("base64"),
    tag: cifra.getAuthTag().toString("base64"),
    dados: dados.toString("base64"),
  };
}

export function decifrar(arquivo: ArquivoCifrado, senhaMestra: string): Segredos {
  const { N, r, p, sal } = arquivo.kdf;
  const chave = derivarChave(senhaMestra, Buffer.from(sal, "base64"), { N, r, p });
  const decifra = createDecipheriv("aes-256-gcm", chave, Buffer.from(arquivo.iv, "base64"));
  decifra.setAAD(AAD);
  decifra.setAuthTag(Buffer.from(arquivo.tag, "base64"));
  try {
    const claro = Buffer.concat([
      decifra.update(Buffer.from(arquivo.dados, "base64")),
      decifra.final(),
    ]);
    return JSON.parse(claro.toString("utf8")) as Segredos;
  } catch {
    // GCM falha igual para senha errada e arquivo adulterado; não distinguir
    // é proposital.
    throw new Error("senha-mestra errada ou cofre de segredos corrompido");
  }
}

function caminhoDoArquivo(): string {
  return join(caminhosDoCofre().segredos, "cofre.enc");
}

export function abrirSegredos(senhaMestra: string): Segredos {
  const arquivo = JSON.parse(readFileSync(caminhoDoArquivo(), "utf8")) as ArquivoCifrado;
  return decifrar(arquivo, senhaMestra);
}

function gravar(segredos: Segredos, senhaMestra: string): void {
  const destino = caminhoDoArquivo();
  const temporario = `${destino}.novo`;
  writeFileSync(temporario, JSON.stringify(cifrar(segredos, senhaMestra)), { mode: 0o600 });
  renameSync(temporario, destino);
}

async function lerOculto(pergunta: string): Promise<string> {
  const stdin = process.stdin;
  if (!stdin.isTTY) {
    throw new Error("rode num terminal interativo: segredo não entra por pipe nem argumento");
  }
  process.stdout.write(pergunta);
  stdin.setRawMode(true);
  stdin.resume();
  stdin.setEncoding("utf8");
  return new Promise((resolve, reject) => {
    let valor = "";
    const encerrar = () => {
      stdin.setRawMode(false);
      stdin.pause();
      stdin.off("data", aoDigitar);
      process.stdout.write("\n");
    };
    const aoDigitar = (pedaco: string) => {
      for (const ch of pedaco) {
        if (ch === "\r" || ch === "\n") {
          encerrar();
          resolve(valor);
          return;
        }
        if (ch === "\u0003") {
          encerrar();
          reject(new Error("cancelado"));
          return;
        }
        if (ch === "\u007f" || ch === "\b") valor = valor.slice(0, -1);
        else valor += ch;
      }
    };
    stdin.on("data", aoDigitar);
  });
}

async function novaSenhaMestra(): Promise<string> {
  for (;;) {
    const senha = await lerOculto(
      `Senha-mestra (mínimo ${TAMANHO_MINIMO_SENHA_MESTRA} caracteres): `,
    );
    if (senha.length < TAMANHO_MINIMO_SENHA_MESTRA) {
      console.log("Curta demais.");
      continue;
    }
    if ((await lerOculto("Repita a senha-mestra: ")) === senha) return senha;
    console.log("Não confere. De novo.");
  }
}

async function lerCpf(): Promise<string> {
  for (;;) {
    const cpf = cpfDigitos(await lerOculto("CPF (não aparece na tela): "));
    if (cpfValido(cpf)) return cpf;
    console.log("CPF inválido (dígito verificador não confere). De novo.");
  }
}

async function criar(substituir: boolean): Promise<void> {
  if (existsSync(caminhoDoArquivo()) && !substituir) {
    throw new Error("o cofre de segredos já existe; use `verificar`, ou `criar --substituir`");
  }
  const senhaMestra = await novaSenhaMestra();
  gravar({ cpf: await lerCpf(), senhas: {} }, senhaMestra);
  console.log(`Cofre de segredos criado em ${caminhoDoArquivo()} (permissão 600).`);
  console.log("Esqueceu a senha-mestra, perdeu o cofre: ela não é recuperável.");
}

async function verificar(): Promise<void> {
  const segredos = abrirSegredos(await lerOculto("Senha-mestra: "));
  const rotulos = Object.keys(segredos.senhas);
  console.log(`OK. CPF ${cpfValido(segredos.cpf) ? "presente e válido" : "INVÁLIDO"}.`);
  console.log(
    `${rotulos.length} senha(s) própria(s)${rotulos.length ? `: ${rotulos.join(", ")}` : ""}.`,
  );
}

async function adicionar(rotulo: string | undefined): Promise<void> {
  if (!rotulo || !/^[a-z0-9][a-z0-9_.-]*$/.test(rotulo)) {
    throw new Error("rótulo obrigatório, em minúsculas: ex. condominio-qnl, fatura-riachuelo");
  }
  const senhaMestra = await lerOculto("Senha-mestra: ");
  const segredos = abrirSegredos(senhaMestra);
  segredos.senhas[rotulo] = await lerOculto(`Senha para "${rotulo}" (não aparece na tela): `);
  gravar(segredos, senhaMestra);
  console.log(`Senha "${rotulo}" guardada.`);
}

if (import.meta.main) {
  const [comando, arg] = process.argv.slice(2);
  const acoes: Record<string, () => Promise<void>> = {
    criar: () => criar(arg === "--substituir"),
    verificar,
    adicionar: () => adicionar(arg),
  };
  const acao = comando ? acoes[comando] : undefined;
  if (!acao) {
    console.error("uso: segredos.ts criar [--substituir] | verificar | adicionar <rotulo>");
    process.exit(2);
  }
  acao().catch((erro: unknown) => {
    console.error(erro instanceof Error ? erro.message : String(erro));
    process.exit(1);
  });
}
