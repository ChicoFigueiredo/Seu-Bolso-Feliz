-- Inventário financeiro (ADR-011). Vocabulário em CONTEXT.md: achado, origem,
-- registro, sub-registro, ciclo.
--
-- Colunas espelhadas de source_documents mantêm o nome do app para a promoção
-- ser um mapeamento: origin_type, origin_key, local_filepath, local_mtime,
-- filename, mime_type, file_size_bytes, content_hash, gmail_*, document_type,
-- supplier_name_raw. content_hash é o SHA-256 hex dos bytes, igual a
-- computeContentHash de @sbf/operations.
--
-- Nenhuma tabela guarda segredo: senhas e CPF vivem só em segredos/cofre.enc.

CREATE TABLE IF NOT EXISTS meta (
  chave TEXT PRIMARY KEY,
  valor TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS ciclo (
  id     TEXT PRIMARY KEY,  -- AAAA-MM do início
  inicio TEXT NOT NULL,     -- AAAA-MM-DD, inclusive
  fim    TEXT NOT NULL      -- AAAA-MM-DD, inclusive
);

-- Uma fonte: um disco, uma nuvem, uma caixa de e-mail, o celular, a Pluggy.
-- Toda fonte planejada é registrada antes de ser varrida, para a matriz de
-- cobertura mostrar "ausente" em vez de esquecer.
CREATE TABLE IF NOT EXISTS fonte (
  id        TEXT PRIMARY KEY,  -- disco:e2, gmail, onedrive, celular, pluggy…
  tipo      TEXT NOT NULL CHECK (tipo IN
              ('disco', 'nuvem', 'email', 'celular', 'mensageiro', 'conexao', 'banco', 'app', 'rh')),
  descricao TEXT NOT NULL,
  raiz      TEXT,
  escopo    TEXT NOT NULL DEFAULT 'dentro' CHECK (escopo IN ('dentro', 'fora')),
  motivo    TEXT,              -- por que está fora, quando está
  criada_em TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Uma varredura é um lote: quem rodou, quando, sobre o quê, com que resultado.
CREATE TABLE IF NOT EXISTS varredura (
  id           INTEGER PRIMARY KEY,
  fonte_id     TEXT NOT NULL REFERENCES fonte(id),
  etapa        TEXT NOT NULL,  -- E1…E5
  escopo       TEXT NOT NULL,  -- caminho raiz, query, janela de datas
  responsavel  TEXT NOT NULL,  -- script@versão ou agente/modelo
  parametros   TEXT,           -- JSON
  iniciada_em  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  concluida_em TEXT,
  status       TEXT NOT NULL DEFAULT 'rodando' CHECK (status IN
                 ('rodando', 'concluida', 'interrompida', 'falhou')),
  vistos       INTEGER NOT NULL DEFAULT 0,
  novos        INTEGER NOT NULL DEFAULT 0,
  repetidos    INTEGER NOT NULL DEFAULT 0,
  excluidos    INTEGER NOT NULL DEFAULT 0,
  erros        INTEGER NOT NULL DEFAULT 0,
  cursor       TEXT            -- onde retomar se for interrompida
);

-- Achado: um conteúdo único. Vira documento quando se confirma financeiro.
CREATE TABLE IF NOT EXISTS achado (
  content_hash          TEXT PRIMARY KEY,
  file_size_bytes       INTEGER NOT NULL,
  mime_type             TEXT,
  extensao              TEXT,
  camada                TEXT NOT NULL CHECK (camada IN ('A', 'B')),
  caminho_acervo        TEXT,  -- relativo à raiz do cofre; NULL = não copiado
  estado                TEXT NOT NULL DEFAULT 'novo' CHECK (estado IN
                          ('novo', 'documento', 'descartado', 'duvida')),
  document_type         TEXT,  -- comprovante, fatura, extrato, boleto, contracheque…
  protegido_senha       INTEGER,  -- 0/1; NULL = não verificado
  canonical_fingerprint TEXT,  -- SHA-256 do texto normalizado (possível duplicidade)
  supplier_name_raw     TEXT,  -- fornecedor deduzido, como apareceu
  data_chave            TEXT,  -- AAAA-MM-DD
  data_chave_tipo       TEXT CHECK (data_chave_tipo IN
                          ('pagamento', 'vencimento', 'emissao', 'compra', 'mensagem', 'arquivo')),
  ciclo_id              TEXT REFERENCES ciclo(id),
  valor_centavos        INTEGER,
  confianca             REAL,
  classificado_por      TEXT,
  primeiro_visto_em     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  notas                 TEXT
);
CREATE INDEX IF NOT EXISTS achado_estado ON achado(estado);
CREATE INDEX IF NOT EXISTS achado_ciclo ON achado(ciclo_id);
CREATE INDEX IF NOT EXISTS achado_fingerprint ON achado(canonical_fingerprint);

-- Texto extraído de um achado (pdftotext, OCR), separado para não inchar achado.
CREATE TABLE IF NOT EXISTS texto (
  content_hash TEXT PRIMARY KEY REFERENCES achado(content_hash),
  metodo       TEXT NOT NULL,  -- pdftotext, ocr-por, exif, xml…
  conteudo     TEXT NOT NULL,
  extraido_em  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Origem: cada lugar onde um conteúdo apareceu. O mesmo conteúdo em três
-- lugares é um achado com três origens.
CREATE TABLE IF NOT EXISTS origem (
  id                  INTEGER PRIMARY KEY,
  content_hash        TEXT REFERENCES achado(content_hash),  -- NULL enquanto não hasheado
  fonte_id            TEXT NOT NULL REFERENCES fonte(id),
  varredura_id        INTEGER NOT NULL REFERENCES varredura(id),
  origin_type         TEXT NOT NULL,  -- local_file | gmail | outlook | telegram | whatsapp | sms…
  origin_key          TEXT NOT NULL UNIQUE,  -- local:{path}:{mtimeMs}, gmail:{msgId}:{hash}…
  local_filepath      TEXT,
  local_mtime         TEXT,
  filename            TEXT NOT NULL,
  file_size_bytes     INTEGER,
  gmail_message_id    TEXT,
  gmail_thread_id     TEXT,
  gmail_attachment_id TEXT,
  gmail_from          TEXT,
  gmail_subject       TEXT,
  gmail_date          TEXT,
  gmail_label         TEXT,
  mensagem_ref        TEXT,  -- conversa/mensagem em Telegram, WhatsApp, SMS, Outlook
  visto_em            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  erro                TEXT
);
CREATE INDEX IF NOT EXISTS origem_hash ON origem(content_hash);
CREATE INDEX IF NOT EXISTS origem_fonte ON origem(fonte_id);

-- Camada C: áreas vistas e deixadas de fora de propósito, com contagem.
CREATE TABLE IF NOT EXISTS area_excluida (
  id           INTEGER PRIMARY KEY,
  fonte_id     TEXT NOT NULL REFERENCES fonte(id),
  varredura_id INTEGER NOT NULL REFERENCES varredura(id),
  caminho      TEXT NOT NULL,
  motivo       TEXT NOT NULL,  -- livros, jogos, cursos, codigo, cache, outra-pessoa, trabalho
  arquivos     INTEGER NOT NULL,
  bytes        INTEGER NOT NULL,
  UNIQUE (varredura_id, caminho)
);

-- Registro: fato financeiro declarado por um documento ou por uma conexão.
-- Só vira movimentação quando o usuário aprova a promoção.
CREATE TABLE IF NOT EXISTS registro (
  id                     INTEGER PRIMARY KEY,
  content_hash           TEXT REFERENCES achado(content_hash),  -- NULL se veio de conexão
  external_ref           TEXT UNIQUE,  -- pluggy:txn_<id>, igual a draft_records
  fonte_id               TEXT NOT NULL REFERENCES fonte(id),
  tipo                   TEXT NOT NULL,  -- compra, pagamento, credito, debito, tarifa, salario, estorno…
  data                   TEXT NOT NULL,  -- AAAA-MM-DD
  data_tipo              TEXT NOT NULL,  -- pagamento, vencimento, emissao, compra…
  ciclo_id               TEXT REFERENCES ciclo(id),
  valor_centavos         INTEGER NOT NULL,  -- positivo entrou, negativo saiu
  descricao_original     TEXT,
  supplier_name_raw      TEXT,
  fornecedor_normalizado TEXT,
  categoria_provavel     TEXT,
  conta_provavel         TEXT,
  confianca              REAL,
  extraido_por           TEXT NOT NULL,
  conferido_por          TEXT,
  estado                 TEXT NOT NULL DEFAULT 'extraido' CHECK (estado IN
                           ('extraido', 'conferido', 'divergente', 'aprovado', 'promovido')),
  movimentacao_id        TEXT,  -- transactions.id no app, quando a movimentação já existe
  criado_em              TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX IF NOT EXISTS registro_ciclo ON registro(ciclo_id);
CREATE INDEX IF NOT EXISTS registro_documento ON registro(content_hash);

CREATE TABLE IF NOT EXISTS sub_registro (
  id                    INTEGER PRIMARY KEY,
  registro_id           INTEGER NOT NULL REFERENCES registro(id) ON DELETE CASCADE,
  tipo                  TEXT NOT NULL,  -- parcela, iof, juros, rubrica, taxa, desconto
  codigo                TEXT,           -- código da rubrica, quando houver
  descricao_original    TEXT NOT NULL,
  descricao_normalizada TEXT,
  valor_centavos        INTEGER,
  parcela_numero        INTEGER,
  parcela_total         INTEGER,
  confianca             REAL
);

-- Relação entre achados: possível duplicidade, anexo de, mesma fatura.
CREATE TABLE IF NOT EXISTS relacao (
  a         TEXT NOT NULL REFERENCES achado(content_hash),
  b         TEXT NOT NULL REFERENCES achado(content_hash),
  tipo      TEXT NOT NULL,
  confianca REAL,
  motivo    TEXT,
  PRIMARY KEY (a, b, tipo)
);

-- Regra de senha: fornecedor/tipo → estratégia. Nunca o segredo.
CREATE TABLE IF NOT EXISTS regra_senha (
  id                INTEGER PRIMARY KEY,
  fornecedor        TEXT NOT NULL,
  document_type     TEXT NOT NULL DEFAULT '*',
  estrategia        TEXT NOT NULL,  -- cpf_completo, cpf_5_primeiros, senha_propria:<rotulo>…
  prioridade        INTEGER NOT NULL DEFAULT 100,
  ativa             INTEGER NOT NULL DEFAULT 1,
  sucessos          INTEGER NOT NULL DEFAULT 0,
  falhas            INTEGER NOT NULL DEFAULT 0,
  ultimo_sucesso_em TEXT,
  ultima_falha_em   TEXT,
  notas             TEXT,
  UNIQUE (fornecedor, document_type, estrategia)
);

-- Cada tentativa de abrir um PDF protegido: a estratégia e o resultado, nunca a senha.
CREATE TABLE IF NOT EXISTS tentativa_senha (
  id           INTEGER PRIMARY KEY,
  content_hash TEXT NOT NULL REFERENCES achado(content_hash),
  estrategia   TEXT NOT NULL,
  sucesso      INTEGER NOT NULL,
  em           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- Pendência: dúvida que espera o usuário (a fila das 06:00).
CREATE TABLE IF NOT EXISTS pendencia (
  id           INTEGER PRIMARY KEY,
  etapa        TEXT NOT NULL,
  tipo         TEXT NOT NULL,
  referencia   TEXT,
  pergunta     TEXT NOT NULL,
  criada_em    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  resolvida_em TEXT,
  resposta     TEXT
);
