/**
 * Onde o cofre mora (ADR-011): fora do repositório, em /mnt/d/Chico/financas
 * por padrão, sobrescrevível por FINANCAS_DIR. Nenhum caminho aqui aponta
 * para dentro do repo.
 */
import { join } from "node:path";

export interface CaminhosDoCofre {
  raiz: string;
  banco: string;
  /** Uma cópia por conteúdo único, nomeada pelo hash. */
  acervo: string;
  /** Vista por ciclo, gerada a partir do SQLite (hardlinks para o acervo). */
  ciclos: string;
  /** Documentos sem data-chave ainda. */
  aClassificar: string;
  /** Material trazido de fora (rclone, celular, exportações) antes do scan. */
  entrada: string;
  segredos: string;
  logs: string;
  /** Cópias decifradas temporárias; apagadas depois da extração. */
  tmp: string;
}

export const RAIZ_PADRAO = "/mnt/d/Chico/financas";

export function caminhosDoCofre(raiz = process.env.FINANCAS_DIR ?? RAIZ_PADRAO): CaminhosDoCofre {
  return {
    raiz,
    banco: join(raiz, "inventario.sqlite"),
    acervo: join(raiz, "acervo"),
    ciclos: join(raiz, "ciclos"),
    aClassificar: join(raiz, "a-classificar"),
    entrada: join(raiz, "entrada"),
    segredos: join(raiz, "segredos"),
    logs: join(raiz, "logs"),
    tmp: join(raiz, "tmp"),
  };
}

/** Caminho no acervo para um conteúdo: acervo/ab/ab12…f9.pdf */
export function caminhoNoAcervo(contentHash: string, extensao: string): string {
  const ext = extensao.replace(/^\./, "").toLowerCase();
  return join("acervo", contentHash.slice(0, 2), ext ? `${contentHash}.${ext}` : contentHash);
}
