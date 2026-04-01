import { createClient } from "https://esm.sh/@supabase/supabase-js@2";

const corsHeaders = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type",
  "Access-Control-Allow-Methods": "POST, OPTIONS",
};

interface TriggerRequest {
  source_type?: string; // "gmail" | "local" | "manual"
  file_paths?: string[]; // para upload manual
}

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") {
    return new Response("ok", { headers: corsHeaders });
  }

  if (req.method !== "POST") {
    return new Response(JSON.stringify({ error: "Method not allowed" }), {
      status: 405,
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
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
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  }

  // Client com service_role para operações administrativas
  const adminClient = createClient(supabaseUrl, serviceRoleKey);

  try {
    const body: TriggerRequest = await req.json();
    const sourceType = body.source_type ?? "manual";

    // 1. Criar ingestion_run
    const { data: run, error: runError } = await adminClient
      .from("ingestion_runs")
      .insert({
        user_id: user.id,
        source_type: sourceType,
        status: "running",
        started_at: new Date().toISOString(),
      })
      .select()
      .single();

    if (runError) {
      throw new Error(`Failed to create ingestion run: ${runError.message}`);
    }

    // 2. Se há file_paths (upload manual), criar source_documents e ingestion_jobs
    let jobsCreated = 0;

    if (body.file_paths && body.file_paths.length > 0) {
      for (const filePath of body.file_paths) {
        // Criar source_document
        const fileName = filePath.split("/").pop() ?? filePath;
        const { data: doc, error: docError } = await adminClient
          .from("source_documents")
          .insert({
            user_id: user.id,
            file_name: fileName,
            file_path: filePath,
            source_type: sourceType,
            status: "pending",
          })
          .select()
          .single();

        if (docError) {
          // logar e continuar
          await adminClient.from("ingestion_logs").insert({
            user_id: user.id,
            run_id: run.id,
            level: "error",
            message: `Failed to create source_document for ${filePath}`,
            details: { error: docError.message },
          });
          continue;
        }

        // Criar ingestion_job
        const { error: jobError } = await adminClient.from("ingestion_jobs").insert({
          user_id: user.id,
          run_id: run.id,
          source_document_id: doc.id,
          status: "discovered",
          current_step: "discovered",
        });

        if (jobError) {
          await adminClient.from("ingestion_logs").insert({
            user_id: user.id,
            run_id: run.id,
            level: "error",
            message: `Failed to create ingestion_job for ${filePath}`,
            details: { error: jobError.message },
          });
          continue;
        }

        jobsCreated++;
      }
    }

    // 3. Log de sucesso
    await adminClient.from("ingestion_logs").insert({
      user_id: user.id,
      run_id: run.id,
      level: "info",
      message: `Ingestion triggered: ${jobsCreated} job(s) created`,
      details: { source_type: sourceType, jobs_created: jobsCreated },
    });

    return new Response(
      JSON.stringify({
        success: true,
        run_id: run.id,
        jobs_created: jobsCreated,
      }),
      {
        status: 200,
        headers: { ...corsHeaders, "Content-Type": "application/json" },
      },
    );
  } catch (err) {
    const message = err instanceof Error ? err.message : "Unknown error";

    // Tentar logar o erro
    try {
      await adminClient.from("ingestion_logs").insert({
        user_id: user.id,
        level: "error",
        message: `Trigger ingestion failed: ${message}`,
      });
    } catch {
      // ignore logging failure
    }

    return new Response(JSON.stringify({ error: message }), {
      status: 500,
      headers: { ...corsHeaders, "Content-Type": "application/json" },
    });
  }
});
