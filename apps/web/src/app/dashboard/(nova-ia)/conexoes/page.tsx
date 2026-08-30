import type { Metadata } from "next";
import { SkeletonRoute } from "@/components/shell/skeleton-route";

export const metadata: Metadata = { title: "Conexões" };

export default function ConexoesPage() {
  return (
    <SkeletonRoute
      icon="sync_alt"
      description="Em breve: o estado de cada banco, e-mail e pasta conectados ao Seu Bolso Feliz."
    />
  );
}
