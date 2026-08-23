/**
 * `/dashboard/cards` — issue #5.
 *
 * A rota não existia. Sem ela não havia como cadastrar um cartão pela
 * interface, e como `statement_cycles.card_id` é `NOT NULL`, nenhuma fatura
 * podia ser criada — a jornada do Marco 2 parava antes de começar.
 */
import Link from "next/link";
import { CreditCard as CreditCardIcon, Plus, CalendarClock, CircleDollarSign } from "lucide-react";
import { getCards } from "@/app/actions/cards";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { formatCurrency } from "@/lib/format";
import { CardActions } from "./card-actions";

export const metadata = { title: "Cartões" };

/** `20` → "dia 20". `null` → "—", porque zero não é um dia do mês. */
function diaDoMes(dia: number | null): string {
  return dia == null ? "—" : `dia ${dia}`;
}

export default async function CardsPage() {
  const cards = await getCards();

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Cartões</h1>
          <p className="text-muted-foreground">
            Limite, fechamento e vencimento de cada cartão cadastrado
          </p>
        </div>
        <Button asChild>
          <Link href="/dashboard/cards/new">
            <Plus className="mr-2 size-4" />
            Novo cartão
          </Link>
        </Button>
      </div>

      {cards.length === 0 ? (
        <Card>
          <CardContent className="flex flex-col items-center justify-center py-16 text-center">
            <CreditCardIcon className="mb-4 size-12 text-muted-foreground" />
            <CardTitle className="mb-2">Nenhum cartão cadastrado</CardTitle>
            <CardDescription className="mb-6 max-w-md">
              Uma fatura precisa estar ligada a um cartão. Cadastre o primeiro para conseguir
              registrar faturas e acompanhar o ciclo.
            </CardDescription>
            <Button asChild>
              <Link href="/dashboard/cards/new">
                <Plus className="mr-2 size-4" />
                Cadastrar cartão
              </Link>
            </Button>
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-3">
          {cards.map((card) => {
            const produto = card.financial_products;
            const instituicao = produto?.institutions?.name;

            return (
              <Card key={card.id} className={card.is_active ? undefined : "opacity-60"}>
                <CardHeader className="pb-3">
                  <div className="flex items-start justify-between gap-2">
                    <div className="min-w-0">
                      <CardTitle className="truncate text-base">
                        {produto?.name ?? "Produto removido"}
                      </CardTitle>
                      <CardDescription className="truncate">
                        {[instituicao, card.card_brand].filter(Boolean).join(" · ") || "—"}
                      </CardDescription>
                    </div>
                    <div className="flex shrink-0 flex-col items-end gap-1">
                      {card.is_primary && <Badge>Principal</Badge>}
                      {!card.is_active && <Badge variant="outline">Inativo</Badge>}
                    </div>
                  </div>
                </CardHeader>

                <CardContent className="space-y-3">
                  <p className="font-mono text-lg tracking-widest text-muted-foreground">
                    •••• {card.last_four_digits ?? "••••"}
                  </p>

                  <dl className="space-y-2 text-sm">
                    <div className="flex items-center justify-between gap-2">
                      <dt className="flex items-center gap-1.5 text-muted-foreground">
                        <CircleDollarSign className="size-3.5" />
                        Limite
                      </dt>
                      <dd className="font-medium">
                        {card.credit_limit == null ? "—" : formatCurrency(card.credit_limit)}
                      </dd>
                    </div>
                    <div className="flex items-center justify-between gap-2">
                      <dt className="flex items-center gap-1.5 text-muted-foreground">
                        <CalendarClock className="size-3.5" />
                        Fechamento
                      </dt>
                      <dd className="font-medium">{diaDoMes(card.closing_day)}</dd>
                    </div>
                    <div className="flex items-center justify-between gap-2">
                      <dt className="flex items-center gap-1.5 text-muted-foreground">
                        <CalendarClock className="size-3.5" />
                        Vencimento
                      </dt>
                      <dd className="font-medium">{diaDoMes(card.due_day)}</dd>
                    </div>
                  </dl>

                  {card.holder_name && (
                    <p className="truncate text-xs uppercase text-muted-foreground">
                      {card.holder_name}
                    </p>
                  )}

                  <CardActions id={card.id} isActive={card.is_active} />
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}
