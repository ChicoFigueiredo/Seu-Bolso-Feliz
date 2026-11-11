import { requireUser } from "@/lib/auth";
import { ChatContextProvider } from "@/contexts/chat-context";
import { AppShell } from "@/components/shell/app-shell";
import { formatLongDatePtBR, fullNameOf, greetingLine } from "@/components/shell/nav-config";

export default async function DashboardLayout({ children }: { children: React.ReactNode }) {
  const user = await requireUser();

  // Calculado no servidor (não em useEffect) de propósito: o AppShell só
  // recebe texto já pronto, então o cliente nunca recalcula "agora" com um
  // relógio diferente do que foi usado no HTML enviado — sem isso, a saudação
  // e a data por extenso arriscam um mismatch de hidratação.
  const now = new Date();
  const displayName = fullNameOf(user.name, user.email);
  const greeting = greetingLine(now.getHours(), user.name, user.email);
  const dateLabel = formatLongDatePtBR(now);

  return (
    <ChatContextProvider>
      <AppShell displayName={displayName} greeting={greeting} dateLabel={dateLabel}>
        {children}
      </AppShell>
    </ChatContextProvider>
  );
}
