"use server";

import { requireUser } from "@/lib/auth";
import { PluggyProvider } from "@sbf/financial-connectors/pluggy";
import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq } from "drizzle-orm";

export type ProviderConnection = typeof schema.provider_connections.$inferSelect;
export type ExternalAccountMapping = typeof schema.external_account_mappings.$inferSelect;
type ExternalAccountMappingUpdate = Partial<
  Omit<typeof schema.external_account_mappings.$inferInsert, "user_id" | "id">
>;

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

export async function getProviderConnections(): Promise<ProviderConnection[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.provider_connections)
    .where(eq(schema.provider_connections.user_id, userId))
    .orderBy(desc(schema.provider_connections.created_at));
}

export async function getExternalAccountMappings(
  providerConnectionId: string,
): Promise<ExternalAccountMapping[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.external_account_mappings)
    .where(
      and(
        eq(schema.external_account_mappings.provider_connection_id, providerConnectionId),
        eq(schema.external_account_mappings.user_id, userId),
      ),
    )
    .orderBy(asc(schema.external_account_mappings.created_at));
}

/**
 * Token de vida curta (30min) pro widget Pluggy Connect no navegador, mais a
 * lista de conectores que o widget deve oferecer.
 *
 * A lista vem junto de propósito. Sem ela o widget mostra também os conectores
 * antigos, que pedem a senha do internet banking — e para vários bancos os
 * dois aparecem lado a lado com nomes que só diferem por um acento ("Caixa
 * Economica Federal" pede senha; "Caixa Econômica Federal" não). Escolher
 * errado ali entrega a senha do banco a um widget de terceiro, que é
 * exatamente o que o Open Finance existe para evitar.
 *
 * Se a consulta de conectores falhar, o token ainda vale e a lista volta
 * vazia: melhor abrir o widget sem filtro do que não abrir.
 */
export async function createPluggyConnectToken(options?: {
  itemId?: string;
  /** `true` devolve todos os conectores, inclusive os que pedem senha. */
  incluirConectoresComSenha?: boolean;
}): Promise<{ connectToken: string; connectorIds: number[] }> {
  const userId = await requireUserId();
  const provider = PluggyProvider.fromEnv();
  const { connectToken } = await provider.createConnectToken({
    itemId: options?.itemId,
    clientUserId: userId,
  });

  if (options?.incluirConectoresComSenha) return { connectToken, connectorIds: [] };

  let connectorIds: number[] = [];
  try {
    connectorIds = await provider.listOpenFinanceConnectorIds();
  } catch (erro) {
    console.error("[pluggy] não consegui listar os conectores para filtrar o widget:", erro);
  }
  return { connectToken, connectorIds };
}

/**
 * Chamada pelo frontend depois que o widget Pluggy Connect termina e devolve
 * um `itemId`. Persiste a conexão e cria (sem sobrescrever mapeamentos já
 * feitos pelo usuário) um `external_account_mappings` "pending_mapping" pra
 * cada conta que a Pluggy retornar.
 */
export async function completePluggyConnection(
  externalItemId: string,
): Promise<ProviderConnection> {
  const userId = await requireUserId();
  const db = getDb();

  const provider = PluggyProvider.fromEnv();
  const connection = await provider.getConnection(externalItemId);
  const accounts = await provider.listAccounts(externalItemId);

  const [row] = await db
    .insert(schema.provider_connections)
    .values({
      user_id: userId,
      provider: "pluggy",
      external_item_id: externalItemId,
      institution_name: connection.institutionName ?? null,
      status: "active",
    })
    .onConflictDoUpdate({
      target: [
        schema.provider_connections.user_id,
        schema.provider_connections.provider,
        schema.provider_connections.external_item_id,
      ],
      set: { institution_name: connection.institutionName ?? null, status: "active" },
    })
    .returning();
  if (!row) throw new Error("Falha ao salvar conexão do provedor");

  for (const account of accounts) {
    await db
      .insert(schema.external_account_mappings)
      .values({
        user_id: userId,
        provider_connection_id: row.id,
        external_account_id: account.externalAccountId,
        external_account_name: account.name,
        external_account_type: account.type,
      })
      .onConflictDoNothing({
        target: [
          schema.external_account_mappings.provider_connection_id,
          schema.external_account_mappings.external_account_id,
        ],
      });
  }

  return row;
}

export async function updateExternalAccountMapping(
  id: string,
  input: ExternalAccountMappingUpdate,
): Promise<ExternalAccountMapping> {
  const userId = await requireUserId();
  const db = getDb();

  // IDOR (achado da varredura retroativa de segurança, mesmo padrão do fix
  // em document-transactions.ts): o `and(id, user_id)` abaixo protege a
  // própria linha, mas não impede o `.set(input)` de reapontar
  // financial_product_id/provider_connection_id pra um recurso de outro
  // usuário. Valida posse das duas FKs antes de gravar, quando presentes no
  // input.
  if (input.financial_product_id != null) {
    const [product] = await db
      .select({ id: schema.financial_products.id })
      .from(schema.financial_products)
      .where(
        and(
          eq(schema.financial_products.id, input.financial_product_id),
          eq(schema.financial_products.user_id, userId),
        ),
      );
    if (!product) throw new Error("Produto financeiro não encontrado");
  }
  if (input.provider_connection_id != null) {
    const [connection] = await db
      .select({ id: schema.provider_connections.id })
      .from(schema.provider_connections)
      .where(
        and(
          eq(schema.provider_connections.id, input.provider_connection_id),
          eq(schema.provider_connections.user_id, userId),
        ),
      );
    if (!connection) throw new Error("Conexão do provedor não encontrada");
  }

  const [row] = await db
    .update(schema.external_account_mappings)
    .set(input)
    .where(
      and(
        eq(schema.external_account_mappings.id, id),
        eq(schema.external_account_mappings.user_id, userId),
      ),
    )
    .returning();
  if (!row) throw new Error("Vínculo de conta externa não encontrado");
  return row;
}

