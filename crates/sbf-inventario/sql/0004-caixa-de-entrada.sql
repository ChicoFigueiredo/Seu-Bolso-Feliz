-- Migração 4 (Caixa de entrada): a fila de reconhecimento. Uma linha por conteúdo
-- (`content_hash`) que passou pelo reconhecimento depois desta migração; nada retroage nos
-- achados antigos. `IF NOT EXISTS` porque o reconhecimento (worker, CLI) também a garante
-- num banco que a janela ainda não migrou.

CREATE TABLE IF NOT EXISTS caixa_entrada (
  content_hash  TEXT PRIMARY KEY,
  nome          TEXT NOT NULL,
  -- Quando o conteúdo entrou na fila e quando o reconhecimento rodou por último.
  chegou_em     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  atualizado_em TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  -- Por onde chegou da primeira vez.
  origem        TEXT NOT NULL CHECK (origem IN
                  ('telegram', 'gmail', 'celular', 'pasta', 'arrastado', 'cli')),
  -- O que foi reconhecido: o tipo ("Contracheque", "Boleto") e uma linha de resumo.
  tipo          TEXT,
  resumo        TEXT,
  -- Os estados do documento que o usuário vê (CONTEXT.md).
  estado        TEXT NOT NULL CHECK (estado IN
                  ('pronto', 'precisa_revisar', 'possivel_duplicidade', 'protegido',
                   'nao_reconhecido', 'processando')),
  -- O que o usuário decidiu: segue aberta até ele resolver ou dispensar.
  situacao      TEXT NOT NULL DEFAULT 'aberta' CHECK (situacao IN
                  ('aberta', 'resolvida', 'dispensada')),
  resolvida_em  TEXT,
  -- Quantas vezes o reconhecimento rodou neste conteúdo.
  vezes         INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS caixa_entrada_fila ON caixa_entrada(situacao, estado, chegou_em);
