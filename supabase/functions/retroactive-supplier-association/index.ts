import { createClient } from "https://esm.sh/@supabase/supabase-js@2";

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
};

interface AssociationCandidate {
  transaction_id: string;
  description: string;
  event_date: string;
  amount: number;
  suggested_supplier_id: string;
  suggested_supplier_name: string;
  match_source: "alias_exact" | "alias_fuzzy" | "name_fuzzy";
  confidence: number;
}

interface AssociationConfirmation {
  transaction_id: string;
  supplier_id: string;
}

Deno.serve(async (req) => {
  // Responder preflight CORS
  if (req.method === "OPTIONS") {
    return new Response("ok", { headers: corsHeaders });
  }

  const authHeader = req.headers.get("Authorization");
  if (!authHeader) {
    return new Response(JSON.stringify({ error: "Missing authorization" }), {
      status: 401,
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  }

  const supabaseUrl = Deno.env.get("SUPABASE_URL")!;
  const serviceRoleKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!;

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
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  }

  const supabase = createClient(supabaseUrl, serviceRoleKey);

  // GET — buscar candidatos para associação
  // POST — confirmar associações em lote
  if (req.method === "GET") {
    return await handleSuggest(supabase, user.id);
  }

  if (req.method === "POST") {
    let body: { confirmations: AssociationConfirmation[] };
    try {
      body = await req.json();
    } catch {
      return new Response(JSON.stringify({ error: "Invalid JSON body" }), {
        status: 400,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      });
    }
    return await handleConfirm(supabase, user.id, body.confirmations);
  }

  return new Response(JSON.stringify({ error: "Method not allowed" }), {
    status: 405,
    headers: { ...corsHeaders, "Content-Type": "application/json" },
  });
});

async function handleSuggest(
  supabase: ReturnType<typeof createClient>,
  userId: string,
): Promise<Response> {
  // Buscar transações sem supplier_id
  const { data: unlinked, error: txError } = await supabase
    .from("transactions")
    .select("id, description, event_date, amount")
    .eq("user_id", userId)
    .is("supplier_id", null)
    .not("description", "is", null)
    .order("event_date", { ascending: false })
    .limit(200);

  if (txError) {
    return new Response(JSON.stringify({ error: txError.message }), {
      status: 500,
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  }

  if (!unlinked || unlinked.length === 0) {
    return new Response(JSON.stringify({ candidates: [] }), {
      status: 200,
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  }

  // Buscar aliases e nomes de fornecedores ativos
  const { data: aliases } = await supabase
    .from("supplier_aliases")
    .select("supplier_id, alias_name, suppliers!inner(name)")
    .eq("user_id", userId)
    .eq("is_active", true);

  const { data: suppliers } = await supabase
    .from("suppliers")
    .select("id, name")
    .eq("user_id", userId)
    .eq("is_active", true);

  const candidates: AssociationCandidate[] = [];

  for (const tx of unlinked) {
    if (!tx.description) continue;
    const desc = tx.description.toLowerCase();

    // 1. Match exato por alias (case-insensitive)
    const aliasMatch = aliases?.find((a) => desc.includes(a.alias_name.toLowerCase()));
    if (aliasMatch) {
      candidates.push({
        transaction_id: tx.id,
        description: tx.description,
        event_date: tx.event_date,
        amount: tx.amount,
        suggested_supplier_id: aliasMatch.supplier_id,
        suggested_supplier_name: (aliasMatch.suppliers as { name: string }).name,
        match_source: "alias_exact",
        confidence: 0.95,
      });
      continue;
    }

    // 2. Match por nome do fornecedor (substring)
    const nameMatch = suppliers?.find((s) => desc.includes(s.name.toLowerCase()));
    if (nameMatch) {
      candidates.push({
        transaction_id: tx.id,
        description: tx.description,
        event_date: tx.event_date,
        amount: tx.amount,
        suggested_supplier_id: nameMatch.id,
        suggested_supplier_name: nameMatch.name,
        match_source: "name_fuzzy",
        confidence: 0.8,
      });
    }
  }

  return new Response(JSON.stringify({ candidates }), {
    status: 200,
    headers: { ...corsHeaders, "Content-Type": "application/json" },
  });
}

async function handleConfirm(
  supabase: ReturnType<typeof createClient>,
  userId: string,
  confirmations: AssociationConfirmation[],
): Promise<Response> {
  if (!confirmations || confirmations.length === 0) {
    return new Response(JSON.stringify({ error: "No confirmations provided" }), {
      status: 400,
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  }

  let updated = 0;
  const errors: string[] = [];

  for (const conf of confirmations) {
    const { error } = await supabase
      .from("transactions")
      .update({ supplier_id: conf.supplier_id })
      .eq("id", conf.transaction_id)
      .eq("user_id", userId);

    if (error) {
      errors.push(`${conf.transaction_id}: ${error.message}`);
    } else {
      updated++;
    }
  }

  // Registrar no audit_log
  await supabase.from("audit_logs").insert({
    user_id: userId,
    action: "retroactive_supplier_association",
    entity_type: "transaction",
    details: {
      total_confirmed: confirmations.length,
      total_updated: updated,
      errors: errors.length > 0 ? errors : undefined,
    },
  });

  return new Response(
    JSON.stringify({ success: true, updated, errors: errors.length > 0 ? errors : undefined }),
    { status: 200, headers: { ...corsHeaders, "Content-Type": "application/json" } },
  );
}
