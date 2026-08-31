"use client";

/**
 * Folha de criação/edição de uma dívida.
 *
 * Sem `dividaExistente`: cria. Com `dividaExistente`: edita e permite excluir
 * (com confirmação inline, sem empilhar uma segunda folha). O `<select>` de
 * produto financeiro existe porque `liabilities.financial_product_id` é
 * `NOT NULL` no banco — não dá pra cadastrar uma dívida sem apontar pra um
 * produto já existente (ver packages/db/schema.sql).
 */

import { useEffect, useState } from "react";
import { Button, Card, Sheet } from "@sbf/design-system";
import { createLiability, updateLiability, deleteLiability } from "@/app/actions/liabilities";
import { getFinancialProducts } from "@/app/actions/financial-products";
import { getInstitutions } from "@/app/actions/institutions";
import type { DetalheDivida } from "@/app/actions/painel-dividas";
import {
  SISTEMA_AMORTIZACAO_LABEL,
  STATUS_DIVIDA_LABEL,
  TIPO_DIVIDA_LABEL,
  TIPO_TAXA_LABEL,
} from "./labels";
import styles from "./dividas.module.css";

export interface SheetDividaProps {
  onClose: () => void;
  /** Chamado depois que criar/salvar/excluir dá certo — o chamador decide se navega e/ou revalida. */
  onSalvo: () => void;
  dividaExistente?: DetalheDivida;
}

interface ProdutoOpcao {
  id: string;
  rotulo: string;
}

