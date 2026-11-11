"use client";

import { useState, useTransition } from "react";
import { Button, Sheet } from "@sbf/design-system";
import {
  createFinancialProduct,
  deleteFinancialProduct,
  updateFinancialProduct,
} from "@/app/actions/financial-products";
import type { ContaDetalhe, InstituicaoResumo } from "@/app/actions/painel-contas";
import {
  CampoAlternavel,
  CampoNumero,
  CampoSelecao,
  CampoTexto,
  numeroOuNulo,
  type OpcaoCampo,
} from "./campos";

/**
 * Só os dois tipos que a seção "Contas" desta tela entende — mesmo recorte de
 * `TIPOS_CONTA` em `app/actions/painel-contas.ts` (que espelha `CONTAS` em
 * `painel-hoje.ts`). Os demais valores aceitos por
 * `financial_products_type_check` (investimento, empréstimo, financiamento,
 * cheque especial) não têm seção própria na arquitetura de informação nova
 * ainda; cartão de crédito tem o próprio fluxo (`CartaoSheet`).
 */
const TIPOS: readonly OpcaoCampo[] = [
  { value: "checking_account", label: "Conta corrente" },
  { value: "savings_account", label: "Poupança" },
];

export interface ContaSheetProps {
  conta?: ContaDetalhe;
  instituicoes: readonly InstituicaoResumo[];
  onClose: () => void;
  onSaved: () => void;
  onPrecisaInstituicao: () => void;
}

export function ContaSheet({
  conta,
  instituicoes,
  onClose,
  onSaved,
  onPrecisaInstituicao,
}: ContaSheetProps) {
  const editando = Boolean(conta);

  // A instituição atual da conta entra na lista mesmo se foi desativada
  // depois — senão o <select> "pularia" para outro banco ao editar uma conta
  // cuja instituição foi desativada, trocando o dono da conta sem avisar.
  const opcoesInstituicao = instituicoes.filter((i) => i.ativa || i.id === conta?.instituicaoId);

  const [nome, setNome] = useState(conta?.nome ?? "");
  const [instituicaoId, setInstituicaoId] = useState(
    conta?.instituicaoId ?? opcoesInstituicao[0]?.id ?? "",
  );
  const [tipo, setTipo] = useState(conta?.tipo ?? "checking_account");
  const [saldo, setSaldo] = useState(conta ? String(conta.saldo) : "0");
  const [ativa, setAtiva] = useState(conta?.ativa ?? true);
  const [erro, setErro] = useState<string | null>(null);
  const [pendente, startTransition] = useTransition();

  if (opcoesInstituicao.length === 0) {
    return (
      <Sheet title="Nova conta" onClose={onClose}>
        <div style={{ display: "grid", gap: 14 }}>
          <p style={{ fontSize: 13.5, color: "var(--text-3)", lineHeight: 1.5, margin: 0 }}>
            Toda conta pertence a uma instituição. Cadastre a instituição primeiro — leva alguns
            segundos.
          </p>
          <Button variant="primary" size="lg" icon="add" onClick={onPrecisaInstituicao}>
            Cadastrar instituição
          </Button>
        </div>
      </Sheet>
    );
  }

  function salvar() {
    const nomeLimpo = nome.trim();
    if (!nomeLimpo) {
      setErro("Dê um nome para a conta.");
      return;
    }
    if (!instituicaoId) {
      setErro("Escolha a instituição.");
      return;
    }
    setErro(null);
    const saldoNumero = numeroOuNulo(saldo) ?? 0;

    startTransition(async () => {
      try {
        if (conta) {
          await updateFinancialProduct(conta.id, {
            name: nomeLimpo,
            institution_id: instituicaoId,
            type: tipo,
            current_balance: saldoNumero,
            is_active: ativa,
          });
        } else {
          await createFinancialProduct({
            name: nomeLimpo,
            institution_id: instituicaoId,
            type: tipo,
            current_balance: saldoNumero,
            is_active: ativa,
          });
        }
        onSaved();
      } catch (e) {
        setErro(e instanceof Error ? e.message : "Não foi possível salvar.");
      }
    });
  }

  function excluir() {
    if (!conta) return;
    // `deleteFinancialProduct` não tem proteção contra dado ligado (ao
    // contrário de `deleteCard`, que recusa quando há fatura): a exclusão é
    // `ON DELETE CASCADE` até transações, dívidas e cartões desta conta. O
    // aviso precisa dizer isso — "excluir" aqui não é reversível nem contido.
    const aviso =
      `Excluir "${conta.nome}"? Isso apaga PERMANENTEMENTE todas as movimentações, ` +
      "cartões e dívidas ligados a esta conta — não é possível desfazer. Prefira " +
      "desativar (campo acima) se for só parar de usá-la sem perder o histórico.";
    if (!confirm(aviso)) return;
    setErro(null);
    startTransition(async () => {
      try {
        await deleteFinancialProduct(conta.id);
        onSaved();
      } catch (e) {
        setErro(e instanceof Error ? e.message : "Não foi possível excluir.");
      }
    });
  }

  return (
    <Sheet title={editando ? "Editar conta" : "Nova conta"} onClose={onClose}>
      <div style={{ display: "grid", gap: 14 }}>
        <CampoTexto
          rotulo="Nome"
          obrigatorio
          value={nome}
          onChange={setNome}
          placeholder="Ex.: Conta corrente Nubank"
          autoFocus
        />
        <CampoSelecao
          rotulo="Instituição"
          obrigatorio
          value={instituicaoId}
          onChange={setInstituicaoId}
          opcoes={opcoesInstituicao.map((i) => ({ value: i.id, label: i.nome }))}
        />
        <CampoSelecao rotulo="Tipo" obrigatorio value={tipo} onChange={setTipo} opcoes={TIPOS} />
        <CampoNumero
          rotulo="Saldo atual (R$)"
          value={saldo}
          onChange={setSaldo}
          placeholder="0,00"
          step={0.01}
        />
        <CampoAlternavel rotulo="Conta ativa" checked={ativa} onChange={setAtiva} />
        {erro ? <div style={{ fontSize: 12.5, color: "var(--danger)" }}>{erro}</div> : null}
        <div style={{ display: "flex", gap: 10, marginTop: 4 }}>
          <Button variant="outline" size="lg" onClick={onClose} style={{ flex: "1 1 auto" }}>
            Cancelar
          </Button>
          <Button
            variant="primary"
            size="lg"
            onClick={salvar}
            disabled={pendente}
            style={{ flex: "1 1 auto" }}
          >
            {pendente ? "Salvando…" : "Salvar"}
          </Button>
        </div>
        {editando ? (
          <Button variant="quiet" size="sm" icon="delete" onClick={excluir} disabled={pendente}>
            Excluir conta
          </Button>
        ) : null}
      </div>
    </Sheet>
  );
}
