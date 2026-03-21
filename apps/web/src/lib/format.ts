import { formatDistanceToNow, format, isPast, isToday } from "date-fns";
import { ptBR } from "date-fns/locale";

export function formatCurrency(value: number): string {
  return new Intl.NumberFormat("pt-BR", {
    style: "currency",
    currency: "BRL",
  }).format(value);
}

export function formatDate(date: string | Date): string {
  return format(new Date(date), "dd/MM/yyyy", { locale: ptBR });
}

export function formatDateShort(date: string | Date): string {
  return format(new Date(date), "dd/MM", { locale: ptBR });
}

export function formatRelativeDate(date: string | Date): string {
  const d = new Date(date);
  if (isToday(d)) return "Hoje";
  return formatDistanceToNow(d, { addSuffix: true, locale: ptBR });
}

export function isOverdue(date: string | Date): boolean {
  return isPast(new Date(date)) && !isToday(new Date(date));
}

export function isDueToday(date: string | Date): boolean {
  return isToday(new Date(date));
}
