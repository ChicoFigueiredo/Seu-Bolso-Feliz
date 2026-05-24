-- Migration: Storage buckets + políticas de acesso por usuário
-- Gap C2 identificado em auditoria 2026-03-22
-- Storage habilitado no config.toml mas nenhum bucket criado.
-- A tabela documents referencia file_path mas sem bucket não há onde armazenar.

-- Bucket: documents (privado, documentos financeiros dos usuários)
INSERT INTO storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
VALUES (
  'documents',
  'documents',
  false,
  52428800, -- 50 MiB
  ARRAY[
    'application/pdf',
    'image/jpeg',
    'image/jpg',
    'image/png',
    'image/webp',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
    'application/vnd.ms-excel',
    'text/csv',
    'application/msword',
    'application/vnd.openxmlformats-officedocument.wordprocessingml.document'
  ]
) ON CONFLICT (id) DO NOTHING;

-- Bucket: imports (privado, arquivos de importação CSV/XLSX)
INSERT INTO storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
VALUES (
  'imports',
  'imports',
  false,
  52428800, -- 50 MiB
  ARRAY[
    'text/csv',
    'text/plain',
    'application/vnd.ms-excel',
    'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet'
  ]
) ON CONFLICT (id) DO NOTHING;

-- ─── Políticas de acesso — bucket: documents ────────────────────────────────
-- Padrão de path: <user_id>/<filename>
-- O primeiro componente do path deve ser o user_id do usuário autenticado.

DROP POLICY IF EXISTS "Users upload own documents" ON storage.objects;
CREATE POLICY "Users upload own documents"
  ON storage.objects FOR INSERT
  WITH CHECK (
    bucket_id = 'documents'
    AND auth.uid()::text = (storage.foldername(name))[1]
  );

DROP POLICY IF EXISTS "Users read own documents" ON storage.objects;
CREATE POLICY "Users read own documents"
  ON storage.objects FOR SELECT
  USING (
    bucket_id = 'documents'
    AND auth.uid()::text = (storage.foldername(name))[1]
  );

DROP POLICY IF EXISTS "Users update own documents" ON storage.objects;
CREATE POLICY "Users update own documents"
  ON storage.objects FOR UPDATE
  USING (
    bucket_id = 'documents'
    AND auth.uid()::text = (storage.foldername(name))[1]
  );

DROP POLICY IF EXISTS "Users delete own documents" ON storage.objects;
CREATE POLICY "Users delete own documents"
  ON storage.objects FOR DELETE
  USING (
    bucket_id = 'documents'
    AND auth.uid()::text = (storage.foldername(name))[1]
  );

-- ─── Políticas de acesso — bucket: imports ──────────────────────────────────

DROP POLICY IF EXISTS "Users upload own imports" ON storage.objects;
CREATE POLICY "Users upload own imports"
  ON storage.objects FOR INSERT
  WITH CHECK (
    bucket_id = 'imports'
    AND auth.uid()::text = (storage.foldername(name))[1]
  );

DROP POLICY IF EXISTS "Users read own imports" ON storage.objects;
CREATE POLICY "Users read own imports"
  ON storage.objects FOR SELECT
  USING (
    bucket_id = 'imports'
    AND auth.uid()::text = (storage.foldername(name))[1]
  );

DROP POLICY IF EXISTS "Users delete own imports" ON storage.objects;
CREATE POLICY "Users delete own imports"
  ON storage.objects FOR DELETE
  USING (
    bucket_id = 'imports'
    AND auth.uid()::text = (storage.foldername(name))[1]
  );
