"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { AIChatDrawer } from "@/components/ai-chat-drawer";
import { Bot } from "lucide-react";

export function ChatToggle() {
  const [open, setOpen] = useState(false);

  return (
    <>
      <Button
        onClick={() => setOpen(true)}
        size="icon"
        className="fixed bottom-6 right-6 z-50 h-12 w-12 rounded-full shadow-lg"
        title="Abrir assistente IA"
      >
        <Bot className="h-6 w-6" />
      </Button>
      <AIChatDrawer open={open} onOpenChange={setOpen} />
    </>
  );
}
