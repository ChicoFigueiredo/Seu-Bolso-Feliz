"use client";

import { useCallback, useState } from "react";
import { useRouter } from "next/navigation";
import dynamic from "next/dynamic";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Plug } from "lucide-react";
import {
  createPluggyConnectToken,
  completePluggyConnection,
} from "@/app/actions/pluggy-connections";

// O widget toca `window` já na avaliação do módulo. Um import estático faz o
// Next avaliá-lo também no servidor — e o processo cai com
// "ReferenceError: window is not defined" antes de responder qualquer coisa.
// `ssr: false` faz o pacote existir só no navegador, que é o único lugar onde
// ele tem sentido. O build não pega isso: é falha de execução, não de tipo.
const PluggyConnect = dynamic(() => import("react-pluggy-connect").then((m) => m.PluggyConnect), {
  ssr: false,
});

interface PluggyConnectButtonProps {
  /** Presente quando o botão reautentica uma conexão existente em vez de criar uma nova. */
  itemIdToUpdate?: string;
  label?: string;
}

/**
 * Isola a integração com o widget Pluggy Connect (`react-pluggy-connect`) —
 * o resto da tela nunca importa esse pacote diretamente. `connectToken` é
 * buscado só quando o usuário clica (token de 30min, não vale manter em
 * estado ocioso na página).
 */
export function PluggyConnectButton({ itemIdToUpdate, label }: PluggyConnectButtonProps) {
  const router = useRouter();
  const [connectToken, setConnectToken] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const openWidget = useCallback(async () => {
    setLoading(true);
    try {
      const { connectToken } = await createPluggyConnectToken({ itemId: itemIdToUpdate });
      setConnectToken(connectToken);
    } catch (error) {
      toast.error("Não foi possível iniciar a conexão", {
        description: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setLoading(false);
    }
  }, [itemIdToUpdate]);

  const handleSuccess = useCallback(
    async (data: { item: { id: string } }) => {
      setConnectToken(null);
      try {
        await completePluggyConnection(data.item.id);
        toast.success("Conta conectada", {
          description: "Agora vincule cada conta encontrada a um produto financeiro.",
        });
        router.refresh();
      } catch (error) {
        toast.error("Conexão feita, mas falhou ao salvar", {
          description: error instanceof Error ? error.message : String(error),
        });
      }
    },
    [router],
  );

  return (
    <>
      <Button onClick={openWidget} disabled={loading}>
        <Plug className="mr-2 size-4" />
        {label ?? (itemIdToUpdate ? "Reconectar" : "Conectar conta")}
      </Button>
      {connectToken && (
        <PluggyConnect
          connectToken={connectToken}
          updateItem={itemIdToUpdate}
          onSuccess={handleSuccess}
          onError={(error) => {
            toast.error("Falha na conexão", { description: error.message });
          }}
          onClose={() => setConnectToken(null)}
        />
      )}
    </>
  );
}
