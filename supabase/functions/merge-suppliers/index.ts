import { createClient } from "https://esm.sh/@supabase/supabase-js@2";

interface MergeRequest {
  source_supplier_id: string;
  target_supplier_id: string;
}

interface MergeResult {
  migrated: Record<string, number>;
  source_name: string;
  target_name: string;
}

Deno.serve(async (req) => {
  if (req.method !== "POST") {
    return new Response(JSON.stringify({ error: "Method not allowed" }), {
      status: 405,
      headers: { "Content-Type": "application/json" },
    });
  }

  // Autenticar o usuário chamador
  const authHeader = req.headers.get("Authorization");
  if (!authHeader) {
    return new Response(JSON.stringify({ error: "Missing authorization" }), {
      status: 401,
      headers: { "Content-Type": "application/json" },
    });
  }

  const supabaseUrl = Deno.env.get("SUPABASE_URL")!;
  const serviceRoleKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;

  // Client do usuário para verificar identidade
  const userClient = createClient(supabaseUrl, Deno.env.get("SUPABASE_ANON_KEY")!, {
    global: { headers: { Authorization: authHeader } },
  });

  const {
    data: { user },
    error: authError,
  } = await userClient.auth.getUser();

  if (authError || !user) {
    return new Response(JSON.stringify({ error: "Unauthorized" }), {
      status: 401,
      headers: { "Content-Type": "application/json" },
    });
  }

  // Service role client para bypass de RLS durante merge atômico
  const supabase = createClient(supabaseUrl, serviceRoleKey);

  let body: MergeRequest;
  try {
    body = await req.json();
  } catch {
    return new Response(JSON.stringify({ error: "Invalid JSON body" }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    });
  }

  const { source_supplier_id, target_supplier_id } = body;

  if (!source_supplier_id || !target_supplier_id) {
    return new Response(
      JSON.stringify({ error: "source_supplier_id and target_supplier_id are required" }),
      { status: 400, headers: { "Content-Type": "application/json" } },
    );
  }

  if (source_supplier_id === target_supplier_id) {
    return new Response(JSON.stringify({ error: "Cannot merge a supplier with itself" }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    });
  }

  // Validar que ambos fornecedores pertencem ao usuário e estão ativos
  const { data: suppliers, error: fetchError } = await supabase
    .from("suppliers")
    .select("id, name, user_id, is_active")
    .in("id", [source_supplier_id, target_supplier_id]);

  if (fetchError || !suppliers || suppliers.length !== 2) {
    return new Response(JSON.stringify({ error: "One or both suppliers not found" }), {
      status: 404,
      headers: { "Content-Type": "application/json" },
    });
  }

  const source = suppliers.find((s) => s.id === source_supplier_id);
  const target = suppliers.find((s) => s.id === target_supplier_id);

  if (!source || !target) {
    return new Response(JSON.stringify({ error: "Supplier mismatch" }), {
      status: 404,
      headers: { "Content-Type": "application/json" },
    });
  }

  if (source.user_id !== user.id || target.user_id !== user.id) {
    return new Response(
      JSON.stringify({ error: "Suppliers do not belong to authenticated user" }),
      {
        status: 403,
        headers: { "Content-Type": "application/json" },
      },
    );
  }

  if (!source.is_active) {
    return new Response(JSON.stringify({ error: "Source supplier is already inactive" }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    });
  }

  // Executar merge atômico via SQL raw (transação única)
  // Conforme ADR-003 — 8 etapas
  const { data: result, error: mergeError } = await supabase.rpc("merge_suppliers", {
    p_user_id: user.id,
    p_source_id: source_supplier_id,
    p_target_id: target_supplier_id,
    p_source_name: source.name,
    p_target_name: target.name,
  });

  if (mergeError) {
    return new Response(JSON.stringify({ error: "Merge failed", details: mergeError.message }), {
      status: 500,
      headers: { "Content-Type": "application/json" },
    });
  }

  const mergeResult: MergeResult = {
    migrated: result as Record<string, number>,
    source_name: source.name,
    target_name: target.name,
  };

  return new Response(JSON.stringify({ success: true, result: mergeResult }), {
    status: 200,
    headers: { "Content-Type": "application/json" },
  });
});
