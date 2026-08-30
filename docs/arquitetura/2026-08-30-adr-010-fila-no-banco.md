# ADR-010 — A Vercel e os workers conversam por uma fila no Postgres

**Data:** 2026-08-30 · **Status:** aceita

## Contexto

Com a saída do Supabase, a aplicação na Vercel precisa mandar trabalho pesado
para dois lugares: o worker na VPS da Locaweb (que fica de pé 24/7) e o worker
na máquina do CEO, sob WSL (que tem `ocrmypdf`, a pasta monitorada e as senhas
de PDF). O prompt original propôs uma API privada autenticada entre a Vercel e
a VPS, com TLS e assinatura HMAC ou JWT de curta duração, e um túnel
Tailscale/WireGuard como alternativa. O CEO, ao revisar, propôs um túnel SSH
persistente entre os dois lados, com chave e porta combinada — motivado por
não querer nenhum endpoint exposto a ataque.

Três fatos mudaram a conta:

1. **A Vercel é serverless.** Uma function nasce e morre por requisição; não
   existe processo persistente para segurar um túnel SSH. Abrir sessão por
   operação custa handshake a cada chamada e obriga a guardar a chave privada
   de `root@` da VPS dentro do ambiente da Vercel — que passaria a ser o
   segredo mais valioso do sistema.
2. **O próprio prompt já vetava isso**: _"SSH deve ficar reservado à
   administração/deploy… evitar abrir sessões SSH a cada operação da
   aplicação."_
3. **A máquina do CEO fica atrás de NAT doméstico.** Qualquer desenho em que
   alguém _chama_ o worker local exige túnel ou IP fixo. Um desenho em que o
   worker _puxa_ não exige nada.

## Decisão

A comunicação é uma **fila na própria tabela `ingestion_jobs`**. A aplicação
grava a linha; os workers reivindicam com
`UPDATE … FROM (SELECT … FOR UPDATE SKIP LOCKED)` numa única instrução, e
acordam por `LISTEN/NOTIFY` disparado por trigger no insert. Verificado contra
o Neon real: `LISTEN/NOTIFY` funciona na conexão direta (`DATABASE_URL_UNPOOLED`)
e `SKIP LOCKED` também.

Cada job declara em `requires` as capacidades que exige (`ocr`, `pasta-local`,
`senha-pdf`) e cada worker anuncia as que oferece; um worker só reivindica job
cujas exigências cabem no que ele tem. Sem isso os dois brigariam pela mesma
fila e um job de pasta local acabaria na VPS, que não tem a pasta.

SSH fica reservado a deploy e administração, como o prompt mandava.

## Consequências

**Some a superfície de ataque em vez de protegê-la.** Não há endpoint,
certificado, segredo compartilhado nem rotação: a única credencial em jogo é a
`DATABASE_URL`, que os dois lados já precisavam ter.

**Funciona atrás de NAT.** O worker no WSL não precisa de porta aberta, IP fixo
nem túnel — ele puxa.

**Queda de rede vira job esperando na fila**, não job perdido. `reapStaleJobs`
devolve à fila o que ficou travado quando um worker morreu.

**O custo é latência**, limitada pelo `LISTEN/NOTIFY` a milissegundos no caminho
feliz e ao intervalo de polling quando a conexão de notificação cai — que
degrada sozinha, sem derrubar o worker.

**Limitação conhecida, ainda aberta:** um worker que morra no meio de um passo
deixa o job num estado intermediário. O reaper devolve o lock, mas nenhum claim
volta a enxergá-lo, porque `CLAIMABLE_STATUSES` não sabe para onde cada estado
intermediário retrocede. É herdado do poll antigo e precisa de decisão sobre a
máquina de estados, não de código novo.

## Rejeitado

**API HTTP com HMAC** (a proposta do prompt) e **túnel SSH persistente** (a
proposta do CEO) resolvem o problema de autenticar um canal. A fila faz o canal
deixar de existir — e o túnel SSH, especificamente, não é implementável a partir
de um runtime serverless. **Tailscale/WireGuard** resolveria o NAT, mas
adiciona um serviço de terceiros e um daemon em cada máquina para transportar o
que o banco já transporta.
