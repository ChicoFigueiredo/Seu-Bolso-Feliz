import type { Metadata } from "next";
import { SkeletonRoute } from "@/components/shell/skeleton-route";

export const metadata: Metadata = { title: "Dívidas" };

export default function DividasPage() {
  return (
    <SkeletonRoute
      icon="trending_down"
      description="Em breve: o progresso de quitação dos seus compromissos de longo prazo."
    />
  );
}
