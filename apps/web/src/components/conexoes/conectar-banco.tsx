"use client";

/**
 * O botão que abre o Pluggy Connect — a única porta por onde um banco entra
 * no Seu Bolso Feliz.
 *
 * O widget vem do pacote `react-pluggy-connect`, que JÁ é dependência de
 * apps/web (package.json) — nada de script de CDN nem de dependência nova.
 * Este arquivo existe separado do resto da tela por dois motivos: manter o
 * pacote isolado num único ponto de import, e não vazar o estado do widget
 * para o componente que desenha os cards.
 *
 * O `connectToken` só é pedido no clique. Ele vale 30 minutos e é o ÚNICO
 * segredo que atravessa para o navegador — `PLUGGY_CLIENT_SECRET` fica no
 * servidor, dentro de `createPluggyConnectToken`.
 */

import { useCallback, useState } from "react";
import dynamic from "next/dynamic";
import { Button, type ButtonSize, type ButtonVariant } from "@sbf/design-system";
import {
  completePluggyConnection,
  createPluggyConnectToken,
} from "@/app/actions/pluggy-connections";

// O widget toca `window` já na avaliação do módulo. Um import estático faz o
// Next avaliá-lo também no servidor — e o processo cai com
// "ReferenceError: window is not defined" antes de responder qualquer coisa.
// `ssr: false` faz o pacote existir só no navegador, que é o único lugar onde
// ele tem sentido. O build não pega isso: é falha de execução, não de tipo.
const PluggyConnect = dynamic(() => import("react-pluggy-connect").then((m) => m.PluggyConnect), {
  ssr: false,
});

export interface ConectarBancoProps {
  /**
   * Presente quando é reautenticação de uma conexão existente
   * (`provider_connections.external_item_id`). O widget abre em modo update:
   * o banco pede a senha de novo sem criar uma segunda conexão.
   */
  itemId?: string;
  rotulo: string;
  icone?: string;
  variante?: ButtonVariant;
  tamanho?: ButtonSize;
  bloco?: boolean;
  desabilitado?: boolean;
  /** Conexão salva no banco — a tela precisa recarregar os dados. */
  aoConcluir: (mensagem: string) => void;
  aoFalhar: (mensagem: string) => void;
}

/**
 * Códigos da Pluggy que chegam crus e não dizem nada a quem está na tela.
 *
 * Um `TRIAL_CLIENT_ITEM_CREATE_NOT_ALLOWED` estampado no painel manda a pessoa
 * pesquisar o que ela deveria fazer. Traduzir aqui é o mínimo; dizer o caminho
 * de saída é o que resolve.
 */
const RECADOS: Array<[RegExp, string]> = [
  [
    /TRIAL_CLIENT_ITEM_CREATE_NOT_ALLOWED/i,
    "A sua aplicação na Pluggy está em teste, e o teste não deixa criar conexão nova. " +
      "O caminho gratuito é o Meu Pluggy: ligue os bancos em meu.pluggy.ai e depois " +
      "conecte aqui escolhendo “MeuPluggy” na lista.",
  ],
  [
    /CLIENT_NOT_ALLOWED|FORBIDDEN|Forbidden/i,
    "A Pluggy recusou a operação para esta aplicação — normalmente é limite do plano.",
  ],
  [/MISSING_OR_INVALID_CREDENTIALS|INVALID_CREDENTIALS/i, "O banco recusou os dados informados."],
];

function mensagemDoErro(erro: unknown): string {
  const bruto = erro instanceof Error ? erro.message : String(erro);
  for (const [padrao, recado] of RECADOS) {
    if (padrao.test(bruto)) return recado;
  }
  return bruto;
}

export function ConectarBanco({
  itemId,
  rotulo,
  icone = "add_link",
  variante = "primary",
  tamanho = "lg",
  bloco = false,
  desabilitado = false,
  aoConcluir,
  aoFalhar,
}: ConectarBancoProps) {
  const [connectToken, setConnectToken] = useState<string | null>(null);
  // Os conectores que o widget pode oferecer. Lista vazia = sem filtro.
  const [connectorIds, setConnectorIds] = useState<number[]>([]);
  const [abrindo, setAbrindo] = useState(false);

  const abrirWidget = useCallback(
    async (incluirConectoresComSenha = false) => {
      setAbrindo(true);
      try {
        const resposta = await createPluggyConnectToken({ itemId, incluirConectoresComSenha });
        setConnectorIds(resposta.connectorIds);
        setConnectToken(resposta.connectToken);
      } catch (erro) {
        aoFalhar(mensagemDoErro(erro));
      } finally {
        setAbrindo(false);
      }
    },
    [itemId, aoFalhar],
  );

  const concluir = useCallback(
    async (dados: { item: { id: string } }) => {
      // Fechar o widget antes de gravar: a chamada abaixo bate na Pluggy para
      // listar as contas e pode demorar alguns segundos. Deixar o modal aberto
      // nesse intervalo faz parecer que travou.
      setConnectToken(null);
      try {
        await completePluggyConnection(dados.item.id);
        aoConcluir(
          itemId
            ? "Conexão reautenticada."
            : "Banco conectado. Agora diga qual conta é qual, abaixo.",
        );
      } catch (erro) {
        // Distinção que importa: o banco autorizou, o que falhou foi guardar.
        // Dizer só "falhou" faria o usuário tentar autorizar de novo à toa.
        aoFalhar(`O banco autorizou, mas falhou ao salvar aqui: ${mensagemDoErro(erro)}`);
      }
    },
    [itemId, aoConcluir, aoFalhar],
  );

  return (
    <>
      <Button
        variant={variante}
        size={tamanho}
        icon={icone}
        block={bloco}
        disabled={desabilitado || abrindo}
        onClick={() => void abrirWidget(false)}
        style={{ minHeight: 44 }}
      >
        {abrindo ? "Abrindo…" : rotulo}
      </Button>

      {/* Saída para as poucas instituições que ainda não têm Open Finance —
          sobretudo corretoras e contas PJ. Fica discreta e diz o preço: por
          este caminho a senha do banco é digitada no widget. */}
      {!itemId ? (
        <button
          type="button"
          onClick={() => void abrirWidget(true)}
          disabled={abrindo}
          style={{
            display: "block",
            marginTop: 8,
            padding: 0,
            border: 0,
            background: "none",
            font: "inherit",
            fontSize: 12,
            color: "var(--text-3)",
            textDecoration: "underline",
            cursor: "pointer",
          }}
        >
          Minha instituição não aparece (usa usuário e senha)
        </button>
      ) : null}

      {connectToken ? (
        <PluggyConnect
          connectToken={connectToken}
          updateItem={itemId}
          // Lista vazia = sem filtro (reautenticação, ou a consulta falhou).
          {...(connectorIds.length > 0 ? { connectorIds } : {})}
          language="pt"
          onSuccess={concluir}
          onError={(erro) => {
            setConnectToken(null);
            aoFalhar(erro.message || "O banco não concluiu a autorização.");
          }}
          onClose={() => setConnectToken(null)}
        />
      ) : null}
    </>
  );
}
