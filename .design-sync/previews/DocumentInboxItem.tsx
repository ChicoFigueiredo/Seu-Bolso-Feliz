import { Card, DocumentInboxItem, formatBRL } from "@sbf/design-system";

export const CaixaDeEntrada = () => (
  <div style={{ maxWidth: 560 }}>
    <Card variant="flush">
      <DocumentInboxItem
        name="hetzner-invoice-2026-08.pdf"
        meta="Gmail (financeiro@) · 21/08 às 06:40"
        value={formatBRL(1000)}
        state="pronto"
      />
      <DocumentInboxItem
        name="Comprovante PIX 22-08.jpg"
        meta="Câmera · hoje, 09:14"
        value={formatBRL(385)}
        state="precisa-revisar"
      />
      <DocumentInboxItem
        name="fatura-nubank-ago.pdf"
        meta="Pasta local · MacBook do Chico"
        state="protegido-por-senha"
      />
      <DocumentInboxItem
        name="adobe-recibo-12-08.pdf"
        meta="Gmail (financeiro@) · 12/08"
        value={formatBRL(119)}
        state="possivel-duplicidade"
      />
      <DocumentInboxItem
        name="scan_20260819.pdf"
        meta="Pasta local · ainda lendo"
        state="processando"
      />
    </Card>
  </div>
);

export const Pronto = () => (
  <div style={{ maxWidth: 560 }}>
    <Card variant="flush">
      <DocumentInboxItem
        name="hetzner-invoice-2026-08.pdf"
        meta="Gmail (financeiro@) · 21/08 às 06:40"
        value={formatBRL(1000)}
        state="pronto"
      />
    </Card>
  </div>
);

export const NaoReconhecido = () => (
  <div style={{ maxWidth: 560 }}>
    <Card variant="flush">
      <DocumentInboxItem
        name="IMG_4471.HEIC"
        meta="Câmera · 19/08 · não consegui ler o valor"
        state="nao-reconhecido"
      />
    </Card>
  </div>
);
