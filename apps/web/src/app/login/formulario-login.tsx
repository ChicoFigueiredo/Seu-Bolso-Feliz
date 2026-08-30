"use client";

import { useActionState, useState } from "react";
import { Button, Card, Icon, MetricLabel } from "@sbf/design-system";
import { entrarComSenha, type ResultadoLogin } from "@/app/actions/auth";

export function FormularioLogin({ erro, proximo }: { erro?: string; proximo?: string }) {
  const [mostrarSenha, setMostrarSenha] = useState(false);
  const [estado, acao, enviando] = useActionState<ResultadoLogin | null, FormData>(
    entrarComSenha,
    null,
  );

  const mensagem = estado?.erro ?? erro;
  const destino = proximo && proximo.startsWith("/") ? proximo : "/dashboard/hoje";

  return (
    <Card style={{ width: "100%", maxWidth: 380 }}>
      <div style={{ display: "grid", gap: 20 }}>
        <header style={{ display: "grid", gap: 6, justifyItems: "center", textAlign: "center" }}>
          <div
            style={{
              width: 52,
              height: 52,
              borderRadius: 16,
              background: "var(--primary-soft)",
              display: "grid",
              placeItems: "center",
              marginBottom: 4,
            }}
          >
            <Icon name="savings" size={28} color="var(--primary)" filled />
          </div>
          <h1
            style={{
              margin: 0,
              fontFamily: "'Instrument Serif', Georgia, serif",
              fontSize: 26,
              fontWeight: 400,
              color: "var(--text)",
            }}
          >
            Seu Bolso Feliz
          </h1>
          <p style={{ margin: 0, fontSize: 13, color: "var(--text-3)" }}>
            Sua vida financeira em um lugar só.
          </p>
        </header>

        {mensagem ? (
          <div
            role="alert"
            style={{
              display: "flex",
              gap: 8,
              alignItems: "flex-start",
              padding: "10px 12px",
              borderRadius: 12,
              background: "color-mix(in srgb, var(--danger) 8%, transparent)",
              border: "1px solid color-mix(in srgb, var(--danger) 25%, transparent)",
              fontSize: 13,
              color: "var(--danger)",
            }}
          >
            <Icon name="error" size={16} />
            <span>{mensagem}</span>
          </div>
        ) : null}

        <Button
          variant="primary"
          size="lg"
          block
          icon="login"
          onClick={() => {
            window.location.href = `/auth/google?next=${encodeURIComponent(destino)}`;
          }}
        >
          Entrar com o Google
        </Button>

        {mostrarSenha ? (
          <form action={acao} style={{ display: "grid", gap: 10 }}>
            <label style={{ display: "grid", gap: 4 }}>
              <MetricLabel>E-mail</MetricLabel>
              <input
                name="email"
                type="email"
                required
                autoComplete="username"
                style={estiloCampo}
              />
            </label>
            <label style={{ display: "grid", gap: 4 }}>
              <MetricLabel>Senha</MetricLabel>
              <input
                name="senha"
                type="password"
                required
                autoComplete="current-password"
                style={estiloCampo}
              />
            </label>
            <Button variant="outline" size="md" block type="submit" disabled={enviando}>
              {enviando ? "Entrando…" : "Entrar"}
            </Button>
          </form>
        ) : (
          <Button variant="quiet" size="sm" block onClick={() => setMostrarSenha(true)}>
            Entrar com senha
          </Button>
        )}

        <p
          style={{
            margin: 0,
            fontSize: 11,
            lineHeight: 1.5,
            color: "var(--text-3)",
            textAlign: "center",
          }}
        >
          O acesso por senha é a chave reserva, para quando o Google falhar.
        </p>
      </div>
    </Card>
  );
}

const estiloCampo: React.CSSProperties = {
  width: "100%",
  padding: "10px 12px",
  borderRadius: 12,
  border: "1px solid var(--line)",
  background: "var(--surface)",
  color: "var(--text)",
  // 16px é o mínimo que impede o Safari do iPhone de dar zoom ao focar o
  // campo — abaixo disso o layout do login pula na cara do usuário.
  fontSize: 16,
  fontFamily: "inherit",
};
