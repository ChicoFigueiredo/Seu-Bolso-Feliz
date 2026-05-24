-- ============================================================
-- Migration 017: Bucket de Storage — Documentos de Ingestão
-- ============================================================
-- Bucket privado para armazenar documentos originais
-- (PDFs, imagens, OFX, CSV) ingeridos pelo pipeline.
-- ============================================================

-- Cria o bucket privado para originais da ingestão
INSERT INTO storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
VALUES (
  'ingestion-originals',
  'ingestion-originals',
  false,
  52428800, -- 50 MB
  ARRAY[
    'application/pdf',
    'image/png',
    'image/jpeg',
    'image/webp',
    'text/csv',
    'application/vnd.ms-excel',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    'application/xml',
    'text/xml',
    'application/octet-stream'
  ]
);

-- RLS: usuário só acessa seus próprios arquivos
-- A convenção de path é: {user_id}/{source_document_id}/{filename}
CREATE POLICY "Users manage own ingestion files"
  ON storage.objects
  FOR ALL
  USING (
    bucket_id = 'ingestion-originals'
    AND auth.uid()::text = (storage.foldername(name))[1]
  )
  WITH CHECK (
    bucket_id = 'ingestion-originals'
    AND auth.uid()::text = (storage.foldername(name))[1]
  );
