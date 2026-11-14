-- Migração 3 (spec 09, "Banco"): triagem do inventário por IA. A IA só sugere:
-- uma sugestão nunca é decisão. Aceitar grava uma decisão do usuário no log
-- (`decisao`) e marca a sugestão como `aceita`.

-- Uma rodada: um passe da fila sobre um conjunto de itens.
CREATE TABLE rodada_ia (
  id                         INTEGER PRIMARY KEY,
  iniciada_em                TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  terminada_em               TEXT,
  itens                      INTEGER NOT NULL DEFAULT 0,
  chamadas                   INTEGER NOT NULL DEFAULT 0,
  custo_nominal_centavos_usd INTEGER NOT NULL DEFAULT 0,
  provedor                   TEXT NOT NULL,
  situacao                   TEXT NOT NULL DEFAULT 'rodando' CHECK (situacao IN
                               ('rodando', 'pausada', 'concluida', 'falhou', 'cancelada')),
  -- O piloto mede o acerto sobre itens que o usuário já decidiu (IA-09); as sugestões
  -- dele nascem `obsoleta` e nunca entram na lista de aceite.
  piloto                     INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE sugestao_ia (
  id            INTEGER PRIMARY KEY,
  alvo_tipo     TEXT NOT NULL CHECK (alvo_tipo IN ('achado', 'registro')),
  alvo_id       TEXT NOT NULL,
  -- achado: e_financeiro, tipo_documento, fornecedor, data_chave, valor
  -- registro: categoria, fornecedor, conta, tipo, duplicata_de
  -- nao_classificado: a IA não conseguiu (IA-03); nunca aparece na lista.
  campo         TEXT NOT NULL,
  valor_json    TEXT,            -- JSON
  confianca     REAL NOT NULL CHECK (confianca >= 0 AND confianca <= 1),
  justificativa TEXT NOT NULL,   -- até 140 caracteres
  modelo        TEXT NOT NULL,   -- nome do modelo, ou regra:<nome> para as regras gratuitas
  prompt_versao TEXT NOT NULL,
  rodada_id     INTEGER REFERENCES rodada_ia(id),
  criada_em     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
  estado        TEXT NOT NULL DEFAULT 'aberta'
                  CHECK (estado IN ('aberta', 'aceita', 'recusada', 'obsoleta'))
);
CREATE INDEX sugestao_ia_alvo ON sugestao_ia(alvo_tipo, alvo_id);
CREATE INDEX sugestao_ia_estado ON sugestao_ia(estado, campo);
CREATE INDEX sugestao_ia_rodada ON sugestao_ia(rodada_id);

-- O que saiu da máquina (ADR-015): tamanho, tokens e custo, nunca o texto.
CREATE TABLE saida_ia (
  id                      INTEGER PRIMARY KEY,
  destinatario            TEXT NOT NULL,
  provedor                TEXT NOT NULL,
  modelo                  TEXT NOT NULL,
  nivel                   TEXT NOT NULL CHECK (nivel IN ('mascarado', 'parcial', 'completo')),
  bytes_enviados          INTEGER NOT NULL,
  tokens_entrada          INTEGER NOT NULL DEFAULT 0,
  tokens_saida            INTEGER NOT NULL DEFAULT 0,
  custo_dezmilesimos_usd  INTEGER NOT NULL DEFAULT 0,
  nominal                 INTEGER NOT NULL DEFAULT 0,
  quando                  TEXT NOT NULL,
  rodada_id               INTEGER REFERENCES rodada_ia(id)  -- o lote de triagem
);
CREATE INDEX saida_ia_rodada ON saida_ia(rodada_id);

-- Cache por conteúdo: sha256(prompt_versao + modelo + entrada) → resposta.
CREATE TABLE cache_ia (
  chave         TEXT PRIMARY KEY,
  resposta_json TEXT NOT NULL,
  criada_em     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
