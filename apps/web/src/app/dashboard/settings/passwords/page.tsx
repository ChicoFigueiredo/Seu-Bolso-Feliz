import Link from "next/link";
import { ArrowLeft } from "lucide-react";
import { listSecrets } from "@/app/actions/secrets";
import { getSuppliers } from "@/app/actions/suppliers";
import { Button } from "@/components/ui/button";
import { SecretsManager } from "./secrets-manager";

export const metadata = { title: "Senhas de documentos" };

export default async function PasswordsPage() {
  const [secrets, fornecedores] = await Promise.all([listSecrets(), getSuppliers(true)]);

  return (
    <div className="mx-auto max-w-3xl space-y-6">
      <div>
        <Button asChild variant="ghost" size="sm" className="mb-2 -ml-2">
          <Link href="/dashboard/settings">
            <ArrowLeft className="mr-1 size-4" />
            Configurações
          </Link>
        </Button>
        <h1 className="text-2xl font-bold tracking-tight">Senhas de documentos</h1>
        <p className="text-muted-foreground">
          Faturas de cartão e contas costumam vir em PDF protegido. Sem a senha cadastrada, o
          documento entra na ingestão e para sem ser lido.
        </p>
      </div>

      <SecretsManager secrets={secrets} fornecedores={fornecedores} />
    </div>
  );
}
