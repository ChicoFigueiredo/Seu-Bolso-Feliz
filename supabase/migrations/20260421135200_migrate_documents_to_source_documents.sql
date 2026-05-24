-- ─────────────────────────────────────────────────────────────────────────────
-- Migration: S2-001 — Migrar dados de documents → source_documents
--
-- Para cada registro em documents que ainda não possui equivalente em
-- source_documents, criamos:
--   1. Um ingestion_run sintético com status 'completed'
--   2. Um source_document com origin_type = 'manual_upload'
--   3. Um ingestion_job sintético ligando os dois
--
-- Os documentos já migrados são ignorados (upsert por origin_key).
-- O campo origin_key usa o padrão 'legacy::<documents.id>' para
-- garantir unicidade e rastreabilidade.
-- ─────────────────────────────────────────────────────────────────────────────

DO $$
DECLARE
  doc         RECORD;
  run_id      UUID;
  src_doc_id  UUID;
BEGIN
  FOR doc IN
    SELECT d.*
    FROM documents d
    WHERE NOT EXISTS (
      SELECT 1 FROM source_documents sd
       WHERE sd.user_id   = d.user_id
         AND sd.origin_type = 'manual_upload'
         AND sd.origin_key  = 'legacy::' || d.id::TEXT
    )
  LOOP
    -- 1. Criar ingestion_run sintético (status completed)
    INSERT INTO ingestion_runs (user_id, status, started_at, finished_at, metadata)
    VALUES (
      doc.user_id,
      'completed',
      COALESCE(doc.created_at, now()),
      COALESCE(doc.created_at, now()),
      jsonb_build_object('source', 'legacy_migration', 'legacy_document_id', doc.id)
    )
    RETURNING id INTO run_id;

    -- 2. Criar source_document com origin 'manual_upload'
    INSERT INTO source_documents (
      user_id,
      origin_type,
      origin_key,
      filename,
      mime_type,
      file_size_bytes,
      storage_path,
      status,
      created_at,
      updated_at
    ) VALUES (
      doc.user_id,
      'manual_upload',
      'legacy::' || doc.id::TEXT,
      doc.name,
      doc.file_type,
      doc.file_size,
      doc.file_path,
      'processed',            -- documento já estava "armazenado"
      COALESCE(doc.created_at, now()),
      COALESCE(doc.created_at, now())
    )
    ON CONFLICT (user_id, origin_type, origin_key) DO NOTHING
    RETURNING id INTO src_doc_id;

    -- Se houve conflito (ON CONFLICT DO NOTHING), busca o id existente
    IF src_doc_id IS NULL THEN
      SELECT id INTO src_doc_id
        FROM source_documents
       WHERE user_id    = doc.user_id
         AND origin_type = 'manual_upload'
         AND origin_key  = 'legacy::' || doc.id::TEXT;
    END IF;

    -- 3. Criar ingestion_job sintético (status 'approved')
    INSERT INTO ingestion_jobs (
      run_id,
      user_id,
      source_document_id,
      status,
      metadata,
      created_at,
      updated_at
    ) VALUES (
      run_id,
      doc.user_id,
      src_doc_id,
      'approved',
      jsonb_build_object('source', 'legacy_migration', 'legacy_document_id', doc.id),
      COALESCE(doc.created_at, now()),
      COALESCE(doc.created_at, now())
    );

  END LOOP;
END $$;
