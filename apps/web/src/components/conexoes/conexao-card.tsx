"use client";

/**
 * Um banco conectado, e o que ainda depende de uma decisão do usuário.
 *
 * A lista de contas nunca fica escondida atrás de um "ver mais": uma conta
 * que o banco mandou e que ninguém vinculou é a pendência mais cara desta
 * tela — o worker continua trazendo as movimentações dela, mas o rascunho
 * chega sem saber em que conta lançar. Esconder isso num acordeão é o mesmo
 * que não avisar.
 */

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Button, Card, Icon, Pill, formatBRL } from "@sbf/design-system";
import type { Conexao, ContaExterna, ProdutoOpcao } from "@/app/actions/painel-conexoes";
import { updateExternalAccountMapping } from "@/app/actions/pluggy-connections";
import { ConectarBanco } from "./conectar-banco";
import {
  aparenciaDaConexao,
  aparenciaDaConta,
  explicacaoDoStatus,
  momentoRelativo,
  plural,
  rotuloTipoExterno,
  rotuloTipoProduto,
} from "./rotulos";

/**
 * Sentinelas do `<select>`. Não podem colidir com um id de produto porque
 * `financial_products.id` é uuid — nenhum uuid tem underscore.
 */
const DEIXAR_PENDENTE = "__pendente__";
const NAO_IMPORTAR = "__ignorar__";

export interface ConexaoCardProps {
  conexao: Conexao;
  produtos: ProdutoOpcao[];
  /** Sem credencial no servidor não há como reautenticar — o botão nasce desligado. */
  pluggyConfigurado: boolean;
  aoAvisar: (mensagem: string, icone?: string) => void;
  aoFalhar: (mensagem: string) => void;
  /** Recarrega os dados da página depois de uma escrita. */
  aoMudar: () => void;
  aoPedirDesconexao: (conexao: Conexao) => void;
}

const estiloSelect = {
  width: "100%",
  minHeight: 44,
  padding: "10px 12px",
  borderRadius: "var(--sbf-radius-md)",
  border: "1px solid var(--line)",
  background: "var(--surface)",
  color: "var(--text)",
  fontFamily: "var(--sbf-font-sans)",
  fontSize: 13.5,
  fontWeight: 600,
} as const;

function valorDaConta(conta: ContaExterna): string {
  if (conta.status === "ignored") return NAO_IMPORTAR;
  if (conta.status === "mapped" && conta.produtoId) return conta.produtoId;
  return DEIXAR_PENDENTE;
}

