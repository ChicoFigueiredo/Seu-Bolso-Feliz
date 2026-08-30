import type { Metadata } from "next";
import { SkeletonRoute } from "@/components/shell/skeleton-route";

export const metadata: Metadata = { title: "Projetos & Centros" };

export default function CentrosPage() {
  return (
    <SkeletonRoute
      icon="workspaces"
      description="Em breve: o saldo e os rateios de cada projeto ou pessoa para quem o dinheiro vai."
    />
  );
}
