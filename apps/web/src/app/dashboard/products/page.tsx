import { createClient } from "@/lib/supabase/server";
import Link from "next/link";
import { Plus, Wallet } from "lucide-react";
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
import { formatCurrency } from "@/lib/format";

const productTypeLabels: Record<string, string> = {
  checking_account: "Conta Corrente",
  savings_account: "Poupança",
  credit_card: "Cartão de Crédito",
  debit_card: "Cartão de Débito",
  investment: "Investimento",
  loan: "Empréstimo",
  mortgage: "Financiamento",
  insurance: "Seguro",
  overdraft: "Cheque Especial",
  other: "Outro",
};

export default async function ProductsPage() {
  const supabase = await createClient();
  const { data: products } = await supabase
    .from("financial_products")
    .select("*, institutions(name)")
    .order("institution_id, name");

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Produtos Financeiros</h1>
          <p className="text-muted-foreground">Contas, cartões, investimentos e empréstimos</p>
        </div>
        <Button asChild>
          <Link href="/dashboard/products/new">
            <Plus className="mr-2 size-4" />
            Novo Produto
          </Link>
        </Button>
      </div>

      {(products ?? []).length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-16">
            <Wallet className="mb-4 size-12 text-muted-foreground" />
            <CardTitle className="mb-2">Nenhum produto cadastrado</CardTitle>
            <CardDescription className="mb-4">
              Cadastre contas, cartões e outros produtos das suas instituições.
            </CardDescription>
            <Button asChild>
              <Link href="/dashboard/products/new">
                <Plus className="mr-2 size-4" />
                Cadastrar Primeiro Produto
              </Link>
            </Button>
          </CardContent>
        </Card>
      ) : (
        <Card>
          <CardContent className="p-0">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Nome</TableHead>
                  <TableHead>Instituição</TableHead>
                  <TableHead>Tipo</TableHead>
                  <TableHead className="text-right">Saldo</TableHead>
                  <TableHead className="text-right">Ações</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {(products ?? []).map((prod) => (
                  <TableRow key={prod.id}>
                    <TableCell className="font-medium">{prod.name}</TableCell>
                    <TableCell>
                      {(prod.institutions as { name: string } | null)?.name ?? "—"}
                    </TableCell>
                    <TableCell>
                      <Badge variant="secondary">{productTypeLabels[prod.type] ?? prod.type}</Badge>
                    </TableCell>
                    <TableCell className="text-right font-mono">
                      {prod.current_balance != null ? formatCurrency(prod.current_balance) : "—"}
                    </TableCell>
                    <TableCell className="text-right">
                      <Button variant="ghost" size="sm" asChild>
                        <Link href={`/dashboard/products/${prod.id}`}>Editar</Link>
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