function ContaExternaLinha({
  conta,
  produtos,
  aoAvisar,
  aoFalhar,
  aoMudar,
}: {
  conta: ContaExterna;
  produtos: ProdutoOpcao[];
  aoAvisar: (mensagem: string, icone?: string) => void;
  aoFalhar: (mensagem: string) => void;
  aoMudar: () => void;
}) {
  const [valor, setValor] = useState(() => valorDaConta(conta));
  const [salvando, setSalvando] = useState(false);
  const aparencia = aparenciaDaConta(conta.status);
  const tipo = rotuloTipoExterno(conta.tipo);

  async function salvar(novoValor: string) {
    const anterior = valor;
    setValor(novoValor);
    setSalvando(true);
    try {
      // Os três destinos possíveis são exatamente os três valores do CHECK
      // `external_account_mappings_status_check`. Nenhuma outra combinação é
      // oferecida porque nenhuma outra o banco aceita.
      if (novoValor === DEIXAR_PENDENTE) {
        await updateExternalAccountMapping(conta.id, {
          status: "pending_mapping",
          financial_product_id: null,
        });
        aoAvisar("Conta deixada como pendência.", "help");
      } else if (novoValor === NAO_IMPORTAR) {
        await updateExternalAccountMapping(conta.id, {
          status: "ignored",
          financial_product_id: null,
        });
        aoAvisar("Esta conta não será mais importada.", "block");
      } else {
        await updateExternalAccountMapping(conta.id, {
          status: "mapped",
          financial_product_id: novoValor,
        });
        aoAvisar("Conta vinculada.", "link");
      }
      aoMudar();
    } catch (erro) {
      // Voltar o select ao que o banco realmente tem: deixá-lo mostrando a
      // escolha que não foi salva é a forma mais fácil de a tela mentir.
      setValor(anterior);
      aoFalhar(erro instanceof Error ? erro.message : String(erro));
    } finally {
      setSalvando(false);
    }
  }

  return (
    <div
      style={{
        display: "grid",
        gap: 8,
        padding: "14px 0",
        borderTop: "1px solid var(--line-soft)",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
        <span style={{ fontSize: 13.5, fontWeight: 700, color: "var(--text)" }}>
          {conta.nome ?? "Conta sem nome"}
        </span>
        <Pill tone={aparencia.tom} icon={aparencia.icone} size="xs">
          {aparencia.rotulo}
        </Pill>
      </div>

      {tipo || conta.produtoNome ? (
        <div style={{ fontSize: 12.5, color: "var(--text-3)" }}>
          {[
            tipo,
            conta.produtoNome
              ? `${conta.produtoNome}${conta.produtoSaldo == null ? "" : ` · ${formatBRL(conta.produtoSaldo)}`}`
              : null,
          ]
            .filter(Boolean)
            .join(" · ")}
        </div>
      ) : null}

      <label style={{ display: "grid", gap: 5 }}>
        <span style={{ fontSize: 12, fontWeight: 600, color: "var(--text-3)" }}>
          Esta conta é a minha…
        </span>
        <select
          value={valor}
          disabled={salvando}
          onChange={(evento) => void salvar(evento.target.value)}
          style={{ ...estiloSelect, opacity: salvando ? 0.6 : 1 }}
        >
          <option value={DEIXAR_PENDENTE}>Ainda vou decidir</option>
          {produtos.length > 0 ? (
            <optgroup label="Vincular a">
              {produtos.map((produto) => (
                <option key={produto.id} value={produto.id}>
                  {produto.nome} — {rotuloTipoProduto(produto.tipo)}
                  {produto.instituicao ? ` (${produto.instituicao})` : ""}
                </option>
              ))}
            </optgroup>
          ) : null}
          <option value={NAO_IMPORTAR}>Não importar esta conta</option>
        </select>
      </label>
    </div>
  );
}

export function ConexaoCard({
  conexao,
  produtos,
  pluggyConfigurado,
  aoAvisar,
  aoFalhar,
  aoMudar,
  aoPedirDesconexao,
}: ConexaoCardProps) {
  const router = useRouter();
  const aparencia = aparenciaDaConexao(conexao);
  const explicacao = explicacaoDoStatus(conexao);
  const sincronizacao = momentoRelativo(conexao.ultimaSincronizacao);
  const pendentes = conexao.contas.filter((c) => c.status === "pending_mapping").length;
  const desconectada = conexao.status === "revoked";

  return (
    <Card padding={18} style={{ gap: 14 }}>
      <div style={{ display: "flex", alignItems: "flex-start", gap: 12 }}>
        <span
          style={{
            width: 40,
            height: 40,
            flex: "0 0 40px",
            borderRadius: "var(--sbf-radius-md)",
            background: "var(--secondary-soft)",
            color: "var(--secondary-ink)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <Icon name="account_balance" size={22} />
        </span>

        <div style={{ flex: "1 1 auto", minWidth: 0, display: "grid", gap: 6 }}>
          <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
            <span style={{ fontSize: 15, fontWeight: 700, color: "var(--text)" }}>
              {conexao.instituicao ?? "Banco sem nome"}
            </span>
            <Pill tone={aparencia.tom} icon={aparencia.icone} size="xs">
              {aparencia.rotulo}
            </Pill>
          </div>
          <div style={{ fontSize: 12.5, color: "var(--text-3)", lineHeight: 1.45 }}>
            {sincronizacao
              ? `Última sincronização ${sincronizacao}`
              : "Ainda não sincronizou nenhuma vez"}
            {" · "}
            {plural(conexao.contas.length, "conta trazida", "contas trazidas")}
          </div>
        </div>
      </div>

      {explicacao ? (
        <div
          style={{
            display: "flex",
            gap: 8,
            alignItems: "flex-start",
            background: conexao.status === "active" ? "var(--muted)" : "var(--warn-bg)",
            border: `1px solid ${conexao.status === "active" ? "var(--line)" : "var(--warn-line)"}`,
            borderRadius: "var(--sbf-radius-sm)",
            padding: "10px 12px",
            fontSize: 12.5,
            color: conexao.status === "active" ? "var(--text-3)" : "var(--warn)",
            lineHeight: 1.45,
          }}
        >
          <Icon name={conexao.status === "active" ? "info" : "warning"} size={17} />
          <span>{explicacao}</span>
        </div>
      ) : null}

      {conexao.contas.length > 0 ? (
        <div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              fontSize: 13,
              fontWeight: 700,
              color: "var(--text-2)",
            }}
          >
            <span>Contas deste banco</span>
            {pendentes > 0 && !desconectada ? (
              <Pill tone="warn" size="xs" icon="pending">
                {plural(pendentes, "sem vínculo", "sem vínculo")}
              </Pill>
            ) : null}
          </div>

          {produtos.length === 0 ? (
            <div style={{ display: "grid", gap: 8, marginTop: 8, justifyItems: "start" }}>
              <span style={{ fontSize: 12.5, color: "var(--text-3)", lineHeight: 1.45 }}>
                Você ainda não cadastrou nenhuma conta ou cartão aqui dentro, então não há a que
                vincular. Cadastre uma e volte — o que o banco mandou continua esperando.
              </span>
              <Button
                variant="outline"
                size="lg"
                icon="add"
                onClick={() => router.push("/dashboard/products")}
                style={{ minHeight: 44 }}
              >
                Cadastrar conta ou cartão
              </Button>
            </div>
          ) : null}

          {conexao.contas.map((conta) => (
            <ContaExternaLinha
              key={conta.id}
              conta={conta}
              produtos={produtos}
              aoAvisar={aoAvisar}
              aoFalhar={aoFalhar}
              aoMudar={aoMudar}
            />
          ))}
        </div>
      ) : (
        // A lista de contas é lida uma vez, no momento da conexão
        // (`completePluggyConnection`); a sincronização seguinte só usa o que
        // já está gravado, nunca pergunta de novo. Então dizer "espere a
        // próxima sincronização" seria mandar o usuário esperar por algo que
        // não vai acontecer — o caminho real é reautenticar.
        <div style={{ fontSize: 12.5, color: "var(--text-3)", lineHeight: 1.45 }}>
          O banco não devolveu nenhuma conta quando esta conexão foi criada. Isso não se resolve
          sozinho: use Reautenticar abaixo para pedir a lista de novo.
        </div>
      )}

      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
        <ConectarBanco
          itemId={conexao.itemId}
          rotulo={desconectada ? "Reconectar" : "Reautenticar"}
          icone={desconectada ? "link" : "lock_reset"}
          variante={desconectada ? "primary" : "soft"}
          tamanho="lg"
          desabilitado={!pluggyConfigurado}
          aoConcluir={(mensagem) => {
            aoAvisar(mensagem);
            aoMudar();
          }}
          aoFalhar={aoFalhar}
        />
        {desconectada ? null : (
          <Button
            variant="outline"
            size="lg"
            icon="link_off"
            onClick={() => aoPedirDesconexao(conexao)}
            style={{ minHeight: 44 }}
          >
            Desconectar
          </Button>
        )}
      </div>
    </Card>
  );
}
