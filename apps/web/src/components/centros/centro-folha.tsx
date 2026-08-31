"use client";

/**
 * Criar e editar um Centro — e as duas formas de tirá-lo da frente, que **não
 * são a mesma coisa**:
 *
 * - **Arquivar** tira o centro das listas de escolha e o histórico continua
 *   inteiro. É o caminho normal para um projeto que acabou.
 * - **Excluir** apaga o centro de vez, e só é possível enquanto ele não tiver
 *   nenhum rateio — a FK é `on delete restrict`, porque sumir com o centro
 *   levaria junto a história do dinheiro que passou por ele.
 *
 * Por isso "Excluir" só aparece habilitado quando é verdade. Quando não é, o
 * botão continua visível mas desligado, com o motivo escrito ao lado: um botão
 * que abre um diálogo bonito para depois falhar é pior do que um botão
 * desligado que explica.
 */

import { useState, useTransition } from "react";
import { Button, Card, Icon } from "@sbf/design-system";
import {
  archiveFinancialCenter,
  createFinancialCenter,
  deleteFinancialCenter,
  unarchiveFinancialCenter,
  updateFinancialCenter,
} from "@/app/actions/financial-centers";
import type { CentroNoPainel } from "@/app/actions/painel-centros";
import { CampoSelecao, CampoTexto } from "./campos";
import { Folha } from "./folha";
import { Recado } from "./rateio-folha";
import {
  CORES_DE_CENTRO,
  COR_PADRAO,
  ICONES_DE_CENTRO,
  ICONE_PADRAO,
  TIPOS_DE_CENTRO,
  corDoCentro,
  iconeDoCentro,
} from "./paleta";

export interface CentroFolhaProps {
  /** `null` cria um centro novo; um centro edita o existente. `undefined` fecha. */
  centro: CentroNoPainel | null | undefined;
  onFechar: () => void;
  onGravado: () => void;
  /**
   * Chamado no lugar de `onGravado` quando o centro foi **excluído**. A tela
   * de detalhe precisa saber a diferença: recarregar uma página cujo centro
   * acabou de sumir cairia num 404 logo depois de uma ação bem-sucedida.
   */
  onExcluido?: () => void;
}

/** O `type` vem de um `<select>` fechado; esta função troca um `as` por uma
 * garantia de verdade — valor estranho vira o padrão em vez de vazar. */
function normalizaTipo(valor: string): "projeto" | "pessoa" | "negocio" {
  return valor === "pessoa" || valor === "negocio" ? valor : "projeto";
}

export function CentroFolha({ centro, onFechar, onGravado, onExcluido }: CentroFolhaProps) {
  if (centro === undefined) return null;
  return (
    <Formulario
      key={centro?.id ?? "novo"}
      centro={centro}
      onFechar={onFechar}
      onGravado={onGravado}
      onExcluido={onExcluido ?? onGravado}
    />
  );
}

