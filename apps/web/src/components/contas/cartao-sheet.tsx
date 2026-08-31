"use client";

import { useState, useTransition } from "react";
import { Button, Sheet } from "@sbf/design-system";
import { createCard, deleteCard, updateCard } from "@/app/actions/cards";
import {
  createFinancialProduct,
  deleteFinancialProduct,
  updateFinancialProduct,
} from "@/app/actions/financial-products";
import type { CartaoDetalhe, InstituicaoResumo } from "@/app/actions/painel-contas";
import {
  CampoAlternavel,
  CampoNumero,
  CampoSelecao,
  CampoTexto,
  numeroOuNulo,
  textoOuNulo,
} from "./campos";

export interface CartaoSheetProps {
  cartao?: CartaoDetalhe;
  instituicoes: readonly InstituicaoResumo[];
  onClose: () => void;
  onSaved: () => void;
  onPrecisaInstituicao: () => void;
}

/**
 * Criar/editar cartão.
 *
 * Um cartão, no banco, são duas linhas: um `financial_products` (tipo
 * `credit_card` — é ele que carrega nome e limite) e um `cards` (o cartão
 * físico/virtual em si — bandeira, final, fechamento, vencimento), ligados
 * por `financial_product_id`. O fluxo antigo (`/dashboard/cards/new`) exige
 * que o produto já exista; aqui as duas linhas nascem juntas, numa única
 * folha, porque é isso que "cadastrar um cartão que eu tenho" quer dizer para
 * quem está preenchendo o formulário. Anexar um cartão adicional a um produto
 * já existente continua possível pelo fluxo antigo.
 *
 * O limite mora em `financial_products.credit_limit` (não em
 * `cards.credit_limit`, que existe mas é uma coluna paralela e hoje
 * desconectada de tudo que lê "limite" nesta tela e na tela Hoje — ver
 * relatório final) — por isso editar limite aqui atualiza o produto, não o
 * cartão.
 */
