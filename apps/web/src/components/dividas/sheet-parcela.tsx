"use client";

/**
 * Folha de criação/edição de uma parcela.
 *
 * Marcar uma parcela como paga (pelo botão rápido ou trocando a situação
 * neste formulário para "Paga") sempre passa por `marcarParcelaComoPaga`, que
 * também abate o saldo devedor da dívida — nunca por `updateLiabilityInstallment`
 * puro, que só mexe na parcela. Qualquer outra edição de campo usa
 * `updateLiabilityInstallment` (existente), sem cascata.
 */

import { useState } from "react";
import { Button, Card, Sheet, formatBRL } from "@sbf/design-system";
import {
  createLiabilityInstallment,
  deleteLiabilityInstallment,
  updateLiabilityInstallment,
} from "@/app/actions/liabilities";
import { marcarParcelaComoPaga, type ParcelaDetalhe } from "@/app/actions/painel-dividas";
import { STATUS_PARCELA_LABEL } from "./labels";
import styles from "./dividas.module.css";
import { hojeIso } from "@/lib/datas";

export interface SheetParcelaProps {
  liabilityId: string;
  onClose: () => void;
  onSalvo: () => void;
  parcelaExistente?: ParcelaDetalhe;
  proximoNumeroSugerido?: number;
}