function Formulario({
  centro,
  onFechar,
  onGravado,
  onExcluido,
}: {
  centro: CentroNoPainel | null;
  onFechar: () => void;
  onGravado: () => void;
  onExcluido: () => void;
}) {
  const editando = centro !== null;

  const [nome, setNome] = useState(centro?.nome ?? "");
  const [tipo, setTipo] = useState(centro?.tipo ?? "projeto");
  const [cor, setCor] = useState(corDoCentro(centro?.cor));
  const [icone, setIcone] = useState(iconeDoCentro(centro?.icone));
  const [descricao, setDescricao] = useState(centro?.descricao ?? "");
  const [erro, setErro] = useState<string | null>(null);
  const [confirmandoExclusao, setConfirmandoExclusao] = useState(false);
  const [gravando, iniciarGravacao] = useTransition();

  const nomeVazio = nome.trim() === "";
  const temRateio = (centro?.rateios ?? 0) > 0;

  const executar = (acao: () => Promise<unknown>, aoTerminar: () => void = onGravado) => {
    setErro(null);
    iniciarGravacao(async () => {
      try {
        await acao();
        aoTerminar();
        onFechar();
      } catch (falha) {
        setErro(
          falha instanceof Error
            ? falha.message
            : "Não consegui salvar. Tente de novo em instantes.",
        );
      }
    });
  };

  const salvar = () => {
    if (nomeVazio) {
      setErro("O centro precisa de um nome.");
      return;
    }
    const campos = {
      name: nome.trim(),
      type: normalizaTipo(tipo),
      description: descricao.trim() === "" ? null : descricao.trim(),
      color: cor,
      icon: icone,
    };
    executar(() =>
      centro ? updateFinancialCenter(centro.id, campos) : createFinancialCenter(campos),
    );
  };

  return (
    <Folha titulo={editando ? "Editar centro" : "Novo centro"} aberta onFechar={onFechar}>
      <CampoTexto
        label="Nome"
        valor={nome}
        maxLength={200}
        autoFocus={!editando}
        placeholder="Edulingo"
        ajuda="É o nome que aparece no rateio e nos relatórios."
        erro={nomeVazio && erro ? "O centro precisa de um nome." : null}
        onChange={setNome}
      />

      <CampoSelecao
        label="Natureza"
        valor={tipo}
        opcoes={TIPOS_DE_CENTRO.map((opcao) => ({ valor: opcao.valor, rotulo: opcao.rotulo }))}
        ajuda="Só organiza a lista — não muda nenhum cálculo."
        onChange={setTipo}
      />

      <EscolhaDeCor cor={cor} onChange={setCor} />
      <EscolhaDeIcone icone={icone} cor={cor} onChange={setIcone} />

      <CampoTexto
        label="Descrição"
        valor={descricao}
        linhas={3}
        maxLength={1000}
        placeholder="O que entra e o que não entra neste centro."
        ajuda="Opcional. Ajuda a lembrar do critério daqui a seis meses."
        onChange={setDescricao}
      />

      {erro ? (
        <Recado tom="alerta" icone="error">
          {erro}
        </Recado>
      ) : null}

      <div style={{ display: "flex", gap: 10 }}>
        <Button variant="outline" size="lg" onClick={onFechar} style={{ flex: "0 0 auto" }}>
          Cancelar
        </Button>
        <Button variant="primary" size="lg" block disabled={gravando} onClick={salvar}>
          {gravando ? "Salvando…" : editando ? "Salvar alterações" : "Criar centro"}
        </Button>
      </div>

      {centro ? (
        <Card variant="quiet" style={{ gap: 12 }}>
          <div style={{ fontSize: 13.5, fontWeight: 700, color: "var(--text)" }}>
            Tirar este centro da frente
          </div>

          <LinhaDeSaida
            icone={centro.status === "archived" ? "unarchive" : "archive"}
            titulo={centro.status === "archived" ? "Reativar" : "Arquivar"}
            explicacao={
              centro.status === "archived"
                ? "Volta a aparecer nas listas de escolha e a receber rateio novo."
                : "Some das listas de escolha e para de receber rateio novo. Todo o histórico continua: o que já foi rateado para ele segue somando nos relatórios."
            }
            rotuloAcao={centro.status === "archived" ? "Reativar" : "Arquivar"}
            onAcao={() =>
              executar(() =>
                centro.status === "archived"
                  ? unarchiveFinancialCenter(centro.id)
                  : archiveFinancialCenter(centro.id),
              )
            }
            desabilitado={gravando}
          />

          <LinhaDeSaida
            icone="delete"
            titulo="Excluir de vez"
            perigosa
            explicacao={
              temRateio
                ? `Não dá: ${String(centro.rateios)} ${
                    centro.rateios === 1
                      ? "movimentação já foi rateada"
                      : "movimentações já foram rateadas"
                  } para este centro. Apagá-lo levaria junto a história do dinheiro que passou por ele — o caminho aqui é arquivar.`
                : confirmandoExclusao
                  ? "Isto não tem volta. O centro some da lista para sempre."
                  : "Só é possível porque este centro ainda não tem nenhum rateio."
            }
            rotuloAcao={confirmandoExclusao ? "Confirmar exclusão" : "Excluir"}
            onAcao={() => {
              if (!confirmandoExclusao) {
                setConfirmandoExclusao(true);
                return;
              }
              executar(() => deleteFinancialCenter(centro.id), onExcluido);
            }}
            desabilitado={temRateio || gravando}
          />
        </Card>
      ) : null}
    </Folha>
  );
}

