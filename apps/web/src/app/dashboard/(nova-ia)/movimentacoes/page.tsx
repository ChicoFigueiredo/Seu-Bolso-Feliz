import type { Metadata } from "next";
import { SkeletonRoute } from "@/components/shell/skeleton-route";

export const metadata: Metadata = { title: "Movimentações" };

export default function MovimentacoesPage() {
  return (
    <SkeletonRoute
      icon="receipt_long"
      description="Em breve: tudo que entrou e saiu, com busca e filtros por categoria, tag e centro."
    />
  );
}
