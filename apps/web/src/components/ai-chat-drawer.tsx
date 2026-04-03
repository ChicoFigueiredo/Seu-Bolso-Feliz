"use client";

import { useChat } from "ai/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { Textarea } from "@/components/ui/textarea";
import { Bot, Send, Loader2, AlertCircle, User, Wrench, Plus } from "lucide-react";
import { ChatMarkdown } from "@/components/chat-markdown";
import { createClient } from "@/lib/supabase/client";

interface AIChatDrawerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function AIChatDrawer({ open, onOpenChange }: AIChatDrawerProps) {
  const [sessionId, setSessionId] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const { messages, input, handleInputChange, handleSubmit, isLoading, error } = useChat({
    api: "/api/chat",
    body: { sessionId },
    onError: (err) => {
      console.error("Chat error:", err);
    },
  });

  // Create session on first open
  useEffect(() => {
    if (open && !sessionId) {
      createSession();
    }
  }, [open, sessionId]);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages]);

  // Focus textarea when drawer opens
  useEffect(() => {
    if (open) {
      setTimeout(() => textareaRef.current?.focus(), 100);
    }
  }, [open]);

  async function createSession() {
    const supabase = createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return;

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const { data } = await (supabase as any)
      .from("ai_chat_sessions")
      .insert({
        user_id: user.id,
        title: "Nova conversa",
        context_type: "general",
      })
      .select("id")
      .single();

    if (data) setSessionId(data.id);
  }

  const handleNewSession = useCallback(() => {
    setSessionId(null);
    // useChat doesn't have a direct reset, so we create new session which resets via key
  }, []);

  const onSubmit = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault();
      if (!input.trim() || isLoading) return;
      handleSubmit(e);
    },
    [input, isLoading, handleSubmit],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        onSubmit(e);
      }
    },
    [onSubmit],
  );

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="flex w-full flex-col p-0 sm:max-w-lg">
        <SheetHeader className="border-b px-4 py-3">
          <div className="flex items-center justify-between">
            <SheetTitle className="flex items-center gap-2 text-base">
              <Bot className="h-5 w-5 text-primary" />
              Assistente SBF
            </SheetTitle>
            <Button variant="ghost" size="sm" onClick={handleNewSession} title="Nova conversa">
              <Plus className="h-4 w-4" />
            </Button>
          </div>
        </SheetHeader>

        {/* Messages area */}
        <ScrollArea className="flex-1 px-4" ref={scrollRef}>
          <div className="space-y-4 py-4">
            {messages.length === 0 && (
              <div className="py-8 text-center text-sm text-muted-foreground">
                <Bot className="mx-auto mb-3 h-10 w-10 text-muted-foreground/50" />
                <p className="font-medium">Olá! Sou o assistente do SBF.</p>
                <p className="mt-1">
                  Posso ajudar a revisar documentos, classificar transações e gerenciar drafts.
                </p>
                <div className="mx-auto mt-4 flex max-w-sm flex-wrap justify-center gap-2">
                  <Badge variant="outline" className="cursor-default text-xs">
                    &quot;Quais documentos estão pendentes?&quot;
                  </Badge>
                  <Badge variant="outline" className="cursor-default text-xs">
                    &quot;Mostre os últimos drafts&quot;
                  </Badge>
                  <Badge variant="outline" className="cursor-default text-xs">
                    &quot;Sugira categoria para essa transação&quot;
                  </Badge>
                </div>
              </div>
            )}

            {messages.map((message) => (
              <div
                key={message.id}
                className={`flex gap-3 ${message.role === "user" ? "justify-end" : "justify-start"}`}
              >
                {message.role !== "user" && (
                  <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-primary/10">
                    {message.role === "assistant" ? (
                      <Bot className="h-4 w-4 text-primary" />
                    ) : (
                      <Wrench className="h-4 w-4 text-muted-foreground" />
                    )}
                  </div>
                )}

                <div
                  className={`max-w-[85%] rounded-lg px-3 py-2 text-sm ${
                    message.role === "user" ? "bg-primary text-primary-foreground" : "bg-muted"
                  }`}
                >
                  {message.role === "user" ? (
                    <div className="whitespace-pre-wrap">{message.content}</div>
                  ) : (
                    <ChatMarkdown content={message.content} />
                  )}

                  {/* Show tool invocations */}
                  {message.toolInvocations && message.toolInvocations.length > 0 && (
                    <div className="mt-2 space-y-1">
                      {message.toolInvocations.map((invocation, i) => (
                        <div
                          key={i}
                          className="flex items-center gap-1 text-xs text-muted-foreground"
                        >
                          <Wrench className="h-3 w-3" />
                          <span>{invocation.toolName.replace(/_/g, " ")}</span>
                          {"state" in invocation && invocation.state === "result" && (
                            <Badge variant="outline" className="ml-1 text-[10px]">
                              concluído
                            </Badge>
                          )}
                        </div>
                      ))}
                    </div>
                  )}
                </div>

                {message.role === "user" && (
                  <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-foreground/10">
                    <User className="h-4 w-4" />
                  </div>
                )}
              </div>
            ))}

            {isLoading && (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                Pensando...
              </div>
            )}

            {error && (
              <div className="flex items-center gap-2 rounded-lg bg-destructive/10 p-3 text-sm text-destructive">
                <AlertCircle className="h-4 w-4 shrink-0" />
                <span>{error.message || "Erro ao comunicar com o assistente."}</span>
              </div>
            )}
          </div>
        </ScrollArea>

        {/* Input area */}
        <div className="border-t p-4">
          <form onSubmit={onSubmit} className="flex gap-2">
            <Textarea
              ref={textareaRef}
              value={input}
              onChange={handleInputChange}
              onKeyDown={handleKeyDown}
              placeholder="Digite sua mensagem..."
              className="min-h-[40px] max-h-[120px] resize-none"
              rows={1}
              disabled={isLoading}
            />
            <Button
              type="submit"
              size="icon"
              disabled={!input.trim() || isLoading}
              className="shrink-0"
            >
              {isLoading ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Send className="h-4 w-4" />
              )}
            </Button>
          </form>
          <p className="mt-2 text-center text-[11px] text-muted-foreground">
            IA pode cometer erros. Sempre verifique dados financeiros.
          </p>
        </div>
      </SheetContent>
    </Sheet>
  );
}
