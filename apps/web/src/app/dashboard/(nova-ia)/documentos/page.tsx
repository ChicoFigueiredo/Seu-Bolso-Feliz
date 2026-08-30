import type { Metadata } from "next";
import { SkeletonRoute } from "@/components/shell/skeleton-route";

export const metadata: Metadata = { title: "Documentos" };

export default function DocumentosPage() {
  return (
    <SkeletonRoute
      icon="folder_open"
      description="Em breve: a caixa de entrada de boletos, faturas e comprovantes, com o que precisa da sua revisão."
    />
  );
}
