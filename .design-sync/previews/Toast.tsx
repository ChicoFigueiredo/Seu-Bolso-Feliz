import { Toast } from "@sbf/design-system";

export const Confirmacao = () => <Toast>Documento vinculado ao lançamento</Toast>;

export const ComOutroIcone = () => (
  <div style={{ display: "flex", flexDirection: "column", gap: 12, alignItems: "flex-start" }}>
    <Toast icon="savings">Hetzner agora é acompanhado como recorrente</Toast>
    <Toast icon="sync">Buscando novidades em 3 fontes…</Toast>
    <Toast icon="task_alt">Pagamento marcado</Toast>
  </div>
);
