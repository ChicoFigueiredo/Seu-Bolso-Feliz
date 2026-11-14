# Spec 10 — Desktop, onda C: roteiros, RAG local e pontos de atenção

**Status:** decidida (o CEO pediu a onda C completa em 2026-11-14, com economia e supervisão séria)
· **Depende de:** ADR-015, ADR-016, spec 09

## Frente C1 — Roteiros (ADR-016)

Os **roteiros** navegam em sites sem API, como o e-CAC, a CAIXA e o internet banking, para
consultar e baixar documentos.

- **Navegador:**
  - Chrome visível, controlado por CDP (crate `chromiumoxide` ou equivalente, com licença
    permissiva), num perfil dedicado do SBF.
  - O roteiro roda no `sbf worker` ou a partir da janela, sempre na estação, nunca na VPS.
- **Login:**
  - A senha vem do chaveiro e é preenchida num passo determinístico (seletor gravado). A IA
    nunca vê a senha nem o campo preenchido.
  - MFA, CAPTCHA e confirmações sensíveis param o roteiro e passam a vez ao usuário. A janela
    mostra "Sua vez no navegador" e espera o usuário clicar em Continuar.
- **Só consulta e baixa.**
  - Uma lista de bloqueio intercepta clique e envio em elementos cujo texto, rótulo ou URL
    indique pagar, transferir, contratar, alterar cadastro, PIX, TED, investir ou resgatar.
    O roteiro aborta e registra o motivo.
  - Os testes cobrem o bloqueio.
- **Gravação:**
  - O roteiro é uma lista versionada de passos em JSON: navegar, esperar seletor, clicar,
    preencher com a senha do chaveiro, baixar, ler tabela, e pausa para o usuário.
  - Fica num banco local `roteiros.sqlite` na pasta de dados do app, fora do cofre.
- **IA:**
  - A primeira coleta é guiada pela IA pelo motor `sbf-ia` (provedor principal das Opções).
  - Para decidir o próximo passo, a IA recebe uma versão simplificada da página: texto
    visível, elementos clicáveis com seletor e formulários, **sem** valores de campos de
    senha.
  - Os passos aceitos viram o roteiro. Depois, ele roda sem IA.
  - A IA só volta quando um passo falha (o site mudou), e propõe um conserto. O usuário
    aprova o roteiro novo.
- **Destino do que é baixado:**
  - O arquivo entra no inventário como achado, com origem `roteiro:<nome>/<execução>/<arquivo>`.
  - Os roteiros gravam direto no cofre (ADR-014).
- **Janela:** a área Conexões › Roteiros lista os roteiros, a última execução, Executar,
  Gravar novo (guiado) e o histórico. Opções › Chaveiro ganha as senhas de sites.
- **Testes:**
  - Nada toca site real. Um site falso local (HTML servido em 127.0.0.1) traz login, MFA
    simulado, tabela, download e um botão "Pagar" que tem de ser bloqueado.
  - Os testes que precisam de Chrome ficam atrás de `SBF_CHROME_TESTE=1`.

## Frente C2 — RAG local e pontos de atenção

### Índice local (ADR-015, item 2)

- **Embeddings na estação:**
  - Usam `fastembed` com um modelo multilíngue que entenda português (por exemplo
    `multilingual-e5-small`).
  - O modelo é baixado uma vez, na primeira vez que o índice é usado, com aviso na janela.
  - Por trás de um trait `Embedder`, com uma implementação falsa e determinística para os
    testes. **Nenhum teste baixa modelo.**
- **Índice:**
  - Fica em `indice.sqlite`, na pasta de dados do app.
  - A busca é por força bruta, sem extensão nativa, o que basta para dezenas de milhares de
    trechos. Se a medição pedir, troca-se depois por `sqlite-vec`.
  - Os trechos vêm do texto dos achados (em pedaços de cerca de 800 caracteres com
    sobreposição), da descrição e do fornecedor dos registros e das movimentações do cache do
    livro-caixa.
  - A atualização é incremental, pelo `sbf worker`.
