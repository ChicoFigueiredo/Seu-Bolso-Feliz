import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  webpack: (config) => {
    // react-pdf usa canvas e pdfjs-dist; evitar erros de SSR
    config.resolve.alias = {
      ...config.resolve.alias,
      canvas: false,
    };
    return config;
  },
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

      // Redesign da arquitetura de informação (shell novo em components/shell/):
      // rotas antigas em inglês → rotas novas em português. Só os caminhos que
      // ganharam página nova entram aqui; o resto (documents, suppliers,
      // reports, settings, import) continua no ar sem redirect por enquanto.
      {
        source: "/dashboard/transactions",
        destination: "/dashboard/movimentacoes",
        permanent: true,
      },
      {
        source: "/dashboard/cards",
        destination: "/dashboard/contas",
        permanent: true,
      },
      {
        source: "/dashboard/products",
        destination: "/dashboard/contas",
        permanent: true,
      },
      {
        source: "/dashboard/institutions",
        destination: "/dashboard/contas",
        permanent: true,
      },
      {
        source: "/dashboard/statements",
        destination: "/dashboard/contas",
        permanent: true,
      },
      {
        source: "/dashboard/liabilities",
        destination: "/dashboard/dividas",
        permanent: true,
      },
      {
        source: "/dashboard/recurring",
        destination: "/dashboard/recorrentes",
        permanent: true,
      },
      {
        source: "/dashboard/connections",
        destination: "/dashboard/conexoes",
        permanent: true,
      },
    ];
  },
};

export default nextConfig;
