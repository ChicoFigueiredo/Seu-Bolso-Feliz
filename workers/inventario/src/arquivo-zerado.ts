/**
 * Arquivo cujo conteúdo é todo zero. É o que sobra no disco quando a pasta do
 * OneDrive guarda só o marcador do arquivo ("disponível na nuvem"): o tamanho
 * é o real, os bytes não. O conteúdo de verdade só existe na nuvem.
 *
 * Sem banco nem Bun, para o vitest. Lê em blocos e para no primeiro byte não
 * zero, então arquivo normal custa uma leitura curta.
 */
import { open } from "node:fs/promises";

const BLOCO = 64 * 1024;

export const NOTA_ZERADO = "conteúdo todo zero: marcador de nuvem, o arquivo real está na nuvem";

/** true só para arquivo com pelo menos 1 byte e todos os bytes zero. */
export async function ehSoZeros(caminho: string): Promise<boolean> {
  const arq = await open(caminho, "r");
  try {
    const buf = Buffer.alloc(BLOCO);
    let total = 0;
    for (;;) {
      const { bytesRead } = await arq.read(buf, 0, BLOCO, null);
      if (bytesRead === 0) return total > 0;
      for (let i = 0; i < bytesRead; i++) if (buf[i] !== 0) return false;
      total += bytesRead;
    }
  } finally {
    await arq.close();
  }
}
