"use client";

/**
 * A confirmação de desconexão — e o compromisso de que ela não minta.
 *
 * `revokeProviderConnection` (apps/web/src/app/actions/pluggy-connections.ts)
 * faz UMA coisa: muda `provider_connections.status` para `revoked`. Não apaga
 * movimentação, não apaga vínculo de conta, e não fala com a Pluggy nem com o
 * banco. Cada frase abaixo corresponde a um desses fatos verificados no
 * código — inclusive a última, que é a que um diálogo apressado esconderia:
 * o consentimento no banco continua de pé até o usuário revogá-lo lá.
 */

import { useState } from "react";
import { Button, Icon, Sheet } from "@sbf/design-system";
import type { Conexao } from "@/app/actions/painel-conexoes";
import { revokeProviderConnection } from "@/app/actions/pluggy-connections";

export interface DesconectarSheetProps {
  conexao: Conexao;
  aoFechar: () => void;
  aoConcluir: (mensagem: string) => void;
  aoFalhar: (mensagem: string) => void;
}

const FATOS: { icone: string; texto: string }[] = [
  {
    icone: "inventory_2",
    texto:
      "As movimentações, faturas e documentos que já entraram continuam aqui. Nada do que foi importado é apagado.",
  },
  {
    icone: "sync_disabled",
    texto: "A sincronização automática para: nenhuma movimentação nova vem deste banco.",
  },
  {
    icone: "link",
    texto:
      "O vínculo entre as contas do banco e as suas contas aqui fica guardado. Se você reconectar o mesmo banco depois, ele volta como estava.",
  },
  {
    icone: "gpp_maybe",
    texto:
      "O consentimento no seu banco NÃO é cancelado por aqui. Para encerrar o compartilhamento na origem, revogue no app do seu banco ou no portal do Open Finance.",
  },
];

export function DesconectarSheet({
  conexao,
  aoFechar,
  aoConcluir,
  aoFalhar,
}: DesconectarSheetProps) {
  const [desconectando, setDesconectando] = useState(false);

  async function confirmar() {
    setDesconectando(true);
    try {
      await revokeProviderConnection(conexao.id);
      aoConcluir(`${conexao.instituicao ?? "Banco"} desconectado.`);
      aoFechar();
    } catch (erro) {
      aoFalhar(erro instanceof Error ? erro.message : String(erro));
      setDesconectando(false);
    }
  }

  return (
    <div style={{ position: "fixed", inset: 0, zIndex: 50 }}>
      {/* `height: 100%` faz o véu escuro do Sheet cobrir a tela inteira em vez
          de só a altura do cartão — mesmo ajuste da folha "Mais" do shell. */}
      <Sheet
        title={`Desconectar ${conexao.instituicao ?? "este banco"}?`}
        onClose={aoFechar}
        style={{ height: "100%", overflowY: "auto" }}
      >
        <div style={{ display: "grid", gap: 12 }}>
          {FATOS.map((fato) => (
            <div key={fato.icone} style={{ display: "flex", gap: 10, alignItems: "flex-start" }}>
              <Icon
                name={fato.icone}
                size={19}
                style={{ color: "var(--text-3)", flex: "0 0 19px" }}
              />
              <span style={{ fontSize: 13, color: "var(--text-2)", lineHeight: 1.45 }}>
                {fato.texto}
              </span>
            </div>
          ))}
        </div>

        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginTop: 4 }}>
          <Button
            variant="primary"
            size="lg"
            icon="link_off"
            disabled={desconectando}
            onClick={() => void confirmar()}
            style={{ minHeight: 44, background: "var(--danger)", color: "var(--on-primary)" }}
          >
            {desconectando ? "Desconectando…" : "Desconectar"}
          </Button>
          <Button
            variant="outline"
            size="lg"
            disabled={desconectando}
            onClick={aoFechar}
            style={{ minHeight: 44 }}
          >
            Manter conectado
          </Button>
        </div>
      </Sheet>
    </div>
  );
}
