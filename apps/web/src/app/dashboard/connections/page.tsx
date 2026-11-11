import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, desc, eq, sql } from "drizzle-orm";
import Link from "next/link";
import { Plug } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { PluggyConnectButton } from "@/components/pluggy-connect-button";

const statusVariants: Record<string, "default" | "destructive" | "secondary"> = {
  active: "default",
  error: "destructive",
  revoked: "secondary",
  expired: "secondary",
};

const statusLabels: Record<string, string> = {
  active: "Ativa",
  error: "Erro",
  revoked: "Revogada",
  expired: "Expirada",
};

export default async function ConnectionsPage() {
  const { ownerId } = await requireUser();
  const db = getDb();
  const connections = await db
    .select({
      id: schema.provider_connections.id,
      institution_name: schema.provider_connections.institution_name,
      status: schema.provider_connections.status,
      last_synced_at: schema.provider_connections.last_synced_at,
      account_count: sql<number>`count(${schema.external_account_mappings.id})`,
    })
    .from(schema.provider_connections)
    .leftJoin(
      schema.external_account_mappings,
      and(
        eq(schema.external_account_mappings.provider_connection_id, schema.provider_connections.id),
        eq(schema.external_account_mappings.user_id, ownerId),
      ),
    )
    .where(eq(schema.provider_connections.user_id, ownerId))
    .groupBy(schema.provider_connections.id)
    .orderBy(desc(schema.provider_connections.created_at));

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Conexões</h1>
          <p className="text-muted-foreground">
            Contas bancárias conectadas via Open Finance (Pluggy)
          </p>
        </div>
        <PluggyConnectButton />
      </div>

      {connections.length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-16">
            <Plug className="mb-4 size-12 text-muted-foreground" />
            <CardTitle className="mb-2">Nenhuma conexão ainda</CardTitle>
            <CardDescription className="mb-4">
              Conecte uma conta bancária para sincronizar transações automaticamente.
            </CardDescription>
            <PluggyConnectButton label="Conectar primeira conta" />
          </CardContent>
        </Card>
      ) : (
        <Card>
          <CardContent className="p-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Instituição</TableHead>
                  <TableHead>Status</TableHead>
                  <TableHead className="text-center">Contas</TableHead>
                  <TableHead>Última sincronização</TableHead>
                  <TableHead className="text-right">Ações</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {connections.map((conn) => (
                  <TableRow key={conn.id}>
                    <TableCell className="font-medium">
                      {conn.institution_name ?? "Instituição desconhecida"}
                    </TableCell>
                    <TableCell>
                      <Badge variant={statusVariants[conn.status] ?? "secondary"}>
                        {statusLabels[conn.status] ?? conn.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-center">{Number(conn.account_count)}</TableCell>
                    <TableCell className="text-muted-foreground">
                      {conn.last_synced_at
                        ? new Date(conn.last_synced_at).toLocaleString("pt-BR")
                        : "Nunca sincronizado"}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button variant="ghost" size="sm" asChild>
                        <Link href={`/dashboard/connections/${conn.id}`}>Mapear contas</Link>
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