export async function revokeProviderConnection(id: string): Promise<void> {
  // Divergência deliberada do brief original (que preservava o silêncio do
  // driver anterior em 0 linhas afetadas): por consistência com a decisão já
  // ratificada em institutions.ts (Task 2, deleteInstitution) — lançar erro é
  // mais seguro que falhar em silêncio numa ação que não pertence ao usuário,
  // e hoje não há call site que dependa do silêncio antigo (a UI de conexões
  // ainda está fora do escopo desta fase).
  const userId = await requireUserId();
  const db = getDb();
  const result = await db
    .update(schema.provider_connections)
    .set({ status: "revoked" })
    .where(
      and(eq(schema.provider_connections.id, id), eq(schema.provider_connections.user_id, userId)),
    )
    .returning({ id: schema.provider_connections.id });
  if (result.length === 0) throw new Error("Conexão não encontrada");
}

/**
 * Cria a conta interna a partir de uma conta que o banco trouxe, e já vincula
 * as duas.
 *
 * Existe porque o vínculo, sozinho, é uma armadilha: o `<select>` só oferece
 * contas que já existem aqui dentro, e quem acabou de conectar o primeiro
 * banco não tem nenhuma — então a única opção real era "não importar". Pior em
 * escala: a Caixa devolveu 31 contas de uma vez, todas com o mesmo nome.
 * Cadastrar 31 à mão, para só então poder escolher, não é um fluxo, é uma
 * punição.
 */
export async function criarProdutoDaContaExterna(
  mappingId: string,
): Promise<{ financialProductId: string }> {
  const userId = await requireUserId();
  const db = getDb();

  const [conta] = await db
    .select({
      id: schema.external_account_mappings.id,
      nome: schema.external_account_mappings.external_account_name,
      tipo: schema.external_account_mappings.external_account_type,
      jaVinculada: schema.external_account_mappings.financial_product_id,
      instituicao: schema.provider_connections.institution_name,
    })
    .from(schema.external_account_mappings)
    .innerJoin(
      schema.provider_connections,
      and(
        eq(schema.provider_connections.id, schema.external_account_mappings.provider_connection_id),
        eq(schema.provider_connections.user_id, userId),
      ),
    )
    .where(
      and(
        eq(schema.external_account_mappings.id, mappingId),
        eq(schema.external_account_mappings.user_id, userId),
      ),
    );

  if (!conta) throw new Error("conta externa não encontrada");
  if (conta.jaVinculada) return { financialProductId: conta.jaVinculada };

  // `institution_id` é NOT NULL, então a instituição vem antes do produto.
  // Uma por nome, reaproveitada nas próximas contas da mesma conexão.
  const nomeInstituicao = conta.instituicao?.trim() || "Instituição";
  const [existente] = await db
    .select({ id: schema.institutions.id })
    .from(schema.institutions)
    .where(
      and(eq(schema.institutions.user_id, userId), eq(schema.institutions.name, nomeInstituicao)),
    )
    .limit(1);

  const institutionId =
    existente?.id ??
    (
      await db
        .insert(schema.institutions)
        .values({ user_id: userId, name: nomeInstituicao, type: "bank" })
        .returning({ id: schema.institutions.id })
    )[0]!.id;

  // O tipo vem do que a Pluggy diz. `CREDIT` é cartão; o resto entra como
  // conta corrente, que é o recorte que a tela Hoje soma em "Disponível
  // agora". Se estiver errado, o usuário corrige em Contas & Cartões — o que
  // não dá para corrigir é uma conta que nunca foi criada.
  const tipoProduto = conta.tipo?.toUpperCase() === "CREDIT" ? "credit_card" : "checking_account";

  const [produto] = await db
    .insert(schema.financial_products)
    .values({
      user_id: userId,
      institution_id: institutionId,
      name: conta.nome?.trim() || nomeInstituicao,
      type: tipoProduto,
    })
    .returning({ id: schema.financial_products.id });

  const financialProductId = produto!.id;

  await db
    .update(schema.external_account_mappings)
    .set({ financial_product_id: financialProductId, status: "mapped" })
    .where(
      and(
        eq(schema.external_account_mappings.id, mappingId),
        eq(schema.external_account_mappings.user_id, userId),
      ),
    );

  return { financialProductId };
}

/**
 * Marca como "não importar" todas as contas ainda pendentes de uma conexão.
 *
 * A saída para o caso da Caixa: 31 contas idênticas em nome, das quais o
 * usuário quer duas. Sem isto, dispensar as outras 29 é 29 cliques.
 */
export async function ignorarContasPendentes(connectionId: string): Promise<{ ignoradas: number }> {
  const userId = await requireUserId();
  const db = getDb();

  const [conexao] = await db
    .select({ id: schema.provider_connections.id })
    .from(schema.provider_connections)
    .where(
      and(
        eq(schema.provider_connections.id, connectionId),
        eq(schema.provider_connections.user_id, userId),
      ),
    );
  if (!conexao) throw new Error("conexão não encontrada");

  const ignoradas = await db
    .update(schema.external_account_mappings)
    .set({ status: "ignored" })
    .where(
      and(
        eq(schema.external_account_mappings.provider_connection_id, connectionId),
        eq(schema.external_account_mappings.user_id, userId),
        eq(schema.external_account_mappings.status, "pending_mapping"),
      ),
    )
    .returning({ id: schema.external_account_mappings.id });

  return { ignoradas: ignoradas.length };
}
