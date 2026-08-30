import type { Metadata } from "next";
import { SkeletonRoute } from "@/components/shell/skeleton-route";

export const metadata: Metadata = { title: "Inteligência" };

export default function InteligenciaPage() {
  return (
    <SkeletonRoute
      icon="auto_awesome"
      description="Em breve: sugestões e explicações automáticas sobre os seus lançamentos."
    />
  );
}
