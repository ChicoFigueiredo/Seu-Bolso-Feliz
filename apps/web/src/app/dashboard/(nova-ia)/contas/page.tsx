import type { Metadata } from "next";
import { SkeletonRoute } from "@/components/shell/skeleton-route";

export const metadata: Metadata = { title: "Contas & Cartões" };

export default function ContasPage() {
  return (
    <SkeletonRoute
      icon="account_balance"
      description="Em breve: contas bancárias, cartões e faturas reunidos em um só lugar."
    />
  );
}