- **Usos:**
  1. **Área "Perguntar".** A pergunta busca os k trechos mais próximos. Só esses trechos vão
     ao destinatário (o motor de IA), no nível de confiança dele. A resposta cita os
     trechos com link para o item.
  2. **Exemplos parecidos na triagem.** Atende o "índice local da fatia 6" da spec 09 no
     agente contínuo: o prompt `triagem-v2` passa a receber exemplos por semelhança, além
     dos do mesmo fornecedor.
  3. **Ferramenta MCP `buscar_semelhantes`.**

### Pontos de atenção (CONTEXT.md)

- **Origem.** Uma análise periódica no `sbf worker`, por regras e sem IA, gera **pontos de
  atenção**. Cada um traz:
  - a evidência, com links para os itens;
  - o impacto, sempre com a estimativa marcada como estimativa;
  - o estado: aberto, resolvido ou ignorado.
- **Regras iniciais:**
  - **Dinheiro:**
    - assinatura ou recorrente que subiu de preço;
    - cobrança recorrente sem uso aparente (o mesmo fornecedor todo mês, sem documento);
    - tarifas bancárias somadas no ciclo;
    - juros e multa pagos.
  - **Operação:**
    - documentos parados em "A triar" ou "Em dúvida" há mais de 30 dias;
    - coleta que falha repetidamente;
    - disco sem cifra.
  - **Custo de IA:** consumo da semana em `saida_ia` acima do ritmo, e provedor reserva em
    uso.
  - **Tempo do usuário:** a fila de sugestões crescendo mais que as confirmações.
- **Destino.**
  - Os pontos ficam em `pontos.sqlite` na pasta de dados do app.
  - A área "Pontos de atenção" mostra o filtro por área e as ações Resolver e Ignorar, com
    motivo.
  - Um resumo opcional por IA vai no nível do destinatário.
  - O Telegram recebe os pontos novos pelo trait `FonteDeAlertas` já existente. Isso só é
    ligado quando o canal estiver vinculado.

### Implementação da frente C2 (2026-11-14)

- **`sbf-indice`**: trait `Embedder`; `EmbedderFalso` (256 dimensões, por hash de palavra, conta o que
  embutiu); `FastEmbedder` com `multilingual-e5-small` (384 dimensões, ~120 MB) atrás da feature
  `modelo-real`. Licenças conferidas com `cargo deny` (fastembed Apache-2.0, ort MIT OR Apache-2.0); a
  feature usa `ort-download-binaries-rustls-tls` e `hf-hub-rustls-tls`, sem OpenSSL. O `ort` baixa o
  ONNX Runtime **na compilação**, por isso a feature fica fora do build padrão. O app que se entrega
  compila com `cargo build -p sbf --features modelo-real`. Sem ela, Perguntar diz que a compilação não
  inclui o modelo e o worker não indexa.
- **`indice.sqlite`** (pasta de dados): tabela `trecho` com o texto, o hash dele (SHA-256), o modelo e o
  vetor como BLOB `f32` little-endian. Busca por força bruta (cosseno). Pedaços de 800 caracteres com
  120 de sobreposição, cortados em espaço. `sincronizar(fonte, trechos)` só embute o que é novo, mudou
  ou veio de outro modelo, e apaga o que sumiu (RG-02). Fontes: `achado` (texto extraído), `registro`
  (descrição, fornecedor, valor, categoria) e `movimentacao` (cache do livro-caixa, lido só para leitura
  da tabela `linha`).
- **Download do modelo**: só em `FastEmbedder::novo`, chamado pelo botão "Baixar modelo (~120 MB)" da
  área Perguntar, que mostra o aviso antes. O `sbf worker` e o MCP só carregam modelo que já está em
  `modelos/` (`modelo_baixado`) e nunca baixam.
- **Perguntar**: `perguntar(busca, provedor, registro, nivel, pergunta, k)`. Só os k=6 trechos mais
  próximos vão, numerados, com a pergunta, mascarados no nível (o padrão é **mascarado**, ADR-015:
  destinatário novo). Sem trecho, nada sai e nada é registrado. Cada chamada grava `saida_ia`
  (destinatário `perguntar`, bytes, tokens). Citação fora dos trechos enviados é descartada.
