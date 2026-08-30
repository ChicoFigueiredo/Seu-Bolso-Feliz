import { AIReviewItem } from "@sbf/design-system";

export const AumentoAtipico = () => (
  <div style={{ maxWidth: 560 }}>
    <AIReviewItem
      icon="trending_up"
      title="A cobrança da Hetzner subiu 34%"
      body="Os últimos três meses ficaram em torno de R$ 745. A cobrança de agosto veio R$ 1.000. Pode ser um servidor novo ou reajuste — abrir os lançamentos mostra o que mudou."
      actionLabel="Ver lançamentos"
      confidence="confiança alta"
    />
  </div>
);

export const PossivelDuplicidade = () => (
  <div style={{ maxWidth: 560 }}>
    <AIReviewItem
      icon="content_copy"
      title="Duas cobranças iguais da Adobe em 12/08"
      body="Encontrei R$ 119,00 lançados duas vezes no mesmo dia, no mesmo cartão. Se foi cobrança em duplicidade, dá para marcar uma delas para contestação."
      actionLabel="Comparar as duas"
      dismissLabel="São diferentes"
      confidence="confiança média"
    />
  </div>
);

export const NovaRecorrencia = () => (
  <div style={{ maxWidth: 560 }}>
    <AIReviewItem
      title="A Hetzner parece ser uma recorrência"
      body="Apareceu nos últimos 6 meses, sempre entre os dias 24 e 26, com valor próximo de R$ 1.000. Posso passar a acompanhá-la como recorrente e prever o próximo valor."
      actionLabel="Acompanhar como recorrente"
      confidence="6 de 6 meses"
    />
  </div>
);

export const FilaPrecisaDaSuaAtencao = () => (
  <div style={{ display: "flex", flexDirection: "column", gap: 12, maxWidth: 560 }}>
    <AIReviewItem
      icon="trending_up"
      title="A cobrança da Hetzner subiu 34%"
      body="Os últimos três meses ficaram em torno de R$ 745. A cobrança de agosto veio R$ 1.000."
      actionLabel="Ver lançamentos"
      confidence="confiança alta"
    />
    <AIReviewItem
      icon="link_off"
      title="4 PDFs ainda não viraram lançamento"
      body="Chegaram pelo Gmail entre 14/08 e 21/08. Três são comprovantes de pagamento e um é uma fatura de cartão."
      actionLabel="Abrir a caixa de entrada"
      confidence="4 documentos"
    />
  </div>
);
