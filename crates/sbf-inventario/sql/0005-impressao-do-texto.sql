-- Migração 5 (documento repetido por texto idêntico): a "impressão do texto" de cada achado,
-- o SHA-256 do texto normalizado (espaços e quebras de linha em sequência viram um espaço,
-- pontas aparadas; acentos e maiúsculas ficam). Dois achados com bytes diferentes e a mesma
-- impressão são o mesmo documento (a Stripe gera o PDF de novo a cada envio; o mesmo recibo
-- chega por e-mail e pelo Telegram).
--
-- Uma linha por linha de `texto`. `impressao` fica NULL quando o texto normalizado tem menos
-- de 200 caracteres (foto com OCR vazio não vira "duplicada" de outra) ou enquanto
-- `pendente = 1`. Quem sabe calcular o SHA-256 é o Rust, então quem grava `texto` sem passar
-- por ele (os scripts Bun) apenas deixa a linha pendente, pelos gatilhos abaixo, e o Rust
-- completa na próxima leitura. A tabela não tem chave estrangeira: os gatilhos a mantêm.
-- Os gatilhos usam `ON CONFLICT DO UPDATE` e não `INSERT OR REPLACE`: a cláusula de conflito
-- do comando de fora (`INSERT OR IGNORE`, o upsert do `e3-ocr.ts`) vence a de dentro do
-- gatilho, e um REPLACE ali falharia ou seria ignorado.
-- O preenchimento das linhas que já existem é feito em Rust, em lotes (`migracoes.rs`).

CREATE TABLE IF NOT EXISTS impressao_do_texto (
  content_hash TEXT PRIMARY KEY,
  impressao    TEXT,
  pendente     INTEGER NOT NULL DEFAULT 0 CHECK (pendente IN (0, 1))
);
-- A busca é por igualdade; só entra no índice quem tem impressão.
CREATE INDEX IF NOT EXISTS impressao_do_texto_igual
  ON impressao_do_texto(impressao) WHERE impressao IS NOT NULL;
CREATE INDEX IF NOT EXISTS impressao_do_texto_pendente
  ON impressao_do_texto(content_hash) WHERE pendente = 1;

CREATE TRIGGER IF NOT EXISTS texto_impressao_ao_inserir AFTER INSERT ON texto
BEGIN
  INSERT INTO impressao_do_texto (content_hash, impressao, pendente)
  VALUES (NEW.content_hash, NULL, 1)
  ON CONFLICT (content_hash) DO UPDATE SET impressao = NULL, pendente = 1;
END;

CREATE TRIGGER IF NOT EXISTS texto_impressao_ao_mudar AFTER UPDATE OF conteudo ON texto
BEGIN
  INSERT INTO impressao_do_texto (content_hash, impressao, pendente)
  VALUES (NEW.content_hash, NULL, 1)
  ON CONFLICT (content_hash) DO UPDATE SET impressao = NULL, pendente = 1;
END;

CREATE TRIGGER IF NOT EXISTS texto_impressao_ao_apagar AFTER DELETE ON texto
BEGIN
  DELETE FROM impressao_do_texto WHERE content_hash = OLD.content_hash;
END;
