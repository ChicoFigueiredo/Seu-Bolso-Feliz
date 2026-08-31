/**
 * @sbf/mcp-server — identidade fixa do usuário local.
 *
 * O MCP server roda como processo stdio local, sem sessão: não há login, não
 * há JWT, e não há mais RLS do banco segurando o isolamento por
 * usuário. Toda operação age em nome de UM usuário só, vindo de env — quem
 * consegue rodar o processo já é, por definição, quem tem acesso à máquina.
 * Cada ferramenta em `tools/` continua filtrando por este `userId`
 * explicitamente em toda query, exatamente como o RLS fazia antes.
 */
export function getUserId(): string {
  const uid = process.env.LOCAL_USER_ID;
  if (!uid) throw new Error("LOCAL_USER_ID is required for MCP server");
  return uid;
}
