import { Button, Field, Sheet } from "@sbf/design-system";

export const AcoesRapidas = () => (
  <div style={{ width: 420 }}>
    <Sheet title="O que você quer registrar?" withScrim={false}>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <Button variant="outline" size="lg" icon="photo_camera" block>
          Fotografar comprovante
        </Button>
        <Button variant="outline" size="lg" icon="remove_circle_outline" block>
          Registrar despesa
        </Button>
        <Button variant="outline" size="lg" icon="add_circle_outline" block>
          Registrar receita
        </Button>
        <Button variant="outline" size="lg" icon="task_alt" block>
          Marcar pagamento
        </Button>
        <Button variant="outline" size="lg" icon="auto_awesome" block>
          Perguntar à IA
        </Button>
      </div>
    </Sheet>
  </div>
);

export const ConfirmarCaptura = () => (
  <div style={{ width: 420 }}>
    <Sheet title="Confirme o que eu li" withScrim={false}>
      <Field label="Fornecedor" value="CEMIG Distribuição" confidence="alta" />
      <Field label="Valor" value="R$ 385,00" confidence="alta" />
      <Field label="Conta" value="Conta Corrente Caixa" confidence="provavel" divider={false} />
      <div style={{ display: "flex", gap: 8, marginTop: 4 }}>
        <Button variant="primary" size="lg">
          Confirmar
        </Button>
        <Button variant="outline" size="lg">
          Editar detalhes
        </Button>
      </div>
    </Sheet>
  </div>
);

export const ComVeu = () => (
  <div style={{ width: 420, background: "var(--bg)" }}>
    <Sheet title="Como chegamos neste valor?">
      <Field label="Conta Corrente Caixa" value="R$ 2.350" />
      <Field label="Conta Nubank" value="R$ 1.200" />
      <Field label="Conta C6" value="R$ 800" />
      <Field label="Disponível agora" value="R$ 4.350" divider={false} />
    </Sheet>
  </div>
);
