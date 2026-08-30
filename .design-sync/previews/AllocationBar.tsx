import { AllocationBar, Card, formatBRL } from "@sbf/design-system";

export const ServidorDivididoEmTres = () => (
  <div style={{ maxWidth: 420 }}>
    <Card>
      <div style={{ fontSize: 14.5, fontWeight: 700, color: "var(--text)" }}>
        Hetzner Online · {formatBRL(1000)}
      </div>
      <AllocationBar
        allocations={[
          { name: "Edulingo", value: formatBRL(600), share: 0.6, color: "var(--primary)" },
          { name: "Imob Max", value: formatBRL(250), share: 0.25, color: "var(--accent)" },
          {
            name: "Seu Bolso Feliz",
            value: formatBRL(150),
            share: 0.15,
            color: "var(--secondary)",
          },
        ]}
      />
    </Card>
  </div>
);

export const DivisaoIncompleta = () => (
  <div style={{ maxWidth: 420 }}>
    <Card>
      <div style={{ fontSize: 14.5, fontWeight: 700, color: "var(--text)" }}>
        Assinatura Adobe · {formatBRL(119)}
      </div>
      <AllocationBar
        title="Divisão parcial — 30% ainda sem centro"
        allocations={[
          { name: "Edulingo", value: formatBRL(83), share: 0.7, color: "var(--primary)" },
        ]}
      />
    </Card>
  </div>
);

export const DuasFrentes = () => (
  <div style={{ maxWidth: 420 }}>
    <Card>
      <div style={{ fontSize: 14.5, fontWeight: 700, color: "var(--text)" }}>
        Contador · {formatBRL(800)}
      </div>
      <AllocationBar
        allocations={[
          { name: "Edulingo", value: formatBRL(400), share: 0.5, color: "var(--primary)" },
          { name: "Imob Max", value: formatBRL(400), share: 0.5, color: "var(--secondary)" },
        ]}
      />
    </Card>
  </div>
);
