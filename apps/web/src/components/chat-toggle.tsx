"use client";

import { Button } from "@/components/ui/button";
import { AIChatDrawer } from "@/components/ai-chat-drawer";
import { Bot } from "lucide-react";
import { useChatContext } from "@/contexts/chat-context";

export function ChatToggle() {
  const { drawerOpen, setDrawerOpen } = useChatContext();

  return (
    <>
      <Button
        onClick={() => setDrawerOpen(true)}
        size="icon"
        className="fixed bottom-6 right-6 z-50 h-12 w-12 rounded-full shadow-lg"
        title="Abrir assistente IA"
      >
        <Bot className="h-6 w-6" />
      </Button>
      <AIChatDrawer open={drawerOpen} onOpenChange={setDrawerOpen} />
    </>
  );
}