export function SheetParcela({
  liabilityId,
  onClose,
  onSalvo,
  parcelaExistente,
  proximoNumeroSugerido,
}: SheetParcelaProps) {
  const editando = parcelaExistente != null;

  const [numero, setNumero] = useState(
    parcelaExistente ? String(parcelaExistente.numero) : String(proximoNumeroSugerido ?? 1),
  );
  const [vencimento, setVencimento] = useState(parcelaExistente?.vencimento ?? "");
  const [valor, setValor] = useState(parcelaExistente ? String(parcelaExistente.valor) : "");
  const [statusParcela, setStatusParcela] = useState(parcelaExistente?.status ?? "pending");
  const [valorPago, setValorPago] = useState(
    parcelaExistente && parcelaExistente.valorPago > 0 ? String(parcelaExistente.valorPago) : "",
  );
  const [dataPagamento, setDataPagamento] = useState(parcelaExistente?.dataPagamento ?? "");

  const [salvando, setSalvando] = useState(false);
  const [erro, setErro] = useState<string | null>(null);
  const [confirmandoExclusao, setConfirmandoExclusao] = useState(false);
  const [excluindo, setExcluindo] = useState(false);
  const [marcandoPaga, setMarcandoPaga] = useState(false);

  async function marcarPagaRapido() {
    if (!parcelaExistente) return;
    setErro(null);
    setMarcandoPaga(true);
    try {
      await marcarParcelaComoPaga(parcelaExistente.id, {
        valorPago: parcelaExistente.valor,
        dataPagamento: hojeIso(),
      });
      onSalvo();
    } catch (e) {
      setErro(e instanceof Error ? e.message : "Não deu pra marcar a parcela como paga.");
      setMarcandoPaga(false);
    }
  }

  async function salvar() {
    setErro(null);

    const numeroNum = Number.parseInt(numero, 10);
    if (!Number.isFinite(numeroNum) || numeroNum <= 0) {
      return setErro("O número da parcela precisa ser maior que zero.");
    }
    if (!vencimento) return setErro("Informe a data de vencimento.");
    const valorNum = Number(valor);
    if (!Number.isFinite(valorNum) || valorNum <= 0) {
      return setErro("O valor da parcela precisa ser maior que zero.");
    }

    setSalvando(true);
    try {
      if (!editando) {
        // Se a parcela já nasce "paga" (backfill de histórico), cria como
        // pendente e imediatamente passa pela mesma cascata do botão rápido —
        // criar direto como paga puxando `createLiabilityInstallment` sozinho
        // deixaria o saldo devedor da dívida sem abater.
        const nasceQuitada = statusParcela === "paid";
        const criada = await createLiabilityInstallment({
          liability_id: liabilityId,
          installment_number: numeroNum,
          due_date: vencimento,
          total_amount: valorNum,
          status: nasceQuitada ? "pending" : statusParcela,
          paid_amount: !nasceQuitada && valorPago.trim() ? Number(valorPago) : undefined,
          paid_date: !nasceQuitada && dataPagamento.trim() ? dataPagamento : undefined,
        });
        if (nasceQuitada) {
          await marcarParcelaComoPaga(criada.id, {
            valorPago: valorPago.trim() ? Number(valorPago) : valorNum,
            dataPagamento: dataPagamento.trim() ? dataPagamento : hojeIso(),
          });
        }
      } else if (statusParcela === "paid" && parcelaExistente.status !== "paid") {
        // Transição pra "paga" pelo formulário: mesma cascata do botão rápido,
        // pra não existir um jeito de marcar como paga que esqueça de abater
        // o saldo devedor.
        await marcarParcelaComoPaga(parcelaExistente.id, {
          valorPago: valorPago.trim() ? Number(valorPago) : valorNum,
          dataPagamento: dataPagamento.trim() ? dataPagamento : hojeIso(),
        });
        if (valorNum !== parcelaExistente.valor || vencimento !== parcelaExistente.vencimento) {
          await updateLiabilityInstallment(parcelaExistente.id, {
            installment_number: numeroNum,
            due_date: vencimento,
            total_amount: valorNum,
          });
        }
      } else {
        await updateLiabilityInstallment(parcelaExistente.id, {
          installment_number: numeroNum,
          due_date: vencimento,
          total_amount: valorNum,
          status: statusParcela,
          paid_amount: valorPago.trim() ? Number(valorPago) : 0,
          paid_date: dataPagamento.trim() ? dataPagamento : null,
        });
      }
      onSalvo();
    } catch (e) {
      setErro(e instanceof Error ? e.message : "Não deu pra salvar a parcela.");
    } finally {
      setSalvando(false);
    }
  }

  async function excluir() {
    if (!parcelaExistente) return;
    setExcluindo(true);
    setErro(null);
    try {
      await deleteLiabilityInstallment(parcelaExistente.id);
      onSalvo();
    } catch (e) {
      setErro(e instanceof Error ? e.message : "Não deu pra excluir a parcela.");
      setExcluindo(false);
    }
  }

  return (
    <div className={styles.sheetOverlay}>
      <Sheet
        title={editando ? `Parcela ${parcelaExistente.numero}` : "Nova parcela"}
        onClose={onClose}
        style={{ maxHeight: "92vh", overflowY: "auto" }}
      >
        {erro ? <div className={styles.formError}>{erro}</div> : null}

        {editando && parcelaExistente.status !== "paid" ? (
          <Button
            variant="soft"
            size="lg"
            icon="check_circle"
            onClick={marcarPagaRapido}
            disabled={marcandoPaga || salvando || excluindo}
          >
            {marcandoPaga
              ? "Marcando…"
              : `Marcar como paga hoje · ${formatBRL(parcelaExistente.valor, { cents: true })}`}
          </Button>
        ) : null}

        <div className={styles.formGrid}>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="parcela-numero">
              Número
            </label>
            <input
              id="parcela-numero"
              className={styles.formInput}
              type="number"
              min="1"
              step="1"
              value={numero}
              onChange={(e) => setNumero(e.target.value)}
            />
          </div>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="parcela-vencimento">
              Vencimento
            </label>
            <input
              id="parcela-vencimento"
              className={styles.formInput}
              type="date"
              value={vencimento}
              onChange={(e) => setVencimento(e.target.value)}
            />
          </div>
        </div>

        <div className={styles.formField}>
          <label className={styles.formLabel} htmlFor="parcela-valor">
            Valor total (R$)
          </label>
          <input
            id="parcela-valor"
            className={styles.formInput}
            type="number"
            step="0.01"
            min="0"
            value={valor}
            onChange={(e) => setValor(e.target.value)}
          />
        </div>

        <div className={styles.formField}>
          <label className={styles.formLabel} htmlFor="parcela-status">
            Situação
          </label>
          <select
            id="parcela-status"
            className={styles.formSelect}
            value={statusParcela}
            onChange={(e) => setStatusParcela(e.target.value)}
          >
            {Object.entries(STATUS_PARCELA_LABEL).map(([valorOpcao, rotulo]) => (
              <option key={valorOpcao} value={valorOpcao}>
                {rotulo}
              </option>
            ))}
          </select>
        </div>

        <div className={styles.formGrid}>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="parcela-valor-pago">
              Valor pago (R$)
            </label>
            <input
              id="parcela-valor-pago"
              className={styles.formInput}
              type="number"
              step="0.01"
              min="0"
              value={valorPago}
              onChange={(e) => setValorPago(e.target.value)}
            />
          </div>
          <div className={styles.formField}>
            <label className={styles.formLabel} htmlFor="parcela-data-pagamento">
              Data do pagamento
            </label>
            <input
              id="parcela-data-pagamento"
              className={styles.formInput}
              type="date"
              value={dataPagamento}
              onChange={(e) => setDataPagamento(e.target.value)}
            />
          </div>
        </div>

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
                Excluir parcela
              </Button>
            ) : (
              <>
                <span style={{ fontSize: 13, color: "var(--text-2)" }}>
                  Não dá pra desfazer.
                  {parcelaExistente.status === "paid"
                    ? " Como essa parcela já está paga, excluí-la não devolve o valor ao saldo devedor automaticamente."
                    : ""}
                </span>
                <div className={styles.formActions}>
                  <Button
                    variant="outline"
                    size="lg"
                    onClick={() => setConfirmandoExclusao(false)}
                    disabled={excluindo}
                  >
                    Manter parcela
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