function LinhaDeSaida({
  icone,
  titulo,
  explicacao,
  rotuloAcao,
  onAcao,
  desabilitado,
  perigosa = false,
}: {
  icone: string;
  titulo: string;
  explicacao: string;
  rotuloAcao: string;
  onAcao: () => void;
  desabilitado: boolean;
  perigosa?: boolean;
}) {
  return (
    <div style={{ display: "grid", gap: 8 }}>
      <div style={{ display: "flex", gap: 10, alignItems: "flex-start" }}>
        <Icon
          name={icone}
          size={18}
          style={{ color: perigosa ? "var(--danger)" : "var(--text-3)", flex: "0 0 auto" }}
        />
        <div style={{ display: "grid", gap: 3, minWidth: 0 }}>
          <span style={{ fontSize: 13, fontWeight: 700, color: "var(--text-2)" }}>{titulo}</span>
          <span style={{ fontSize: 12.5, color: "var(--text-3)", lineHeight: 1.45 }}>
            {explicacao}
          </span>
        </div>
      </div>
      <Button
        variant="outline"
        size="lg"
        disabled={desabilitado}
        onClick={onAcao}
        style={perigosa && !desabilitado ? { color: "var(--danger)" } : undefined}
      >
        {rotuloAcao}
      </Button>
    </div>
  );
}

function EscolhaDeCor({ cor, onChange }: { cor: string; onChange: (valor: string) => void }) {
  return (
    <div style={{ display: "grid", gap: 6 }}>
      <span
        style={{
          fontFamily: "var(--sbf-font-sans)",
          fontSize: 12.5,
          fontWeight: 700,
          color: "var(--text-3)",
        }}
      >
        Cor
      </span>
      <div
        role="radiogroup"
        aria-label="Cor do centro"
        style={{ display: "flex", gap: 8, flexWrap: "wrap" }}
      >
        {CORES_DE_CENTRO.map((opcao) => {
          const ativa = opcao.valor.toLowerCase() === cor.toLowerCase();
          return (
            <button
              key={opcao.valor}
              type="button"
              role="radio"
              aria-checked={ativa}
              aria-label={opcao.nome}
              title={opcao.nome}
              onClick={() => onChange(opcao.valor)}
              style={{
                width: 44,
                height: 44,
                borderRadius: "var(--sbf-radius-md)",
                cursor: "pointer",
                background: opcao.valor,
                border: ativa ? "3px solid var(--text)" : "1px solid var(--line)",
                padding: 0,
              }}
            />
          );
        })}
      </div>
      <span style={{ fontFamily: "var(--sbf-font-sans)", fontSize: 12, color: "var(--text-3)" }}>
        {`A cor é como o centro é reconhecido sem ler o nome. Padrão: ${
          CORES_DE_CENTRO.find((opcao) => opcao.valor === COR_PADRAO)?.nome ?? "Sálvia"
        }.`}
      </span>
    </div>
  );
}

function EscolhaDeIcone({
  icone,
  cor,
  onChange,
}: {
  icone: string;
  cor: string;
  onChange: (valor: string) => void;
}) {
  return (
    <div style={{ display: "grid", gap: 6 }}>
      <span
        style={{
          fontFamily: "var(--sbf-font-sans)",
          fontSize: 12.5,
          fontWeight: 700,
          color: "var(--text-3)",
        }}
      >
        Ícone
      </span>
      <div
        role="radiogroup"
        aria-label="Ícone do centro"
        style={{ display: "flex", gap: 8, flexWrap: "wrap" }}
      >
        {ICONES_DE_CENTRO.map((opcao) => {
          const ativo = opcao.valor === (icone || ICONE_PADRAO);
          return (
            <button
              key={opcao.valor}
              type="button"
              role="radio"
              aria-checked={ativo}
              aria-label={opcao.nome}
              title={opcao.nome}
              onClick={() => onChange(opcao.valor)}
              style={{
                width: 44,
                height: 44,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                borderRadius: "var(--sbf-radius-md)",
                cursor: "pointer",
                background: ativo
                  ? `color-mix(in srgb, ${cor} 18%, transparent)`
                  : "var(--surface)",
                color: ativo ? cor : "var(--text-3)",
                border: `1px solid ${ativo ? cor : "var(--line)"}`,
                padding: 0,
              }}
            >
              <Icon name={opcao.valor} size={21} />
            </button>
          );
        })}
      </div>
    </div>
  );
}
