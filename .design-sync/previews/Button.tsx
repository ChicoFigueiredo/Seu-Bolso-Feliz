import { Button } from "@sbf/design-system";

export const Variantes = () => (
  <div style={{ display: "flex", gap: 10, flexWrap: "wrap", alignItems: "center" }}>
    <Button variant="primary" icon="add">
      Registrar
    </Button>
    <Button variant="secondary" icon="auto_awesome">
      Assistente
    </Button>
    <Button variant="soft">Paguei</Button>
    <Button variant="outline">Dispensar</Button>
    <Button variant="quiet">Ver tudo</Button>
  </div>
);

export const Tamanhos = () => (
  <div style={{ display: "flex", gap: 10, flexWrap: "wrap", alignItems: "center" }}>
    <Button variant="primary" size="sm">
      Paguei
    </Button>
    <Button variant="primary" size="md">
      Confirmar recorrência
    </Button>
    <Button variant="primary" size="lg" icon="add">
      Registrar
    </Button>
  </div>
);

export const SobreOHero = () => (
  <div
    style={{
      background: "var(--hero-bg)",
      borderRadius: 22,
      padding: 24,
      display: "flex",
      gap: 10,
    }}
  >
    <Button variant="onHero">Como chegamos neste valor?</Button>
  </div>
);

export const Desabilitado = () => (
  <div style={{ display: "flex", gap: 10, flexWrap: "wrap", alignItems: "center" }}>
    <Button variant="primary" disabled>
      Sincronizar agora
    </Button>
    <Button variant="soft" disabled>
      Paguei
    </Button>
  </div>
);

export const LarguraTotal = () => (
  <div style={{ maxWidth: 320 }}>
    <Button variant="primary" size="lg" icon="photo_camera" block>
      Capturar comprovante
    </Button>
  </div>
);
