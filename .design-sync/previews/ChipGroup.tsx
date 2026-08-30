import { ChipGroup } from "@sbf/design-system";

const filtros = [
  { value: "todas", label: "Todas", count: 9 },
  { value: "pendentes", label: "Pendentes", count: 4 },
  { value: "pagas", label: "Pagas", count: 5 },
  { value: "receitas", label: "Receitas" },
  { value: "cartao", label: "Cartão" },
];

export const FiltrosDeMovimentacoes = () => (
  <div style={{ maxWidth: 520 }}>
    <ChipGroup options={filtros} value="todas" />
  </div>
);

export const OutraSelecao = () => (
  <div style={{ maxWidth: 520 }}>
    <ChipGroup options={filtros} value="pendentes" />
  </div>
);

export const SemContadores = () => (
  <div style={{ maxWidth: 520 }}>
    <ChipGroup
      options={[
        { value: "edulingo", label: "Edulingo" },
        { value: "imobmax", label: "Imob Max" },
        { value: "ia", label: "IA" },
        { value: "casa", label: "Casa" },
      ]}
      value="edulingo"
    />
  </div>
);