export function SheetDivida({ onClose, onSalvo, dividaExistente }: SheetDividaProps) {
  const editando = dividaExistente != null;

  const [produtos, setProdutos] = useState<ProdutoOpcao[] | null>(null);
  const [nome, setNome] = useState(dividaExistente?.nome ?? "");
  const [tipo, setTipo] = useState(dividaExistente?.tipo ?? "personal_loan");
  const [financialProductId, setFinancialProductId] = useState(
    dividaExistente?.financialProductId ?? "",
  );
  const [original, setOriginal] = useState(
    dividaExistente ? String(dividaExistente.valorOriginal) : "",
  );
  const [saldo, setSaldo] = useState(dividaExistente ? String(dividaExistente.saldoDevedor) : "");
  const [taxaPct, setTaxaPct] = useState(
    dividaExistente?.taxa ? String(Number((dividaExistente.taxa.valor * 100).toFixed(6))) : "",
  );
  const [tipoTaxa, setTipoTaxa] = useState(dividaExistente?.taxa?.tipo ?? "monthly");
  const [sistema, setSistema] = useState(dividaExistente?.sistemaAmortizacao ?? "");
  const [totalParcelas, setTotalParcelas] = useState(
    dividaExistente?.parcelasTotais != null ? String(dividaExistente.parcelasTotais) : "",
  );
  const [parcelasPagas, setParcelasPagas] = useState(
    dividaExistente ? String(dividaExistente.parcelasPagas) : "0",
  );
  const [inicio, setInicio] = useState(dividaExistente?.inicio ?? "");
  const [fim, setFim] = useState(dividaExistente?.fimGravado ?? "");
  const [status, setStatus] = useState(dividaExistente?.status ?? "active");

  const [salvando, setSalvando] = useState(false);
  const [erro, setErro] = useState<string | null>(null);
  const [confirmandoExclusao, setConfirmandoExclusao] = useState(false);
  const [excluindo, setExcluindo] = useState(false);

  useEffect(() => {
    let cancelado = false;
    Promise.all([getFinancialProducts(), getInstitutions()])
      .then(([prods, instituicoes]) => {
        if (cancelado) return;
        const nomeInstituicao = new Map(instituicoes.map((i) => [i.id, i.name]));
        setProdutos(
          prods.map((p) => ({
            id: p.id,
            rotulo: [nomeInstituicao.get(p.institution_id), p.name].filter(Boolean).join(" — "),
          })),
        );
      })
      .catch(() => {
        if (!cancelado) setProdutos([]);
      });
    return () => {
      cancelado = true;
    };
  }, []);

  async function salvar() {
    setErro(null);

    if (!nome.trim()) return setErro("Dê um nome pra essa dívida.");
    if (!financialProductId) return setErro("Escolha a conta ou cartão em que essa dívida está.");
    const originalNum = Number(original);
    if (!Number.isFinite(originalNum) || originalNum <= 0) {
      return setErro("O valor original precisa ser maior que zero.");
    }
    const saldoNum = Number(saldo);
    if (!Number.isFinite(saldoNum) || saldoNum < 0) {
      return setErro("O saldo devedor não pode ser negativo.");
    }

    const totalParcelasNum = totalParcelas.trim() ? Number.parseInt(totalParcelas, 10) : null;
    if (
      totalParcelas.trim() &&
      (!Number.isFinite(totalParcelasNum) || (totalParcelasNum ?? 0) <= 0)
    ) {
      return setErro("O total de parcelas precisa ser um número inteiro maior que zero.");
    }
    const parcelasPagasNum = parcelasPagas.trim() ? Number.parseInt(parcelasPagas, 10) : 0;
    if (!Number.isFinite(parcelasPagasNum) || parcelasPagasNum < 0) {
      return setErro("Parcelas pagas não pode ser negativo.");
    }

    const payload = {
      name: nome.trim(),
      type: tipo,
      financial_product_id: financialProductId,
      original_amount: originalNum,
      outstanding_balance: saldoNum,
      interest_rate: taxaPct.trim() ? Number(taxaPct) / 100 : null,
      rate_type: taxaPct.trim() ? tipoTaxa : null,
      amortization_system: sistema.trim() ? sistema : null,
      total_installments: totalParcelasNum,
      paid_installments: parcelasPagasNum,
      start_date: inicio.trim() ? inicio : null,
      end_date: fim.trim() ? fim : null,
    };

    setSalvando(true);
    try {
      if (editando) {
        await updateLiability(dividaExistente.id, { ...payload, status });
      } else {
        await createLiability(payload);
      }
      onSalvo();
    } catch (e) {
      setErro(e instanceof Error ? e.message : "Não deu pra salvar a dívida.");
    } finally {
      setSalvando(false);
    }
  }

  async function excluir() {
    if (!dividaExistente) return;
    setExcluindo(true);
    setErro(null);
    try {
      await deleteLiability(dividaExistente.id);
      onSalvo();
    } catch (e) {
      setErro(e instanceof Error ? e.message : "Não deu pra excluir a dívida.");
      setExcluindo(false);
    }
  }

  const semProdutos = produtos != null && produtos.length === 0;

  return (
    <div className={styles.sheetOverlay}>
      <Sheet
        title={editando ? "Editar dívida" : "Nova dívida"}
        onClose={onClose}
        style={{ maxHeight: "92vh", overflowY: "auto" }}
      >
        {erro ? <div className={styles.formError}>{erro}</div> : null}

        {semProdutos ? (
          <div className={styles.formError} style={{ color: "var(--text-2)" }}>
            Você ainda não tem nenhuma conta ou cartão cadastrado. Cadastre um em Contas antes de
            registrar uma dívida — toda dívida precisa estar ligada a um produto financeiro.
          </div>
        ) : null}

        <div className={styles.formField}>
          <label className={styles.formLabel} htmlFor="divida-nome">
            Nome
          </label>
          <input
            id="divida-nome"
            className={styles.formInput}
            value={nome}
            onChange={(e) => setNome(e.target.value)}
            placeholder="Ex.: Financiamento do carro"
          />
        </div>

        <div className={styles.formGrid}>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="divida-tipo">
              Tipo
            </label>
            <select
              id="divida-tipo"
              className={styles.formSelect}
              value={tipo}
              onChange={(e) => setTipo(e.target.value)}
            >
              {Object.entries(TIPO_DIVIDA_LABEL).map(([valor, rotulo]) => (
                <option key={valor} value={valor}>
                  {rotulo}
                </option>
              ))}
            </select>
          </div>

          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="divida-produto">
              Conta ou cartão
            </label>
            <select
              id="divida-produto"
              className={styles.formSelect}
              value={financialProductId}
              onChange={(e) => setFinancialProductId(e.target.value)}
              disabled={produtos == null}
            >
              <option value="">{produtos == null ? "Carregando…" : "Selecione"}</option>
              {(produtos ?? []).map((p) => (
                <option key={p.id} value={p.id}>
                  {p.rotulo}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className={styles.formGrid}>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="divida-original">
              Valor original (R$)
            </label>
            <input
              id="divida-original"
              className={styles.formInput}
              type="number"
              step="0.01"
              min="0"
              value={original}
              onChange={(e) => setOriginal(e.target.value)}
            />
          </div>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="divida-saldo">
              Saldo devedor atual (R$)
            </label>
            <input
              id="divida-saldo"
              className={styles.formInput}
              type="number"
              step="0.01"
              min="0"
              value={saldo}
              onChange={(e) => setSaldo(e.target.value)}
            />
          </div>
        </div>

        <div className={styles.formGrid}>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="divida-taxa">
              Taxa de juros (%)
            </label>
            <input
              id="divida-taxa"
              className={styles.formInput}
              type="number"
              step="0.01"
              min="0"
              placeholder="Ex.: 1,49"
              value={taxaPct}
              onChange={(e) => setTaxaPct(e.target.value)}
            />
          </div>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="divida-tipo-taxa">
              Período da taxa
            </label>
            <select
              id="divida-tipo-taxa"
              className={styles.formSelect}
              value={tipoTaxa}
              onChange={(e) => setTipoTaxa(e.target.value)}
              disabled={!taxaPct.trim()}
            >
              {Object.entries(TIPO_TAXA_LABEL).map(([valor, rotulo]) => (
                <option key={valor} value={valor}>
                  {rotulo}
                </option>
              ))}
            </select>
          </div>
        </div>

        <div className={styles.formField}>
          <label className={styles.formLabel} htmlFor="divida-sistema">
            Sistema de amortização
          </label>
          <select
            id="divida-sistema"
            className={styles.formSelect}
            value={sistema}
            onChange={(e) => setSistema(e.target.value)}
          >
            <option value="">Não especificado</option>
            {Object.entries(SISTEMA_AMORTIZACAO_LABEL).map(([valor, rotulo]) => (
              <option key={valor} value={valor}>
                {rotulo}
              </option>
            ))}
          </select>
          <span className={styles.formHint}>
            Define o formato do cronograma gerado automaticamente na tela da dívida.
          </span>
        </div>

        <div className={styles.formGrid}>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="divida-total-parcelas">
              Total de parcelas
            </label>
            <input
              id="divida-total-parcelas"
              className={styles.formInput}
              type="number"
              min="1"
              step="1"
              value={totalParcelas}
              onChange={(e) => setTotalParcelas(e.target.value)}
            />
          </div>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="divida-parcelas-pagas">
              Parcelas já pagas
            </label>
            <input
              id="divida-parcelas-pagas"
              className={styles.formInput}
              type="number"
              min="0"
              step="1"
              value={parcelasPagas}
              onChange={(e) => setParcelasPagas(e.target.value)}
            />
          </div>
        </div>

        <div className={styles.formGrid}>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="divida-inicio">
              Início
            </label>
            <input
              id="divida-inicio"
              className={styles.formInput}
              type="date"
              value={inicio}
              onChange={(e) => setInicio(e.target.value)}
            />
          </div>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="divida-fim">
              Término (se souber)
            </label>
            <input
              id="divida-fim"
              className={styles.formInput}
              type="date"
              value={fim}
              onChange={(e) => setFim(e.target.value)}
            />
          </div>
        </div>

        {editando ? (
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="divida-status">
              Situação
            </label>
            <select
              id="divida-status"
              className={styles.formSelect}
              value={status}
              onChange={(e) => setStatus(e.target.value)}
            >
              {Object.entries(STATUS_DIVIDA_LABEL).map(([valor, rotulo]) => (
                <option key={valor} value={valor}>
                  {rotulo}
                </option>
              ))}
            </select>
          </div>
        ) : null}

        <div className={styles.formActions}>
          <Button variant="outline" size="lg" onClick={onClose} disabled={salvando || excluindo}>
            Cancelar
          </Button>
          <Button variant="primary" size="lg" onClick={salvar} disabled={salvando || excluindo}>
            {salvando ? "Salvando…" : "Salvar"}
          </Button>
        </div>

        {editando ? (
          <Card variant="quiet" style={{ gap: 10 }}>
            {!confirmandoExclusao ? (
              <Button
                variant="quiet"
                size="lg"
                icon="delete"
                style={{ color: "var(--danger)", alignSelf: "flex-start" }}
                onClick={() => setConfirmandoExclusao(true)}
                disabled={salvando || excluindo}
              >
                Excluir dívida
              </Button>
            ) : (
              <>
                <span style={{ fontSize: 13, color: "var(--text-2)" }}>
                  Excluir "{dividaExistente.nome}" remove também as{" "}
                  {dividaExistente.parcelas.length} parcela
                  {dividaExistente.parcelas.length === 1 ? "" : "s"} cadastradas. Não dá pra
                  desfazer.
                </span>
                <div className={styles.formActions}>
                  <Button
                    variant="outline"
                    size="lg"
                    onClick={() => setConfirmandoExclusao(false)}
                    disabled={excluindo}
                  >
                    Manter dívida
                  </Button>
                  <Button
                    variant="primary"
                    size="lg"
                    style={{ background: "var(--danger)" }}
                    onClick={excluir}
                    disabled={excluindo}
                  >
                    {excluindo ? "Excluindo…" : "Confirmar exclusão"}
                  </Button>
                </div>
              </>
            )}
          </Card>
        ) : null}
      </Sheet>
    </div>
  );
}
