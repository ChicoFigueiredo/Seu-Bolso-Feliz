# ADR-014 — Inventário e cofre moram no desktop, e o caminho de um documento depende da fonte

**Data:** 2026-11-12 · **Status:** aceita · **Revisa:** ADR-011

## Contexto e decisão

O ADR-011 pôs o inventário fora do app e fora da nuvem. Com o desktop, a primeira
metade muda e a segunda fica.

**Decisão:**

- **Onde moram:**
  - O desktop absorve o inventário e o cofre. Os coletores Bun são portados um a um.
  - O inventário nunca sincroniza.
  - O worker local roda como serviço do usuário (systemd user, launchd ou serviço do
    Windows), então fechar a janela não para a coleta.
- **Fontes em massa:**
  - São as que trazem quase só coisa não financeira: discos, nuvens, WhatsApp, conversas
    do Telegram e o histórico do e-mail.
  - Passam pelo inventário e só chegam ao servidor por **promoção**.
- **Fontes já filtradas:**
  - São o e-mail por consulta financeira, a Pluggy, o bot do Telegram e os roteiros.
  - Entregam direto na caixa de entrada.
  - Podem rodar na VPS, 24 horas por dia. A exceção são os roteiros, que rodam na
    estação porque MFA e CAPTCHA dependem do usuário (ADR-016).
- **O cofre:**
  - Vira o arquivo completo de originais.
  - A sincronização traz para ele uma cópia de cada documento novo do servidor, e os
    roteiros gravam direto nele.
  - Nada sai do cofre, a não ser por promoção.

## Opções descartadas

- **Manter os scripts e deixar o desktop só lendo o SQLite.** Teríamos dois lugares
  operando o mesmo acervo.
- **Sincronizar o inventário inteiro.** São 9 milhões de achados, quase todos
  descartados, que iriam para o banco de produção. É o que o ADR-011 evitou.
- **Tudo pelo inventário.** A VPS não coletaria nada, e com a máquina desligada a coleta
  pararia.
- **Tudo direto na caixa de entrada.** Fotos de família e livros iriam parar no servidor.

## Consequências

- O verbete Inventário do `CONTEXT.md` agora cobre só as fontes em massa.
- O cofre deixa de depender do Vercel Blob para ter o acervo inteiro.
