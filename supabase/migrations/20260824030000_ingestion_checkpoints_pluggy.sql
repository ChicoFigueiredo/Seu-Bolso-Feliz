-- Fase 3 da integração Pluggy: reusa ingestion_checkpoints (P0-9, já
-- desenhada pra "varredura retomável") para o backfill de 365 dias, em vez
-- de inventar uma tabela paralela. Só precisa de dois valores novos nos
-- CHECKs existentes — o resto da tabela (scope_key, cursor_value, contadores,
-- window_start/end, status) já serve sem alteração.

ALTER TABLE ingestion_checkpoints DROP CONSTRAINT ingestion_checkpoints_source_type_check;
ALTER TABLE ingestion_checkpoints ADD CONSTRAINT ingestion_checkpoints_source_type_check
  CHECK (source_type IN ('gmail', 'local_file', 'pluggy'));

-- Pluggy pagina por número de página (page/pageSize), não por token nem
-- mtime — nenhum cursor_kind existente serve. page_number é 1-based e
-- durável entre execuções (ao contrário do page_token do Gmail).
ALTER TABLE ingestion_checkpoints DROP CONSTRAINT ingestion_checkpoints_cursor_kind_check;
ALTER TABLE ingestion_checkpoints ADD CONSTRAINT ingestion_checkpoints_cursor_kind_check
  CHECK (cursor_kind IN ('page_token', 'internal_date', 'mtime', 'page_number'));

-- rollback:
--   ALTER TABLE ingestion_checkpoints DROP CONSTRAINT ingestion_checkpoints_source_type_check;
--   ALTER TABLE ingestion_checkpoints ADD CONSTRAINT ingestion_checkpoints_source_type_check
--     CHECK (source_type IN ('gmail', 'local_file'));
--   ALTER TABLE ingestion_checkpoints DROP CONSTRAINT ingestion_checkpoints_cursor_kind_check;
--   ALTER TABLE ingestion_checkpoints ADD CONSTRAINT ingestion_checkpoints_cursor_kind_check
--     CHECK (cursor_kind IN ('page_token', 'internal_date', 'mtime'));
