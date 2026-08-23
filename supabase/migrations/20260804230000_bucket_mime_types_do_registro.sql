-- Alinha `allowed_mime_types` do bucket com o registro único de formatos
-- (`packages/contracts/src/formats.ts`).
--
-- O bucket é o último dos cinco lugares que declaravam formatos por conta
-- própria, e o mais traiçoeiro: a recusa por MIME acontece DEPOIS de o
-- documento já ter sido registrado em `source_documents`, deixando uma linha
-- órfã apontando para um arquivo que nunca subiu. O job então falha no download
-- com "Object not found", uma mensagem que não tem relação alguma com a causa.
--
-- Diferenças em relação à lista anterior:
--   + application/x-ofx, application/csv, text/plain — o extrato de banco
--     raramente chega com o MIME certo, e o scanner registra o que recebeu
--   - application/vnd.ms-excel (.xls) — o exceljs lê XLSX e não lê XLS. O
--     formato era aceito pelos scanners e não era aberto por ninguém
--
-- `text/plain` e `application/octet-stream` são amplos de propósito: é o preço
-- de aceitar OFX e CSV vindos de fontes que não rotulam direito. A defesa real
-- está na extensão, checada pelos scanners antes do upload, e no parser, que
-- falha alto quando o conteúdo não é o que o nome diz.

UPDATE storage.buckets
SET allowed_mime_types = ARRAY[
  'application/pdf',
  'text/csv',
  'application/csv',
  'text/plain',
  'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  'application/x-ofx',
  'application/octet-stream',
  'application/xml',
  'text/xml',
  'image/png',
  'image/jpeg',
  'image/webp'
]
WHERE id = 'ingestion-originals';

-- rollback:
--   UPDATE storage.buckets
--   SET allowed_mime_types = ARRAY[
--     'application/pdf', 'image/png', 'image/jpeg', 'image/webp', 'text/csv',
--     'application/vnd.ms-excel',
--     'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
--     'application/xml', 'text/xml', 'application/octet-stream'
--   ]
--   WHERE id = 'ingestion-originals';
