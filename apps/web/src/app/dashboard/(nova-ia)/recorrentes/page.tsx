import type { Metadata } from "next";
import { SkeletonRoute } from "@/components/shell/skeleton-route";

export const metadata: Metadata = { title: "Recorrentes" };

export default function RecorrentesPage() {
  return (
    <SkeletonRoute
      icon="autorenew"
      description="Em breve: assinaturas, contas de consumo e parcelamentos que se repetem todo mês."
    />
  );
}
