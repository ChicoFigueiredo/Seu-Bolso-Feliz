import { Card, Field } from "@sbf/design-system";

export const ComprovanteExtraido = () => (
  <div style={{ maxWidth: 420 }}>
    <Card>
      <Field label="Fornecedor" value="CEMIG Distribuição" confidence="alta" />
      <Field label="Data" value="22/08/2026" confidence="alta" />
      <Field label="Valor" value="R$ 385,00" confidence="alta" />
      <Field label="Conta" value="Conta Corrente Caixa" confidence="provavel" />
      <Field label="Centro" value="Casa" confidence="provavel" divider={false} />
    </Card>
  </div>
);

export const ConfiancaBaixa = () => (
  <div style={{ maxWidth: 420 }}>
    <Card>
      <Field label="Fornecedor" value="Não reconhecido" confidence="baixa" />
      <Field label="Valor" value="R$ 1.000,00" confidence="alta" />
      <Field label="Centro" value="Edulingo" confidence="baixa" divider={false} />
    </Card>
  </div>
);

export const InformadoPorVoce = () => (
  <div style={{ maxWidth: 420 }}>
    <Card>
      <Field label="Descrição" value="Servidor Hetzner" />
      <Field label="Categoria" value="Infraestrutura" divider={false} />
    </Card>
  </div>
);
