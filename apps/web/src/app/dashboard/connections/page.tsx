import { createClient } from "@/lib/supabase/server";
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
  const supabase = await createClient();
  const { data: connections } = await supabase
    .from("provider_connections")
    .select("*, external_account_mappings(count)")
    .order("created_at", { ascending: false });

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

      {(connections ?? []).length === 0 ? (
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
                {(connections ?? []).map((conn) => (
                  <TableRow key={conn.id}>
                    <TableCell className="font-medium">
                      {conn.institution_name ?? "Instituição desconhecida"}
                    </TableCell>
                    <TableCell>
                      <Badge variant={statusVariants[conn.status] ?? "secondary"}>
                        {statusLabels[conn.status] ?? conn.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-center">
                      {(conn.external_account_mappings as { count: number }[])?.[0]?.count ?? 0}
                    </TableCell>
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
