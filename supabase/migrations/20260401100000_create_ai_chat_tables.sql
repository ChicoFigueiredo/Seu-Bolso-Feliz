-- AI Chat tables for SBF AI integration
-- Sessions store conversation history, messages store individual exchanges

-- ══════════════════════════════════════════════════════════════
-- AI Chat Sessions
-- ══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS ai_chat_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES auth.users(id),
    title TEXT,
    context_type TEXT, -- 'general', 'document_review', 'ingestion', 'reconciliation'
    context_id UUID, -- optional: links to a specific document/draft being discussed
    model TEXT NOT NULL DEFAULT 'gpt-4o',
    total_tokens_used INTEGER DEFAULT 0,
    message_count INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now()
);

-- ══════════════════════════════════════════════════════════════
-- AI Chat Messages
-- ══════════════════════════════════════════════════════════════

CREATE TABLE IF NOT EXISTS ai_chat_messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id UUID NOT NULL REFERENCES ai_chat_sessions(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES auth.users(id),
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    content TEXT,
    tool_calls JSONB, -- array of tool invocations from the assistant
    tool_call_id TEXT, -- for tool response messages
    tool_name TEXT, -- which tool was called
    tokens_used INTEGER,
    latency_ms INTEGER,
    model TEXT,
    created_at TIMESTAMPTZ DEFAULT now()
);

-- ══════════════════════════════════════════════════════════════
-- RLS Policies
-- ══════════════════════════════════════════════════════════════

ALTER TABLE ai_chat_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE ai_chat_messages ENABLE ROW LEVEL SECURITY;

CREATE POLICY "Users can manage own chat sessions"
    ON ai_chat_sessions FOR ALL
    USING (auth.uid() = user_id)
    WITH CHECK (auth.uid() = user_id);

CREATE POLICY "Users can manage own chat messages"
    ON ai_chat_messages FOR ALL
    USING (auth.uid() = user_id)
    WITH CHECK (auth.uid() = user_id);

-- ══════════════════════════════════════════════════════════════
-- Indexes
-- ══════════════════════════════════════════════════════════════

CREATE INDEX idx_ai_chat_sessions_user ON ai_chat_sessions(user_id);
CREATE INDEX idx_ai_chat_sessions_updated ON ai_chat_sessions(updated_at DESC);
CREATE INDEX idx_ai_chat_messages_session ON ai_chat_messages(session_id, created_at);
CREATE INDEX idx_ai_chat_messages_user ON ai_chat_messages(user_id);

-- ══════════════════════════════════════════════════════════════
-- Helper RPC: Increment session token/message counters
-- ══════════════════════════════════════════════════════════════

CREATE OR REPLACE FUNCTION increment_session_tokens(
    p_session_id UUID,
    p_tokens INTEGER,
    p_messages INTEGER DEFAULT 2
)
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
BEGIN
    UPDATE ai_chat_sessions
    SET
        total_tokens_used = total_tokens_used + p_tokens,
        message_count = message_count + p_messages,
        updated_at = now()
    WHERE id = p_session_id AND user_id = auth.uid();
END;
$$;
