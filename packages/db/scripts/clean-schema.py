#!/usr/bin/env python3
"""
Remove RLS/Auth/Storage-dependent objects from a Supabase pg_dump --schema-only
output so it can be applied to a plain Neon Postgres instance (no auth.* schema,
no RLS), and document -- inline, in the output itself -- every gap that this
leaves for a later ADR-009 phase to close.

Categories removed:
  A. CREATE POLICY statements (every one, whole comment-header + statement block)
  B. ALTER TABLE ... ENABLE ROW LEVEL SECURITY / FORCE ROW LEVEL SECURITY
  C. FK constraints that reference auth.users(id) (ownership FK per user-owned
     table -- there's no auth.users table on Neon, so this FK can never be
     satisfied there; the app enforces ownership at the query layer instead,
     per ADR-009's explicit-WHERE-user_id model). 30 of these 46 constraints
     carried ON DELETE CASCADE -- removing them loses referential integrity,
     not just authorization (documented inline, see NOTA block below).
  D. Whole functions whose ONLY way to know "the current user" is auth.uid()/
     auth.role() with no user-id parameter at all (fn_set_secret,
     generate_financial_periods, get_financial_period_for_date,
     increment_session_tokens, register_pattern_feedback, search_suppliers).
     These are genuinely Supabase-Auth-session-shaped and can't be salvaged by
     deleting a line -- porting them for real is a Fase 2+ business-logic task,
     not part of "port the DDL." Verified before removal: none of them is
     called by any trigger, view, or other function in this dump.
  E. Inside functions that already take an explicit p_user_id parameter and use
     auth.uid() only to bind that parameter to the JWT identity
     (`IF auth.uid() IS NOT NULL AND auth.uid() <> p_user_id THEN RAISE ...`),
     remove just that guard clause (plus its explanatory comment lines) and
     keep the rest of the function. This check was NOT redundant in Supabase
     (see NOTA block below for why) -- it's dropped here only because Neon has
     no PostgREST/JWT/auth.uid() to check against, not because it was
     unnecessary.

Also documented/fixed inline in the output (not just here):
  - CREATE SCHEMA public -> CREATE SCHEMA IF NOT EXISTS public, so applying
    this file works against a brand-new Neon database (which already has a
    `public` schema out of the box) without a manual DROP SCHEMA first.
  - CREATE EXTENSION IF NOT EXISTS pg_trgm, re-added because
    `pg_dump --schema=public` drops extension DDL even when the extension's
    objects (like the gin_trgm_ops operator class two indexes depend on) are
    registered inside the public schema.
  - A NOTA block (right after the pg_trgm extension) spelling out, in the
    file itself, the three things this port does NOT carry over and that a
    later ADR-009 phase has to address: the `private` schema (crypto key
    material for encrypt_secret/decrypt_secret/fn_get_secrets), the lost
    ON DELETE CASCADE on 30 user-owned tables, and the JWT-binding check
    removed from 4 kept functions.
"""
import re
import sys

DROPPED_SESSION_ONLY_FUNCTIONS = {
    "fn_set_secret",
    "generate_financial_periods",
    "get_financial_period_for_date",
    "increment_session_tokens",
    "register_pattern_feedback",
    "search_suppliers",
}

# Functions kept, but whose auth.uid()-vs-p_user_id guard was stripped (Category E).
JWT_BINDING_GUARD_REMOVED_FROM = {
    "fn_get_secrets",
    "fn_mark_secret_used",
    "fn_materialize_draft_record",
    "fn_upsert_financial_obligation",
}

# Functions kept, but that call private.get_crypto_key() (directly or via
# decrypt_secret) -- the `private` schema itself was never in this dump
# (pg_dump was run with --schema=public), so these fail at *runtime* on Neon
# even though CREATE FUNCTION itself succeeds (check_function_bodies=false).
PRIVATE_SCHEMA_DEPENDENT_FUNCTIONS = {
    "encrypt_secret",
    "decrypt_secret",
    "fn_get_secrets",  # calls decrypt_secret() internally
}