- **Triagem**: `sbf_triagem::semelhantes::completar_exemplos` junta, aos exemplos do mesmo fornecedor,
  decisões **suas** sobre itens de texto parecido (até 8 no total); `rodar_ciclo_com` recebe a busca, e
  `rodar_ciclo` segue igual. O worker só passa a busca quando o modelo e o índice existem.
- **MCP**: `buscar_semelhantes(texto, limite)`: trechos mascarados no nível do cliente (mascarado) e
  uma linha em `saida_ia` (`mcp:buscar_semelhantes`). Só liga se o índice existe.
- **`sbf-pontos`**: dez regras (`assinatura-reajuste`, `cobranca-sem-documento`, `tarifas-no-ciclo`,
  `juros-e-multa`, `documentos-parados`, `coleta-falhando`, `disco-sem-cifra`, `ia-consumo-acima-do-ritmo`,
  `ia-provedor-reserva`, `fila-de-sugestoes-crescendo`), todas em `sbf_pontos::regras`, com os limites em
  `Limites`. Impacto em centavos `i64` com `estimativa`: projeção anual é estimativa (PA-03); soma do
  ciclo é apurada. Resolver e Ignorar exigem motivo. Ponto aberto não se repete; fechado só volta se a
  assinatura da evidência mudar; ponto aberto cuja regra rodou e não o produz fecha com o motivo
  "deixou de valer" (PA-02). Regra sem dado para ler (sem cache do livro, disco desconhecido) não roda e
  não fecha nada.
- **Worker**: de 30 em 30 minutos atualiza o índice (se o modelo está lá) e de 6 em 6 horas analisa os
  pontos; eventos `indice` e `pontos` no log, sem conteúdo.
- **Telegram**: `sbf_canais::pontos::FonteDePontos` implementa `FonteDeAlertas` sobre `pontos.sqlite`.
  **Desligada por padrão**: só entra com `SBF_PONTOS_BANCO=<caminho do pontos.sqlite>` no ambiente do
  `sbf canais`. O texto passa pelo nível de confiança do destinatário.
- **Janela**: grupo Análise ganha **Perguntar** e **Pontos de atenção** (a demonstração usa o motor
  verdadeiro com índice em memória e provedor falso).
- **Fora desta entrega**: resumo opcional dos pontos por IA; nível de confiança de Perguntar configurável
  (a tela Destinatários é futura); medição do tempo de busca em dezenas de milhares de trechos.

## Matriz de aceite

| ID    | Critério                                                                                         |
| ----- | ------------------------------------------------------------------------------------------------ |
| RO-01 | A senha nunca aparece no que vai à IA, em log, em argv ou no roteiro gravado                      |
| RO-02 | Clique e envio em "Pagar", "Transferir", "PIX" etc. são bloqueados, e o roteiro aborta com motivo |
| RO-03 | MFA ou CAPTCHA pausam e passam a vez ao usuário; Continuar retoma do passo certo                  |
| RO-04 | O roteiro gravado roda sem IA no site falso; um site alterado faz a IA propor um conserto, que só vale após aprovação |
| RO-05 | O arquivo baixado vira achado no inventário com origem `roteiro:`, sem duplicar                   |
| RG-01 | Nenhum teste baixa modelo; o `Embedder` falso é determinístico                                    |
| RG-02 | O índice é incremental: reindexar sem mudanças não recalcula embeddings                           |
| RG-03 | Em "Perguntar", só os k trechos recuperados saem para o destinatário, e o registro fica em `saida_ia` |
| PA-01 | Cada regra tem teste com fixture fictícia que gera exatamente o ponto esperado                    |
| PA-02 | Um ponto não se repete enquanto aberto; resolvido ou ignorado não volta sem mudança da evidência  |
| PA-03 | A estimativa de impacto aparece marcada como estimativa                                          |

## Supervisão

1. Cada frente é feita por um agente Sonnet com esforço médio, em worktree própria, com as
   regras de `fatia3/REGRAS.md`.
2. O coordenador revisa as capturas e integra.
3. No fim, uma revisão de segurança feita pelo Opus em esforço alto, só leitura, com foco em
   RO-01 e RO-02 (senha e bloqueio de ações) e no que sai para a IA. Os achados são
   corrigidos antes de o CEO usar os roteiros em site real.
