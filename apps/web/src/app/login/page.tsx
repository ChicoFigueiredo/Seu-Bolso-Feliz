import { redirect } from "next/navigation";
import { getUser } from "@/lib/auth";
import { SbfTheme } from "@sbf/design-system";
import { FormularioLogin } from "./formulario-login";

export const metadata = { title: "Entrar — Seu Bolso Feliz" };

export default async function LoginPage({
  searchParams,
}: {
  searchParams: Promise<{ erro?: string; next?: string }>;
}) {
  if (await getUser()) redirect("/dashboard/hoje");

  const { erro, next } = await searchParams;

  return (
    <SbfTheme theme="salvia" fullBleed>
      <main
        style={{
          minHeight: "100dvh",
          display: "grid",
          placeItems: "center",
          padding: "24px",
          background: "var(--bg)",
        }}
      >
        <FormularioLogin erro={erro} proximo={next} />
      </main>
    </SbfTheme>
  );
}
