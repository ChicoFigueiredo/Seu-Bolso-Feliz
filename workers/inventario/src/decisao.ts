/**
 * A decisão do usuário prevalece (spec 06, "O banco"): linha com
 * decidido_por='usuario' (gravado pelo desktop) não é sobrescrita pelos scripts.
 */
import type { Database } from "bun:sqlite";
import { temColuna } from "./banco";

/**
 * Condição SQL "esta linha tem decisão do usuário". Em banco ainda não migrado
 * pelo Rust (sem a coluna) vale `0`, e o script se comporta como antes.
 * `prefixo` qualifica a coluna (ex.: "registro." dentro de ON CONFLICT).
 */
export function condicaoDoUsuario(
  db: Database,
  tabela: "achado" | "registro",
  prefixo = "",
): string {
  return temColuna(db, tabela, "decidido_por") ? `(${prefixo}decidido_por = 'usuario')` : "0";
}
