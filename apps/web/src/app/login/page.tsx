"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { createClient } from "@/lib/supabase/client";

export default function LoginPage() {
  const router = useRouter();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [status, setStatus] = useState<"idle" | "loading" | "sent" | "signed-in" | "error">("idle");
  const [errorMsg, setErrorMsg] = useState("");

  async function handleGoogleLogin() {
    setStatus("loading");
    setErrorMsg("");

    const supabase = createClient();
    const { error } = await supabase.auth.signInWithOAuth({
      provider: "google",
      options: {
        redirectTo: `${window.location.origin}/auth/callback`,
      },
    });

    if (error) {
      setStatus("error");
      setErrorMsg(error.message);
    }
  }

  async function handleLogin(e: React.FormEvent) {
    e.preventDefault();
    setStatus("loading");
    setErrorMsg("");

    const supabase = createClient();
    const { error } = await supabase.auth.signInWithOtp({
      email,
      options: {
        emailRedirectTo: `${window.location.origin}/auth/callback`,
      },
    });

    if (error) {
      setStatus("error");
      setErrorMsg(error.message);
    } else {
      setStatus("sent");
    }
  }

  async function handlePasswordLogin() {
    setStatus("loading");
    setErrorMsg("");

    const supabase = createClient();
    const { error } = await supabase.auth.signInWithPassword({
      email,
      password,
    });

    if (error) {
      setStatus("error");
      setErrorMsg(error.message);
      return;
    }

    setStatus("signed-in");
    router.push("/dashboard");
    router.refresh();
  }

  async function handleCreateTestAccount() {
    setStatus("loading");
    setErrorMsg("");

    const supabase = createClient();
    const { error } = await supabase.auth.signUp({
      email,
      password,
      options: {
        emailRedirectTo: `${window.location.origin}/auth/callback`,
      },
    });

    if (error) {
      setStatus("error");
      setErrorMsg(error.message);
      return;
    }

    const loginResult = await supabase.auth.signInWithPassword({
      email,
      password,
    });

    if (loginResult.error) {
      setStatus("error");
      setErrorMsg(loginResult.error.message);
      return;
    }

    setStatus("signed-in");
    router.push("/dashboard");
    router.refresh();
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-background">
      <div className="mx-auto w-full max-w-sm space-y-6 rounded-lg border p-8 shadow-sm">
        <div className="space-y-2 text-center">
          <h1 className="text-2xl font-bold">Seu Bolso Feliz</h1>
          <p className="text-sm text-muted-foreground">
            Entre com magic link ou use e-mail e senha para desenvolvimento local
          </p>
        </div>

        {status === "sent" ? (
          <div className="rounded-md bg-green-50 p-4 text-center text-sm text-green-800 dark:bg-green-950 dark:text-green-200">
            <p className="font-medium">Link enviado!</p>
            <p className="mt-1">
              No ambiente local, abra o Inbucket em http://127.0.0.1:54324 e clique no e-mail de{" "}
              {email}.
            </p>
          </div>
        ) : status === "signed-in" ? (
          <div className="rounded-md bg-green-50 p-4 text-center text-sm text-green-800 dark:bg-green-950 dark:text-green-200">
            <p className="font-medium">Login concluido.</p>
            <p className="mt-1">Redirecionando para o dashboard.</p>
          </div>
        ) : (
          <>
            <button
              type="button"
              disabled={status === "loading"}
              onClick={handleGoogleLogin}
              className="flex w-full items-center justify-center gap-3 rounded-md border px-4 py-2 text-sm font-medium hover:bg-muted disabled:opacity-50"
            >
              <svg className="h-5 w-5" viewBox="0 0 24 24">
                <path
                  d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92a5.06 5.06 0 0 1-2.2 3.32v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.1z"
                  fill="#4285F4"
                />
                <path
                  d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"
                  fill="#34A853"
                />
                <path
                  d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"
                  fill="#FBBC05"
                />
                <path
                  d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"
                  fill="#EA4335"
                />
              </svg>
              Entrar com Google
            </button>

            <div className="relative">
              <div className="absolute inset-0 flex items-center">
                <span className="w-full border-t" />
              </div>
              <div className="relative flex justify-center text-xs uppercase">
                <span className="bg-background px-2 text-muted-foreground">ou</span>
              </div>
            </div>

            <form onSubmit={handleLogin} className="space-y-4">
            <div className="space-y-2">
              <label htmlFor="email" className="text-sm font-medium">
                E-mail
              </label>
              <input
                id="email"
                type="email"
                required
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                placeholder="seu@email.com"
                className="w-full rounded-md border bg-background px-3 py-2 text-sm outline-none ring-offset-background focus:ring-2 focus:ring-ring focus:ring-offset-2"
                disabled={status === "loading"}
              />
            </div>

            <div className="space-y-2">
              <label htmlFor="password" className="text-sm font-medium">
                Senha de desenvolvimento
              </label>
              <input
                id="password"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Minimo de 6 caracteres"
                className="w-full rounded-md border bg-background px-3 py-2 text-sm outline-none ring-offset-background focus:ring-2 focus:ring-ring focus:ring-offset-2"
                disabled={status === "loading"}
              />
              <p className="text-xs text-muted-foreground">
                Para login com senha local, crie uma conta de teste ou use uma conta ja cadastrada.
              </p>
            </div>

            {status === "error" && (
              <p className="text-sm text-red-600 dark:text-red-400">{errorMsg}</p>
            )}

            <div className="space-y-2">
              <button
                type="submit"
                disabled={status === "loading"}
                className="w-full rounded-md bg-primary px-4 py-2 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
              >
                {status === "loading" ? "Processando..." : "Entrar com Magic Link"}
              </button>

              <button
                type="button"
                disabled={status === "loading" || !email || !password}
                onClick={handlePasswordLogin}
                className="w-full rounded-md border px-4 py-2 text-sm font-medium hover:bg-muted disabled:opacity-50"
              >
                Entrar com E-mail e Senha
              </button>

              <button
                type="button"
                disabled={status === "loading" || !email || password.length < 6}
                onClick={handleCreateTestAccount}
                className="w-full rounded-md border px-4 py-2 text-sm font-medium hover:bg-muted disabled:opacity-50"
              >
                Criar Conta de Teste Local
              </button>
            </div>
          </form>
          </>
        )}
      </div>
    </div>
  );
}
