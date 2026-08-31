"use client";

import { useState, useTransition } from "react";
import { Button, Sheet } from "@sbf/design-system";
import { createInstitution, updateInstitution } from "@/app/actions/institutions";
import type { InstituicaoResumo } from "@/app/actions/painel-contas";
import { CampoAlternavel, CampoSelecao, CampoTexto, type OpcaoCampo } from "./campos";

/**
 * Os seis valores que `institutions_type_check` aceita — conferidos no banco,
 * não em `schema.sql`.
 *
 * `packages/db/schema.sql` é a linha de base do dump e ainda mostra só quatro;
 * `credit_union` e `brokerage` entraram depois, na migration `0003`. Ler o
 * dump e parar por ali é o erro que faz um `<select>` esconder opção válida —
 * ver `packages/db/README.md`.
 */
const TIPOS: readonly OpcaoCampo[] = [
  { value: "bank", label: "Banco" },
  { value: "credit_union", label: "Cooperativa de crédito" },
  { value: "fintech", label: "Fintech" },
  { value: "broker", label: "Corretora" },
  { value: "brokerage", label: "Corretora de valores" },
  { value: "other", label: "Outra" },
];

export interface InstituicaoSheetProps {
  instituicao?: InstituicaoResumo;
  onClose: () => void;
  onSaved: () => void;
}

/**
 * Criar/editar instituição. Sem exclusão de propósito: `institutions` faz
 * `ON DELETE CASCADE` até `financial_products` e, a partir dali, até
 * transações, dívidas e cartões — apagar aqui apagaria o histórico de todas
 * as contas da instituição, em silêncio, sem nenhuma proteção do lado do
 * banco. "Ativa/Inativa" cobre o caso de uso real (parar de usar) sem esse
 * risco.
 */
export function InstituicaoSheet({ instituicao, onClose, onSaved }: InstituicaoSheetProps) {
  const [nome, setNome] = useState(instituicao?.nome ?? "");
  const [tipo, setTipo] = useState(instituicao?.tipo ?? "bank");
  const [ativa, setAtiva] = useState(instituicao?.ativa ?? true);
  const [erro, setErro] = useState<string | null>(null);
  const [pendente, startTransition] = useTransition();

  const editando = Boolean(instituicao);

  function salvar() {
    const nomeLimpo = nome.trim();
    if (!nomeLimpo) {
      setErro("Dê um nome para a instituição.");
      return;
    }
    setErro(null);
    startTransition(async () => {
      try {
        if (instituicao) {
          await updateInstitution(instituicao.id, {
            name: nomeLimpo,
            type: tipo,
            is_active: ativa,
          });
        } else {
          await createInstitution({ name: nomeLimpo, type: tipo, is_active: ativa });
        }
        onSaved();
      } catch (e) {
        setErro(e instanceof Error ? e.message : "Não foi possível salvar.");
      }
    });
  }

  return (
    <Sheet title={editando ? "Editar instituição" : "Nova instituição"} onClose={onClose}>
      <div style={{ display: "grid", gap: 14 }}>
        <CampoTexto
          rotulo="Nome"
          obrigatorio
          value={nome}
          onChange={setNome}
          placeholder="Ex.: Nubank"
          autoFocus
        />
        <CampoSelecao rotulo="Tipo" obrigatorio value={tipo} onChange={setTipo} opcoes={TIPOS} />
        {editando ? (
          <CampoAlternavel rotulo="Instituição ativa" checked={ativa} onChange={setAtiva} />
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
      </div>
    </Sheet>
  );
}
