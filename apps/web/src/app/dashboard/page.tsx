import { redirect } from "next/navigation";

// A raiz do painel passou a ser "Hoje". A tela antiga vivia aqui; o conteúdo
// dela foi reconstruído em /dashboard/hoje sobre o design novo.
export default function DashboardPage() {
  redirect("/dashboard/hoje");
}
