import { NextResponse, type NextRequest } from "next/server";
import { fechaSessao } from "@/lib/auth";
import { origemDe } from "@/lib/auth/url";

export async function POST(request: NextRequest) {
  await fechaSessao();
  return NextResponse.redirect(new URL("/login", origemDe(request)), { status: 303 });
}

// Sair por GET seria disparado por qualquer <img> apontando para cá.
export async function GET(request: NextRequest) {
  return NextResponse.redirect(new URL("/login", origemDe(request)));
}
