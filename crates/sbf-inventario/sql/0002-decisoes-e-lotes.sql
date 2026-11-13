-- Migração 2 (spec 06, "O banco"): log de decisões só de acréscimo, lotes,
-- quem decidiu e os índices que faltavam. O trigger que recusa UPDATE e DELETE
-- em `decisao` é instalado pela migração em Rust, via bancada-armazem.

CREATE TABLE decisao (
  id        INTEGER PRIMARY KEY,
  alvo_tipo TEXT NOT NULL CHECK (alvo_tipo IN
              ('achado', 'registro', 'fornecedor', 'lote', 'pendencia')),
  alvo_id   TEXT NOT NULL,
  campo     TEXT NOT NULL,
  antes     TEXT,            -- JSON
  depois    TEXT,            -- JSON
  autor     TEXT NOT NULL,   -- usuario | regras@N
  motivo    TEXT,
  em        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  desfaz    INTEGER REFERENCES decisao(id)
);
CREATE INDEX decisao_alvo ON decisao(alvo_tipo, alvo_id);

CREATE TABLE lote (
  id           INTEGER PRIMARY KEY,
  nome         TEXT NOT NULL,
  criado_em    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  aprovado_em  TEXT,
  promovido_em TEXT
);

CREATE TABLE lote_item (
  lote_id   INTEGER NOT NULL REFERENCES lote(id),
  alvo_tipo TEXT NOT NULL CHECK (alvo_tipo IN ('achado', 'registro')),
  alvo_id   TEXT NOT NULL,
  PRIMARY KEY (lote_id, alvo_tipo, alvo_id)
);

ALTER TABLE achado ADD COLUMN decidido_por TEXT;
ALTER TABLE registro ADD COLUMN decidido_por TEXT;
UPDATE achado SET decidido_por = classificado_por;
UPDATE registro SET decidido_por = extraido_por;

CREATE INDEX IF NOT EXISTS origem_gmail_message ON origem(gmail_message_id);
CREATE INDEX IF NOT EXISTS origem_local_filepath ON origem(local_filepath);
-- A área Registros ordena por data (medido pela leitura: 358 ms → 228 ms com 100 mil).
CREATE INDEX IF NOT EXISTS registro_data ON registro(data);
-- As áreas Arquivos e pastas (por `local_mtime`) e E-mails (por `gmail_date`) mostram o mais
-- novo primeiro. Com o índice a página sai da ponta dele; sem, o SQLite ordena as 600 mil
-- origens a cada página.
CREATE INDEX IF NOT EXISTS origem_local_mtime ON origem(local_mtime);
CREATE INDEX IF NOT EXISTS origem_gmail_date ON origem(gmail_date);

-- O que desfaz o quê: "já foi desfeita?" e "Desfazer decisão" consultam por aqui.
CREATE INDEX decisao_desfaz ON decisao(desfaz);

-- Unir conciliados (registro ↔ registro). Não cabe em `relacao`, cujas colunas são
-- chaves estrangeiras para `achado(content_hash)`; a forma é a mesma.
CREATE TABLE relacao_registro (
  a         INTEGER NOT NULL REFERENCES registro(id),
  b         INTEGER NOT NULL REFERENCES registro(id),
  tipo      TEXT NOT NULL,
  confianca REAL,
  motivo    TEXT,
  PRIMARY KEY (a, b, tipo)
);
