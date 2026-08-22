"use client";

import { useTransition } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { toast } from "sonner";
import { Pencil, Power, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { deleteCard, updateCard } from "@/app/actions/cards";

/**
 * Ações de um cartão.
 *
 * Desativar vem antes de excluir de propósito: um cartão cancelado ainda tem
 * faturas e transações passadas, e apagá-lo apagaria a explicação do que já
 * aconteceu. `deleteCard` recusa quando há fatura ligada — aqui a recusa vira
 * uma frase que diz o que fazer em vez de um erro de constraint.
 */
export function CardActions({ id, isActive }: { id: string; isActive: boolean }) {
  const router = useRouter();
  const [pendente, startTransition] = useTransition();

  function alternarAtivo() {
    startTransition(async () => {
      try {
        await updateCard(id, { is_active: !isActive });
        toast.success(isActive ? "Cartão desativado" : "Cartão reativado");
        router.refresh();
      } catch (err) {
        toast.error("Não foi possível alterar o cartão", {
          description: err instanceof Error ? err.message : "Erro desconhecido",
        });
      }
    });
  }

  function excluir() {
    if (!confirm("Excluir este cartão? A ação não pode ser desfeita.")) return;

    startTransition(async () => {
      try {
        await deleteCard(id);
        toast.success("Cartão excluído");
        router.refresh();
      } catch (err) {
        toast.error("Não foi possível excluir", {
          description: err instanceof Error ? err.message : "Erro desconhecido",
        });
      }
    });
  }

  return (
    <div className="flex gap-2 pt-1">
      <Button asChild size="sm" variant="outline" className="flex-1">
        <Link href={`/dashboard/cards/${id}`}>
          <Pencil className="mr-1.5 size-3.5" />
          Editar
        </Link>
      </Button>
      <Button size="sm" variant="outline" onClick={alternarAtivo} disabled={pendente}>
        <Power className="size-3.5" />
        <span className="sr-only">{isActive ? "Desativar" : "Reativar"}</span>
      </Button>
      <Button size="sm" variant="outline" onClick={excluir} disabled={pendente}>
        <Trash2 className="size-3.5" />
        <span className="sr-only">Excluir</span>
      </Button>
    </div>
  );
}
