/**
 * Hash e verificação de senha.
 *
 * Usa `scrypt` da biblioteca padrão do Node em vez de argon2/bcrypt. Os dois
 * são bons, mas ambos são módulos nativos — e módulo nativo em runtime
 * serverless é a fonte de metade dos deploys quebrados que existem. `scrypt` é
 * memory-hard, foi desenhado exatamente para isto, e não tem dependência
 * nenhuma. Para um sistema com dois usuários, é a escolha certa.
 *
 * O formato armazenado é `scrypt$N$r$p$salt$hash`, tudo em base64url. Guardar
 * os parâmetros junto do hash é o que permite endurecê-los no futuro sem
 * invalidar as senhas existentes.
 */
import { randomBytes, scrypt as scryptCb, timingSafeEqual } from "node:crypto";
import { promisify } from "node:util";

const scrypt = promisify(scryptCb) as (
  senha: string | Buffer,
  sal: string | Buffer,
  tamanho: number,
  opcoes: { N: number; r: number; p: number; maxmem: number },
) => Promise<Buffer>;

const N = 2 ** 16; // custo de CPU/memória: ~64 MiB por verificação
const R = 8;
const P = 1;
const TAMANHO_HASH = 32;
const TAMANHO_SAL = 16;
// O default do Node (32 MiB) é menor que o que N=2^16 exige; sem isto, o
// scrypt falha com "memory limit exceeded" em vez de calcular o hash.
const MAXMEM = 128 * N * R * 2;

const b64 = (b: Buffer) => b.toString("base64url");

export async function hashSenha(senha: string): Promise<string> {
  const sal = randomBytes(TAMANHO_SAL);
  const hash = await scrypt(senha.normalize("NFKC"), sal, TAMANHO_HASH, {
    N,
    r: R,
    p: P,
    maxmem: MAXMEM,
  });
  return `scrypt$${N}$${R}$${P}$${b64(sal)}$${b64(hash)}`;
}

/**
 * Comparação em tempo constante. Nunca lança: senha inválida e hash corrompido
 * devolvem `false` igualmente, porque a diferença entre os dois não interessa a
 * quem está tentando entrar — e vazá-la conta algo a quem está atacando.
 */
export async function verificaSenha(senha: string, armazenado: string | null): Promise<boolean> {
  if (!armazenado) return false;
  try {
    const partes = armazenado.split("$");
    if (partes.length !== 6 || partes[0] !== "scrypt") return false;
    const [, nTxt, rTxt, pTxt, salB64, hashB64] = partes as [
      string,
      string,
      string,
      string,
      string,
      string,
    ];
    const n = Number(nTxt);
    const r = Number(rTxt);
    const p = Number(pTxt);
    if (!Number.isSafeInteger(n) || !Number.isSafeInteger(r) || !Number.isSafeInteger(p)) {
      return false;
    }
    const esperado = Buffer.from(hashB64, "base64url");
    const calculado = await scrypt(
      senha.normalize("NFKC"),
      Buffer.from(salB64, "base64url"),
      esperado.length,
      {
        N: n,
        r,
        p,
        maxmem: 128 * n * r * 2,
      },
    );
    return calculado.length === esperado.length && timingSafeEqual(calculado, esperado);
  } catch {
    return false;
  }
}