NOTA_BLOCK = """--
-- NOTA (ADR-009 Fase 1 -> Fase 2 -- ver
-- docs/superpowers/plans/2026-08-25-adr-009-fase1-neon-drizzle.md e
-- .superpowers/sdd/2026-08-25-adr-009-fase1-neon-drizzle/task-3-report.md):
--
-- 1. O schema `private` NAO foi portado (private.crypto_keys,
--    private.get_crypto_key -- ver
--    supabase/migrations/20260727184000_fix_secrets_encryption.sql), porque
--    `pg_dump --schema=public` nunca o incluiu. A chave de criptografia em
--    si e DADO, nao DDL -- recria-la (ou decidir uma alternativa) e decisao
--    de Fase 2, nao desta task. Ate la, chamar public.encrypt_secret(),
--    public.decrypt_secret() ou public.fn_get_secrets() no Neon falha em
--    runtime: function private.get_crypto_key(integer) does not exist.
--
-- 2. As FK para auth.users(id) foram removidas por completo (auth.users nao
--    existe no Neon). 30 das 46 removidas tinham ON DELETE CASCADE --
--    ou seja, isso tambem derrubou integridade referencial, nao so
--    autorizacao: apagar um "usuario" hoje no Neon deixaria orfaos em
--    audit_logs, cards, categories, consumption_metrics, document_patterns,
--    documents_legacy, financial_obligation_evidences,
--    financial_obligation_identity_keys, financial_obligations,
--    financial_periods, financial_products, import_jobs,
--    ingestion_checkpoints, institutions, liabilities,
--    liability_installments, pattern_feedback, recurring_instances,
--    recurring_templates, statement_cycles, statement_items,
--    supplier_aliases, supplier_contracts, supplier_tags, suppliers, tags,
--    transactions, transfers, user_financial_preferences e user_secrets.
--    Fase 2 provavelmente precisa de uma tabela `users` propria + FKs
--    reinstaladas.
--
-- 3. fn_get_secrets, fn_mark_secret_used, fn_materialize_draft_record e
--    fn_upsert_financial_obligation tiveram a checagem
--    `auth.uid() IS NOT NULL AND auth.uid() <> p_user_id` removida. Essa
--    checagem NAO era redundante em Supabase: ela amarrava p_user_id a
--    identidade do JWT nestas funcoes SECURITY DEFINER expostas via
--    PostgREST -- sem ela, qualquer portador de JWT valido podia chamar a
--    RPC passando o p_user_id de outra pessoa. Nao virou Critical aqui
--    porque o Neon so e alcancavel via DATABASE_URL de servidor (sem
--    PostgREST, sem auth.uid(), essa classe de ataque nao existe hoje) --
--    mas Fase 2 PRECISA amarrar p_user_id a sessao real por outro mecanismo
--    antes de expor qualquer uma dessas 4 funcoes a um cliente nao confiavel.
--

"""

DUMP_COMPLETE_FOOTER = """
--
-- PostgreSQL database dump complete
--
"""


def _annotate_function_headers(text):
    """Insert a one-line back-reference comment right after the pg_dump
    "-- Name: fname(...); Type: FUNCTION; ..." header, for every function
    this script's cleanup touched -- so someone reading the function in
    place sees the gap without having to go find the NOTA block first."""

    header_line_re = re.compile(r"^-- Name: (\w+)\(.*; Type: FUNCTION; Schema: public; Owner: -$")

    out_lines = []
    for line in text.split("\n"):
        out_lines.append(line)
        m = header_line_re.match(line)
        if not m:
            continue
        fname = m.group(1)
        notes = []
        if fname in PRIVATE_SCHEMA_DEPENDENT_FUNCTIONS:
            notes.append(
                "-- NOTA: depende de private.get_crypto_key(), nao portado nesta task -- ver NOTA no topo do arquivo (item 1)."
            )
        if fname in JWT_BINDING_GUARD_REMOVED_FROM:
            notes.append(
                "-- NOTA: checagem auth.uid() <> p_user_id removida aqui NAO era redundante -- ver NOTA no topo do arquivo (item 3)."
            )
        out_lines.extend(notes)
    return "\n".join(out_lines)