export function CartaoSheet({
  cartao,
  instituicoes,
  onClose,
  onSaved,
  onPrecisaInstituicao,
}: CartaoSheetProps) {
  const editando = Boolean(cartao);

  const [nome, setNome] = useState(cartao?.nome ?? "");
  const [instituicaoId, setInstituicaoId] = useState(instituicoes.find((i) => i.ativa)?.id ?? "");
  const [last4, setLast4] = useState(cartao?.last4 ?? "");
  const [brand, setBrand] = useState(cartao?.brand ?? "");
  const [holderName, setHolderName] = useState(cartao?.holderName ?? "");
  const [limite, setLimite] = useState(
    cartao?.limiteTotal != null ? String(cartao.limiteTotal) : "",
  );
  const [closingDay, setClosingDay] = useState(
    cartao?.closingDay != null ? String(cartao.closingDay) : "",
  );
  const [dueDay, setDueDay] = useState(cartao?.dueDay != null ? String(cartao.dueDay) : "");
  const [isPrimary, setIsPrimary] = useState(cartao?.isPrimary ?? true);
  const [ativo, setAtivo] = useState(cartao?.ativo ?? true);
  const [erro, setErro] = useState<string | null>(null);
  const [pendente, startTransition] = useTransition();

  const instituicoesAtivas = instituicoes.filter((i) => i.ativa);

  if (!editando && instituicoesAtivas.length === 0) {
    return (
      <Sheet title="Novo cartão" onClose={onClose}>
        <div style={{ display: "grid", gap: 14 }}>
          <p style={{ fontSize: 13.5, color: "var(--text-3)", lineHeight: 1.5, margin: 0 }}>
            Todo cartão pertence a uma instituição. Cadastre a instituição primeiro — leva alguns
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
      setErro("Dê um nome para o cartão.");
      return;
    }
    if (!editando && !instituicaoId) {
      setErro("Escolha a instituição.");
      return;
    }
    setErro(null);
    const limiteNumero = numeroOuNulo(limite);
    const closingDayNumero = numeroOuNulo(closingDay);
    const dueDayNumero = numeroOuNulo(dueDay);

    startTransition(async () => {
      try {
        if (cartao) {
          await updateFinancialProduct(cartao.financialProductId, {
            name: nomeLimpo,
            credit_limit: limiteNumero,
          });
          await updateCard(cartao.id, {
            last_four_digits: textoOuNulo(last4),
            card_brand: textoOuNulo(brand),
            holder_name: textoOuNulo(holderName),
            closing_day: closingDayNumero,
            due_day: dueDayNumero,
            is_primary: isPrimary,
            is_active: ativo,
          });
        } else {
          const produto = await createFinancialProduct({
            name: nomeLimpo,
            institution_id: instituicaoId,
            type: "credit_card",
            current_balance: 0,
            credit_limit: limiteNumero,
          });
          try {
            await createCard({
              financial_product_id: produto.id,
              last_four_digits: textoOuNulo(last4),
              card_brand: textoOuNulo(brand),
              holder_name: textoOuNulo(holderName),
              closing_day: closingDayNumero,
              due_day: dueDayNumero,
              is_primary: isPrimary,
              is_active: true,
            });
          } catch (erroCartao) {
            // Sem o `cards`, o `financial_products` sozinho não aparece em
            // canto nenhum desta tela — desfaz para não deixar um produto
            // órfão para trás. Falha aqui é só registrada, nunca some o erro
            // original.
            await deleteFinancialProduct(produto.id).catch(() => {});
            throw erroCartao;
          }
        }
        onSaved();
      } catch (e) {
        setErro(e instanceof Error ? e.message : "Não foi possível salvar.");
      }
    });
  }

  function excluir() {
    if (!cartao) return;
    // `deleteCard` já recusa quando há fatura ligada (ver cards.ts) — o
    // confirm aqui não precisa do aviso extra de cascata que `ContaSheet` e
    // `InstituicaoSheet` carregam.
    if (!confirm(`Excluir o cartão "${cartao.nome}"? Essa ação não pode ser desfeita.`)) return;
    setErro(null);
    startTransition(async () => {
      try {
        await deleteCard(cartao.id);
        onSaved();
      } catch (e) {
        setErro(e instanceof Error ? e.message : "Não foi possível excluir.");
      }
    });
  }

  return (
    <Sheet
      title={editando ? "Editar cartão" : "Novo cartão"}
      onClose={onClose}
      style={{ maxHeight: "92%", overflowY: "auto" }}
    >
      <div style={{ display: "grid", gap: 14 }}>
        <CampoTexto
          rotulo="Nome"
          obrigatorio
          value={nome}
          onChange={setNome}
          placeholder="Ex.: Cartão Nubank"
          autoFocus
        />
        {editando ? (
          cartao?.instituicao ? (
            <div style={{ fontSize: 12.5, color: "var(--text-3)" }}>
              Instituição: <strong style={{ color: "var(--text-2)" }}>{cartao.instituicao}</strong>
            </div>
          ) : null
        ) : (
          <CampoSelecao
            rotulo="Instituição"
            obrigatorio
            value={instituicaoId}
            onChange={setInstituicaoId}
            opcoes={instituicoesAtivas.map((i) => ({ value: i.id, label: i.nome }))}
          />
        )}
        <div style={{ display: "grid", gap: 14, gridTemplateColumns: "1fr 1fr" }}>
          <CampoTexto
            rotulo="Últimos 4 dígitos"
            value={last4}
            onChange={setLast4}
            placeholder="1234"
            maxLength={4}
          />
          <CampoTexto
            rotulo="Bandeira"
            value={brand}
            onChange={setBrand}
            placeholder="Visa, Mastercard…"
          />
        </div>
        <CampoTexto rotulo="Nome impresso no cartão" value={holderName} onChange={setHolderName} />
        <div style={{ display: "grid", gap: 14, gridTemplateColumns: "1fr 1fr 1fr" }}>
          <CampoNumero
            rotulo="Limite (R$)"
            value={limite}
            onChange={setLimite}
            placeholder="5000"
            step={0.01}
          />
          <CampoNumero
            rotulo="Fechamento"
            value={closingDay}
            onChange={setClosingDay}
            placeholder="20"
            min={1}
            max={31}
            step={1}
          />
          <CampoNumero
            rotulo="Vencimento"
            value={dueDay}
            onChange={setDueDay}
            placeholder="28"
            min={1}
            max={31}
            step={1}
          />
        </div>
        <CampoAlternavel rotulo="Cartão principal" checked={isPrimary} onChange={setIsPrimary} />
        {editando ? (
          <CampoAlternavel rotulo="Cartão ativo" checked={ativo} onChange={setAtivo} />
        ) : null}
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
            Excluir cartão
          </Button>
        ) : null}
      </div>
    </Sheet>
  );
}
