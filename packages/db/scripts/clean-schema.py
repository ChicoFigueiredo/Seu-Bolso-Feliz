#!/usr/bin/env python3
"""
Remove RLS/Auth/Storage-dependent objects from a Supabase pg_dump --schema-only
output so it can be applied to a plain Neon Postgres instance (no auth.* schema,
no RLS).

Categories removed:
  A. CREATE POLICY statements (every one, whole comment-header + statement block)
  B. ALTER TABLE ... ENABLE ROW LEVEL SECURITY / FORCE ROW LEVEL SECURITY
  C. FK constraints that reference auth.users(id) (ownership FK per user-owned
     table -- there's no auth.users table on Neon, so this FK can never be
     satisfied there; the app enforces ownership at the query layer instead,
     per ADR-009's explicit-WHERE-user_id model)
  D. Whole functions whose ONLY way to know "the current user" is auth.uid()/
     auth.role() with no user-id parameter at all (fn_set_secret,
     generate_financial_periods, get_financial_period_for_date,
     increment_session_tokens, register_pattern_feedback, search_suppliers).
     These are genuinely Supabase-Auth-session-shaped and can't be salvaged by
     deleting a line -- porting them for real is a Fase 2+ business-logic task,
     not part of "port the DDL." Verified before removal: none of them is
     called by any trigger, view, or other function in this dump.
  E. Inside functions that already take an explicit p_user_id parameter and use
     auth.uid() only as a *belt-and-braces* consistency check
     (`IF auth.uid() IS NOT NULL AND auth.uid() <> p_user_id THEN RAISE ...`),
     remove just that guard clause (plus its explanatory comment lines) and
     keep the rest of the function -- ownership is still enforced by the
     p_user_id parameter used in the function's WHERE clauses.
"""
import re
import sys

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
            if fname in {
                "fn_set_secret",
                "generate_financial_periods",
                "get_financial_period_for_date",
                "increment_session_tokens",
                "register_pattern_feedback",
                "search_suppliers",
            }:
                drop = True

        if drop:
            drop_ranges.append((s, e))

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
    # Pattern 1: the three functions that share the exact same 4-line guard.
    guard_pattern_a = re.compile(
        r"[ \t]*IF auth\.uid\(\) IS NOT NULL AND auth\.uid\(\) <> p_user_id THEN\n"
        r"[ \t]*RAISE EXCEPTION 'user mismatch' USING ERRCODE = 'insufficient_privilege';\n"
        r"[ \t]*END IF;\n\n?"
    )
    text2, n_a = guard_pattern_a.subn("", text2)

    # Pattern 2: fn_materialize_draft_record's guard, preceded by an explanatory
    # comment block that also mentions auth.uid()/authenticated/service_role.
    guard_pattern_b = re.compile(
        r"[ \t]*-- auth\.uid\(\) é NULL sob service_role, então p_user_id precisa ser\n"
        r"[ \t]*-- parâmetro — o que o tornaria forjável por qualquer 'authenticated'\.\n"
        r"[ \t]*-- Com JWT de usuário, fica preso ao chamador; com service_role \(que já é\n"
        r"[ \t]*-- onipotente\), qualquer usuário é permitido\.\n"
        r"[ \t]*IF auth\.uid\(\) IS NOT NULL AND auth\.uid\(\) <> p_user_id THEN\n"
        r"[ \t]*RAISE EXCEPTION 'user mismatch' USING ERRCODE = 'insufficient_privilege';\n"
        r"[ \t]*END IF;\n\n?"
    )
    text2, n_b = guard_pattern_b.subn("", text2)

    # Pattern A already stripped the bare guard everywhere (including inside
    # fn_materialize_draft_record, which pairs it with the 4-line explanatory
    # comment above). That leaves the comment orphaned since pattern B never
    # got a chance to match the combined form -- strip the orphan too.
    orphan_comment = re.compile(
        r"[ \t]*-- auth\.uid\(\) é NULL sob service_role, então p_user_id precisa ser\n"
        r"[ \t]*-- parâmetro — o que o tornaria forjável por qualquer 'authenticated'\.\n"
        r"[ \t]*-- Com JWT de usuário, fica preso ao chamador; com service_role \(que já é\n"
        r"[ \t]*-- onipotente\), qualquer usuário é permitido\.\n"
    )
    text2, n_c = orphan_comment.subn("", text2)

    if dst_path == "-":
        sys.stdout.write(text2)
    else:
        with open(dst_path, "w", encoding="utf-8") as f:
            f.write(text2)

    print(f"Blocks dropped: {len(drop_ranges)}", file=sys.stderr)
    print(f"Guard pattern A removed: {n_a}", file=sys.stderr)
    print(f"Guard pattern B removed: {n_b}", file=sys.stderr)
    print(f"Orphan comment removed: {n_c}", file=sys.stderr)

    leftover = re.search(r"(?i)auth\.|create policy|row level security", text2)
    if leftover:
        print("ERRO: ainda sobrou referência a auth./RLS após a limpeza.", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    dst = sys.argv[2] if len(sys.argv) > 2 else "-"
    main(sys.argv[1], dst)