def main(src_path, dst_path):
    with open(src_path, encoding="utf-8") as f:
        text = f.read()

    lines = text.split("\n")
    n = len(lines)

    # --- Pass 1: block-level removal using the pg_dump "-- Name: X; Type: Y --" header ---
    # A block is: "--" / "-- Name: ...; Type: T; Schema: ...; Owner: -" / "--" / (blank)?
    # followed by the actual statement, up to (but not including) the next such
    # header or EOF. We keep the block's own trailing blank line handling simple:
    # we just drop full blocks and let the surrounding blank lines fall where
    # they may (harmless extra blank lines are fine in SQL).

    header_re = re.compile(r"^-- Name: .*; Type: (\w[\w ]*\w); Schema: ")

    # Find all header start indices (index of first "--" of the 3-line header)
    starts = []
    i = 0
    while i < n:
        if lines[i] == "--" and i + 1 < n and header_re.match(lines[i + 1] or ""):
            starts.append(i)
            i += 1
        else:
            i += 1

    # Compute block end = next header start - 1, or n-1 for the last block
    drop_ranges = []  # list of (start, end) inclusive, 0-indexed, to delete
    for idx, s in enumerate(starts):
        e = (starts[idx + 1] - 1) if idx + 1 < len(starts) else (n - 1)
        header_line = lines[s + 1]
        m = header_re.match(header_line)
        obj_type = m.group(1)
        block_text = "\n".join(lines[s:e + 1])

        drop = False
        if obj_type == "POLICY":
            drop = True
        elif obj_type == "ROW SECURITY":
            drop = True
        elif obj_type == "FK CONSTRAINT" and "auth.users" in block_text:
            drop = True
        elif obj_type == "FUNCTION":
            fname_match = re.search(r"Name: (\w+)\(", header_line)
            fname = fname_match.group(1) if fname_match else None
            if fname in DROPPED_SESSION_ONLY_FUNCTIONS:
                drop = True

        if drop:
            drop_ranges.append((s, e))

    # The last block in the file is always the last CREATE POLICY, so its
    # range swallows pg_dump's own trailing "-- PostgreSQL database dump
    # complete --" comment too. Re-added explicitly below (DUMP_COMPLETE_FOOTER).
    keep_mask = [True] * n
    for s, e in drop_ranges:
        for k in range(s, e + 1):
            keep_mask[k] = False

    kept_lines = [
        ln
        for k, ln in zip(keep_mask, lines)
        if k and not ln.startswith("\\restrict ") and not ln.startswith("\\unrestrict ")
    ]
    text2 = "\n".join(kept_lines)

    # --- Pass 2: surgical guard-clause removal inside functions we keep ---
    # The four functions above share the exact same 4-line guard; one of them
    # (fn_materialize_draft_record) additionally has a 4-line explanatory
    # comment right above it that needs stripping too.
    guard_pattern = re.compile(
        r"[ \t]*IF auth\.uid\(\) IS NOT NULL AND auth\.uid\(\) <> p_user_id THEN\n"
        r"[ \t]*RAISE EXCEPTION 'user mismatch' USING ERRCODE = 'insufficient_privilege';\n"
        r"[ \t]*END IF;\n\n?"
    )
    text2, n_guard = guard_pattern.subn("", text2)

    orphan_comment = re.compile(
        r"[ \t]*-- auth\.uid\(\) é NULL sob service_role, então p_user_id precisa ser\n"
        r"[ \t]*-- parâmetro — o que o tornaria forjável por qualquer 'authenticated'\.\n"
        r"[ \t]*-- Com JWT de usuário, fica preso ao chamador; com service_role \(que já é\n"
        r"[ \t]*-- onipotente\), qualquer usuário é permitido\.\n"
    )
    text2, n_orphan = orphan_comment.subn("", text2)

    # --- Pass 3: make the schema creation idempotent on a brand-new Neon DB ---
    # Neon (like any fresh Postgres database) already has a `public` schema.
    # Without IF NOT EXISTS, applying this file to a never-touched Neon DB
    # fails immediately on line 1 with "schema public already exists".
    text2, n_schema = re.subn(
        r"^CREATE SCHEMA public;$", "CREATE SCHEMA IF NOT EXISTS public;", text2, count=1, flags=re.M
    )

    # --- Pass 4: re-add the pg_trgm extension pg_dump --schema=public dropped ---
    pg_trgm_block = (
        "\n\n--\n"
        "-- Name: pg_trgm; Type: EXTENSION; Schema: public; Owner: -\n"
        "--\n"
        "-- NOTA: pg_dump --schema=public nao inclui CREATE EXTENSION (extensoes\n"
        "-- nao pertencem ao schema public mesmo quando seus operadores sao\n"
        "-- registrados nele) -- reinstalada aqui porque os indices GIN\n"
        "-- idx_supplier_aliases_trgm e idx_suppliers_name_trgm dependem do\n"
        "-- operator class public.gin_trgm_ops. Confirmado disponivel no Neon\n"
        "-- via pg_available_extensions antes de adicionar esta linha.\n"
        "--\n\n"
        "CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;\n"
    )
    anchor = "COMMENT ON SCHEMA public IS 'standard public schema';\n"
    idx = text2.find(anchor)
    if idx == -1:
        print("ERRO: âncora do schema public não encontrada -- não consegui inserir pg_trgm.", file=sys.stderr)
        sys.exit(1)
    insert_at = idx + len(anchor)
    text2 = text2[:insert_at] + pg_trgm_block + NOTA_BLOCK.rstrip("\n") + "\n" + text2[insert_at:]

    # --- Pass 5: per-function back-reference comments (Category 1/3 gaps) ---
    text2 = _annotate_function_headers(text2)

    # --- Pass 6: restore the dump-complete footer the last dropped block ate ---
    text2 = text2.rstrip("\n") + "\n" + DUMP_COMPLETE_FOOTER

    if dst_path == "-":
        sys.stdout.write(text2)
    else:
        with open(dst_path, "w", encoding="utf-8") as f:
            f.write(text2)

    print(f"Blocks dropped: {len(drop_ranges)}", file=sys.stderr)
    print(f"Guard clauses removed: {n_guard}", file=sys.stderr)
    print(f"Orphan comment removed: {n_orphan}", file=sys.stderr)
    print(f"CREATE SCHEMA made idempotent: {n_schema}", file=sys.stderr)

    # Scoped to non-comment lines: the NOTA block and the per-function notes
    # inserted above (Passes 4/5) *intentionally* mention auth.uid()/
    # auth.users/auth.role() in `--` comments, to document the gaps this
    # port leaves open -- that's the point of Important 1/2/3 from the Task
    # 3 review, not a leftover. What must never survive is a real, executable
    # reference: CREATE POLICY, ENABLE/FORCE ROW LEVEL SECURITY, or
    # auth.uid()/auth.role()/auth.users inside actual DDL/plpgsql.
    functional_re = re.compile(r"(?i)auth\.uid\(\)|auth\.role\(\)|auth\.users|create policy|row level security")
    leftover = [
        ln for ln in text2.split("\n")
        if not ln.strip().startswith("--") and functional_re.search(ln)
    ]
    if leftover:
        print("ERRO: ainda sobrou referência funcional a auth./RLS após a limpeza:", file=sys.stderr)
        for ln in leftover:
            print(f"  {ln}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    dst = sys.argv[2] if len(sys.argv) > 2 else "-"
    main(sys.argv[1], dst)
