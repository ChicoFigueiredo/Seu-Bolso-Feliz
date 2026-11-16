# ADR-017 — WhatsApp por dispositivo vinculado não oficial, só leitura e só em estação Linux

**Data:** 2026-11-12 · **Status:** aceita

## Contexto e decisão

O WhatsApp é uma fonte em massa de comprovantes e boletos. Não existe API oficial que
leia conversas pessoais.

**Decisão:**

- **Como conecta:** a conexão WhatsApp entra como dispositivo vinculado, com a
  biblioteca `whatsapp-rust` (MIT).
- **O que faz:** só lê mensagens e mídia, que vão para o inventário. Nunca envia; os
  avisos saem pelo Telegram.
- **Onde roda:** só em estação Linux, até o projeto qualificar Windows e macOS. O plano
  B é o `whatsmeow` como processo à parte.
- **Risco:** os termos do WhatsApp proíbem acesso automatizado não autorizado e preveem
  suspensão da conta. O CEO aceitou esse risco.

## Opções descartadas

- **Ler só o espelho do celular via Syncthing.** O risco seria zero; foi a recomendação,
  e o CEO recusou.
- **Dispositivo vinculado que também envia.** Mais risco de banimento.
- **API oficial Business.** Exige número separado e não lê as conversas pessoais.

## Consequências

- O celular precisa abrir o WhatsApp a cada 14 dias para manter o vínculo.
- Mensagens não entregues ficam guardadas por até 30 dias. Uma estação desligada por mais
  tempo que isso perde mensagens.
- A biblioteca não é oficial. Uma mudança de protocolo pode quebrar a conexão sem aviso.
