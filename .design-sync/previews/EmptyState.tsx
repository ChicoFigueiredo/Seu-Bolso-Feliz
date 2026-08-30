import { Button, Card, EmptyState } from "@sbf/design-system";

export const CaixaDeEntradaVazia = () => (
  <div style={{ maxWidth: 440 }}>
    <Card>
      <EmptyState
        icon="inbox"
        title="Nada esperando revisão"
        description="Todo documento que chegou do Gmail, do computador ou da câmera já virou lançamento."
      />
    </Card>
  </div>
);

export const SemConexoes = () => (
  <div style={{ maxWidth: 440 }}>
    <Card>
      <EmptyState
        icon="sync_alt"
        title="Nenhuma fonte conectada ainda"
        description="Conecte o Gmail ou aponte uma pasta do computador e os comprovantes passam a chegar sozinhos."
      >
        <Button variant="primary" icon="add">
          Conectar uma fonte
        </Button>
      </EmptyState>
    </Card>
  </div>
);

export const SemResultado = () => (
  <div style={{ maxWidth: 440 }}>
    <Card>
      <EmptyState
        icon="search_off"
        title="Nenhum lançamento neste filtro"
        description="Tente um período maior ou remova o filtro de centro."
      />
    </Card>
  </div>
);
