/**
 * Todas as fontes decididas para o inventário, as que entram e as que ficam
 * de fora com o motivo. Registrar antes de varrer faz a matriz de cobertura
 * mostrar "ausente" em vez de simplesmente não listar.
 */

export type TipoDeFonte =
  "disco" | "nuvem" | "email" | "celular" | "mensageiro" | "conexao" | "banco" | "app" | "rh";

export interface Fonte {
  id: string;
  tipo: TipoDeFonte;
  descricao: string;
  raiz?: string;
  escopo: "dentro" | "fora";
  motivo?: string;
}

export const FONTES: readonly Fonte[] = [
  {
    id: "disco:home",
    tipo: "disco",
    descricao: "Perfil Linux",
    raiz: "/home/chico",
    escopo: "dentro",
  },
  { id: "disco:d", tipo: "disco", descricao: "Disco D (ext4)", raiz: "/mnt/d", escopo: "dentro" },
  {
    id: "disco:p",
    tipo: "disco",
    descricao: "Disco Develop (NTFS)",
    raiz: "/mnt/p",
    escopo: "dentro",
  },
  {
    id: "disco:p2",
    tipo: "disco",
    descricao: "Disco Reserva3T_Dev (NTFS)",
    raiz: "/mnt/p2",
    escopo: "dentro",
  },
  {
    id: "disco:e",
    tipo: "disco",
    descricao: "Disco Estudo (NTFS)",
    raiz: "/mnt/e",
    escopo: "dentro",
  },
  {
    id: "disco:e2",
    tipo: "disco",
    descricao: "Disco Reserva7T_Estudo (NTFS)",
    raiz: "/mnt/e2",
    escopo: "dentro",
  },
  {
    id: "disco:bak",
    tipo: "disco",
    descricao: "Disco Bak4T (NTFS)",
    raiz: "/mnt/bak",
    escopo: "dentro",
  },
  {
    id: "nuvem:onedrive",
    tipo: "nuvem",
    descricao: "OneDrive ao vivo (rclone, só leitura)",
    escopo: "dentro",
  },
  {
    id: "nuvem:gdrive",
    tipo: "nuvem",
    descricao: "Google Drive ao vivo (rclone, só leitura)",
    escopo: "dentro",
  },
  { id: "email:gmail", tipo: "email", descricao: "Gmail, todo o histórico", escopo: "dentro" },
  {
    id: "email:outlook",
    tipo: "email",
    descricao: "outlook.com.br ao vivo (Thunderbird + OAuth)",
    escopo: "dentro",
  },
  {
    id: "celular",
    tipo: "celular",
    descricao: "Android: WhatsApp/Media, DCIM, Screenshots, Download (cópia USB)",
    escopo: "dentro",
  },
  {
    id: "celular:sms",
    tipo: "celular",
    descricao: "SMS (SMS Backup & Restore, XML)",
    escopo: "dentro",
  },
  {
    id: "mensageiro:whatsapp",
    tipo: "mensageiro",
    descricao: "WhatsApp: conversas exportadas com mídia (texto + comprovantes)",
    escopo: "dentro",
  },
  {
    id: "mensageiro:telegram",
    tipo: "mensageiro",
    descricao: "Telegram: conversas pessoais e Mensagens Salvas, 12 meses",
    escopo: "dentro",
  },
  {
    id: "conexao:pluggy",
    tipo: "conexao",
    descricao: "Pluggy (CAIXA, C6, Nubank, 2 cartões)",
    escopo: "dentro",
  },
  {
    id: "banco:faturas",
    tipo: "banco",
    descricao: "Faturas dos 2 cartões, 12 meses",
    escopo: "dentro",
  },
  {
    id: "banco:emprestimos",
    tipo: "banco",
    descricao: "Consignados e empréstimos: contratos e saldo devedor",
    escopo: "dentro",
  },
  {
    id: "banco:ofx",
    tipo: "banco",
    descricao: "Extratos OFX de CAIXA, C6 e Nubank",
    escopo: "dentro",
  },
  {
    id: "banco:beneficios",
    tipo: "banco",
    descricao: "Vale-refeição / alimentação",
    escopo: "dentro",
  },
  {
    id: "app:pedidos",
    tipo: "app",
    descricao: "iFood, Uber, 99, Amazon, Mercado Livre, Shopee (exportação LGPD)",
    escopo: "dentro",
  },
  {
    id: "rh:contracheques",
    tipo: "rh",
    descricao: "Portal de RH da CAIXA, 12 meses, sessão supervisionada",
    escopo: "dentro",
  },
  {
    id: "email:gmail-ost",
    tipo: "email",
    descricao: "Outlook .ost de 19 GB da conta Gmail",
    escopo: "fora",
    motivo: "redundante com o Gmail ao vivo",
  },
  {
    id: "email:estacio",
    tipo: "email",
    descricao: "Contas alunos.estacio.br (.ost)",
    escopo: "fora",
    motivo: "não financeiro",
  },
  {
    id: "nuvem:mega",
    tipo: "nuvem",
    descricao: "MEGA ao vivo",
    escopo: "fora",
    motivo: "não recebe arquivos hoje; a cópia local entra pelo scan dos discos",
  },
  {
    id: "nuvem:dropbox",
    tipo: "nuvem",
    descricao: "Dropbox ao vivo",
    escopo: "fora",
    motivo: "não recebe arquivos hoje; a cópia local entra pelo scan dos discos",
  },
];
