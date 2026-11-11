"use client";

import { useRef, useState, type ReactNode } from "react";
import { usePathname, useRouter } from "next/navigation";
import { AppSidebar, BottomNav, MobileHeader, SbfTheme, TopBar } from "@sbf/design-system";
import { ChatToggle } from "@/components/chat-toggle";
import { useChatContext } from "@/contexts/chat-context";
import {
  LOGOUT_ENTRY,
  LOGOUT_ID,
  MOBILE_TABS,
  PRIMARY_NAV,
  QUICK_ADD_HREF,
  SECONDARY_NAV,
  resolveActiveId,
  resolveTitle,
} from "./nav-config";
import { MoreSheet } from "./more-sheet";
import styles from "./shell.module.css";

export interface AppShellProps {
  /** Nome completo, para as iniciais do Avatar do MobileHeader. */
  displayName: string;
  /** Linha de saudação já pronta — "Bom dia, Chico". Só aparece em Hoje. */
  greeting: string;
  /** Data por extenso — vira o subtítulo do TopBar no desktop. */
  dateLabel: string;
  children: ReactNode;
}

const SIDEBAR_ENTRIES = [...PRIMARY_NAV, ...SECONDARY_NAV];

/**
 * Shell de navegação do Seu Bolso Feliz: sidebar + TopBar no desktop,
 * MobileHeader + BottomNav no mobile — a mesma árvore de rotas por baixo, só
 * o cromo muda por media query (ver shell.module.css).
 *
 * Server Components (páginas antigas em shadcn e as novas em
 * @sbf/design-system) entram como `children` sem virar Client Component só
 * por causa do shell: quem precisa de estado (pathname, folha "Mais",
 * assistente) é só este componente.
 */
export function AppShell({ displayName, greeting, dateLabel, children }: AppShellProps) {
  const pathname = usePathname();
  const router = useRouter();
  const { setDrawerOpen } = useChatContext();
  const [moreOpen, setMoreOpen] = useState(false);
  const signOutFormRef = useRef<HTMLFormElement>(null);

  const activeId = resolveActiveId(pathname);
  const title = resolveTitle(pathname);
  const mobileActiveId = MOBILE_TABS.some((tab) => tab.id === activeId)
    ? activeId
    : activeId
      ? "mais"
      : undefined;

  const handleSignOut = () => signOutFormRef.current?.requestSubmit();

  const handleSidebarNavigate = (id: string) => {
    if (id === LOGOUT_ID) {
      handleSignOut();
      return;
    }
    const entry = SIDEBAR_ENTRIES.find((item) => item.id === id);
    if (entry?.href) router.push(entry.href);
  };

  const handleTabNavigate = (id: string) => {
    if (id === "mais") {
      setMoreOpen(true);
      return;
    }
    const entry = MOBILE_TABS.find((tab) => tab.id === id);
    if (entry?.href) router.push(entry.href);
  };

  const handleAssistant = () => setDrawerOpen(true);
  const handleQuickAdd = () => router.push(QUICK_ADD_HREF);

  return (
    <div className={styles.shellRoot}>
      {/* Sair precisa de um POST de verdade (ver app/auth/signout/route.ts);
          este form fica escondido e é disparado pelos itens "Sair" do
          sidebar e da folha "Mais". */}
      <form
        ref={signOutFormRef}
        action="/auth/signout"
        method="POST"
        className={styles.hiddenSignOutForm}
      />

      <div className={styles.desktopSlot}>
        <SbfTheme style={{ display: "contents" }}>
          <AppSidebar
            items={PRIMARY_NAV}
            active={activeId}
            onNavigate={handleSidebarNavigate}
            secondary={[...SECONDARY_NAV, LOGOUT_ENTRY]}
            style={{ position: "sticky", top: 0, height: "100dvh", alignSelf: "flex-start" }}
          />
        </SbfTheme>
      </div>

      <div className={styles.mainColumn}>
        <div className={styles.desktopSlot}>
          <SbfTheme style={{ display: "contents" }}>
            <TopBar
              title={title}
              subtitle={dateLabel}
              onAction={handleQuickAdd}
              onAssistant={handleAssistant}
              style={{ position: "sticky", top: 0, zIndex: 20 }}
            />
          </SbfTheme>
        </div>

        <div className={styles.mobileSlot}>
          <SbfTheme style={{ display: "contents" }}>
            <MobileHeader
              title={title}
              greeting={activeId === "hoje" ? greeting : undefined}
              userName={displayName}
              onAssistant={handleAssistant}
              style={{ paddingTop: "calc(18px + env(safe-area-inset-top))" }}
            />
          </SbfTheme>
        </div>

        <main className={`${styles.mainContent} p-4 md:p-6`}>{children}</main>
      </div>

      <div className={styles.mobileBottomNavSlot}>
        <SbfTheme style={{ display: "contents" }}>
          <BottomNav
            tabs={MOBILE_TABS}
            active={mobileActiveId}
            onNavigate={handleTabNavigate}
            onFab={handleQuickAdd}
            style={{ paddingBottom: "calc(14px + env(safe-area-inset-bottom))" }}
          />
        </SbfTheme>
      </div>

      <SbfTheme style={{ display: "contents" }}>
        <MoreSheet
          open={moreOpen}
          onClose={() => setMoreOpen(false)}
          onNavigate={(href) => router.push(href)}
          onSignOut={handleSignOut}
        />
      </SbfTheme>

      {/* O botão flutuante do assistente é do shadcn de propósito — nasce
          fora de qualquer <SbfTheme>, então mantém a cara antiga sem
          adaptação. Só no desktop: no mobile ele cairia em cima do BottomNav
          fixo, e o MobileHeader já tem seu próprio atalho para o assistente. */}
      <div className={styles.desktopSlot}>
        <ChatToggle />
      </div>
    </div>
  );
}
