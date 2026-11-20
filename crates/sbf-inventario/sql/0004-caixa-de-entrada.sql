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
  -- O canal por onde o conteúdo chegou da primeira vez (não é a Origem do glossário: um
  -- documento tem várias origens, e esta coluna é só "chegou por").
  canal         TEXT NOT NULL CHECK (canal IN
                  ('telegram', 'gmail', 'celular', 'pasta', 'arrastado', 'cli')),
  -- O que foi reconhecido: o tipo ("Contracheque", "Boleto") e uma linha de resumo.
  tipo          TEXT,
  resumo        TEXT,
  -- Os estados do documento que o usuário vê (CONTEXT.md).
  estado        TEXT NOT NULL CHECK (estado IN
                  ('pronto', 'precisa_revisar', 'possivel_duplicidade', 'protegido',
                   'nao_reconhecido', 'processando')),
  -- O que o usuário decidiu: segue aberta até ele resolver ou dispensar. Resolver, dispensar
  -- e reabrir são decisões do log (`decisao`, campo `caixa_entrada`) e esta coluna é a
  -- projeção delas.
  situacao      TEXT NOT NULL DEFAULT 'aberta' CHECK (situacao IN
                  ('aberta', 'resolvida', 'dispensada')),
  resolvida_em  TEXT,
  -- Quantas vezes o reconhecimento rodou neste conteúdo.
  vezes         INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS caixa_entrada_fila ON caixa_entrada(situacao, estado, chegou_em);

-- Desde quando a Caixa de entrada existe neste banco: só os achados vistos a partir daqui
-- (e ainda sem linha) são reconhecidos pela reconciliação do worker; o acervo antigo não.
INSERT OR IGNORE INTO meta (chave, valor)
VALUES ('caixa_entrada_desde', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
