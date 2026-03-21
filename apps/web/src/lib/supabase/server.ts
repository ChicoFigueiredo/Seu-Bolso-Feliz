import { createServerClient } from "@supabase/ssr";
import { cookies } from "next/headers";
import type { Database } from "@sbf/shared-types";

export async function createClient() {
  const cookieStore = await cookies();

  return createServerClient<Database>(
    process.env.NEXT_PUBLIC_SUPABASE_URL!,
    process.env.NEXT_PUBLIC_SUPABASE_ANON_KEY!,
    {
      cookies: {
        getAll() {
          return cookieStore.getAll();
        },
        setAll(cookiesToSet) {
          try {
            cookiesToSet.forEach(({ name, value, options }) =>
              cookieStore.set(name, value, options),
            );
          } catch {
            // setAll pode falhar em Server Components (read-only).
            // Isso é esperado quando chamado de um Server Component.
            // O middleware vai cuidar do refresh do token.
          }
        },
      },
    },
  );
}
