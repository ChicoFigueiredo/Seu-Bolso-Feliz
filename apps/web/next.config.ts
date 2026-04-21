import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  async redirects() {
    return [
      // Rotas de ingestão que saíram do sidebar principal
      // → redirecionam para a nova localização (Settings Avançado ou Documentos)
      {
        source: "/dashboard/ingestion",
        destination: "/dashboard/documents",
        permanent: true, // 308
      },
      {
        source: "/dashboard/ingestion/documents",
        destination: "/dashboard/documents",
        permanent: true,
      },
      {
        source: "/dashboard/ingestion/review",
        destination: "/dashboard/settings#avancado",
        permanent: true,
      },
      {
        source: "/dashboard/ingestion/patterns",
        destination: "/dashboard/settings#avancado",
        permanent: true,
      },
      {
        source: "/dashboard/ingestion/logs",
        destination: "/dashboard/settings#avancado",
        permanent: true,
      },
    ];
  },
};

export default nextConfig;
