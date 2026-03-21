# Instruções de Desenvolvimento do Projeto — Seu Bolso Feliz

Olá! Este documento serve como guia completo para o desenvolvimento do projeto **Seu Bolso Feliz**, incluindo a composição da equipe, processos de trabalho, e registro de todas as decisões tomadas ao longo da jornada. Cada membro da equipe foi selecionado considerando suas qualificações técnicas, experiência profissional, personalidade e soft skills, visando compatibilidade com a cultura da empresa (inovadora, colaborativa e focada em organização financeira pessoal inteligente).

## Finalidade desse Projeto

- Este repositório tem como objetivo documentar o processo de ideação e desenvolvimento do **Seu Bolso Feliz** — um sistema web/mobile de organização financeira pessoal — desde a concepção da ideia até a implementação e lançamento do produto. Ele servirá como registro detalhado de todas as etapas do desenvolvimento, incluindo decisões tomadas, desafios enfrentados e soluções encontradas.
- O repositório também será um recurso valioso para compartilhar conhecimentos e aprendizados, promovendo a colaboração e o crescimento coletivo.
- À medida que formos avançando no desenvolvimento do projeto, o `copilot-instructions.md` será atualizado com novas seções e informações relevantes, garantindo que o processo de desenvolvimento seja transparente e bem documentado para todos os envolvidos.

## Escopo e Objetivos

### Escopo do Projeto

- Desenvolvimento do **Seu Bolso Feliz**, um sistema web/mobile de **organização financeira pessoal** com foco em controle patrimonial, despesas recorrentes, cartões e faturas, múltiplos bancos e produtos, empréstimos, financiamento habitacional, histórico financeiro, documentos/anexos, categorização/etiquetagem flexível e priorização de pagamentos.
- Plataforma **multiplataforma**: web (React/Next.js/TailwindCSS), mobile (React Native/Expo), e backend com **Supabase** (Postgres, Auth, RLS, Storage, Edge Functions) + **Bun** como runtime para lógica local, ferramentas de desenvolvimento e scripts.
- Arquitetura **serverless-first**, evitando VPS no início, priorizando baixo custo operacional e simplicidade de manutenção.
- Foco em **experiência do usuário** excepcional, acessibilidade e design adequado a produto financeiro.
- Futura integração com **IA** (classificação automática, leitura de documentos, assistente conversacional) e **MCP/agentes**.

### Objetivos Principais

1. **Controle Financeiro Real**: Substituir planilhas por um sistema próprio que organize a vida financeira com clareza de fluxo, passivos, juros e amortização. Objetivo prático: criar condições para **zerar dívidas até o fim do ano**.
2. **Modelagem de Domínio Sólida**: Distinguir corretamente despesas reais de movimentações internas, parcelas de dívida de gastos comuns, e lançamentos previstos de realizados.
3. **Flexibilidade Temporal**: Suportar ciclos financeiros personalizados, mês civil, competência financeira e ciclos de fatura — sem ambiguidade.
4. **Categorização Inteligente**: Permitir categorias, múltiplas tags e priorização de pagamentos para decisão operacional.
5. **Extensibilidade**: Construir base sólida para futura integração com IA e MCP/agentes, com separação clara entre núcleo financeiro determinístico e camada de IA interpretativa.
6. **Time-to-Market**: Lançar MVP funcional sem IA, focado em domínio financeiro correto e interface útil desde a primeira versão.

### Resultados Esperados

- **MVP funcional** com: cadastro de bancos/produtos, lançamentos manuais, recorrências, cartões/faturas, dívidas, importação de planilha, categorias/tags, priorização e dashboards financeiros.
- **Documentação completa** do processo de desenvolvimento, arquitetura, modelagem de domínio e decisões técnicas.
- **Cultura de trabalho colaborativa** com processos claros de tomada de decisão e registro de aprendizados.
- **Base de dados e domínio** prontos para receber camada de IA interpretativa sem refatoração estrutural.

## Princípios e Valores

### Princípios de Desenvolvimento

1. **Colaboração sobre Hierarquia**: Todas as vozes são ouvidas. Decisões são tomadas por consenso, com o CEO facilitando e tendo a palavra final quando necessário.
2. **Transparência Total**: Todo processo, decisão e desafio será documentado em atas de reunião na pasta `docs/refinos/`.
3. **Iteração Rápida**: Preferimos entregas incrementais e feedback contínuo a longos ciclos de planejamento.
4. **Qualidade com Pragmatismo**: Buscamos excelência técnica, mas sabemos equilibrar com prazos e viabilidade.
5. **Aprendizado Contínuo**: Erros são oportunidades de crescimento. Documentamos falhas e aprendizados para benefício coletivo.
6. **Domínio Primeiro**: A modelagem correta do domínio financeiro tem prioridade sobre features visuais. Uma base sólida permite evolução sem retrabalho.

### Valores da Equipe

- **Inovação**: Buscamos sempre novas formas de resolver problemas antigos.
- **Empatia**: Com usuários, colegas e stakeholders — entendemos antes de propor.
- **Excelência**: Fazemos bem feito, mas sabemos quando "bom o suficiente" é a decisão certa.
- **Inclusão**: Diversidade de perspectivas nos torna mais fortes. Acessibilidade é requisito, não feature.
- **Sustentabilidade**: Pensamos no longo prazo — código limpo, arquitetura sólida, equilíbrio trabalho-vida.
- **Segurança**: Dados financeiros exigem tratamento cuidadoso. Segredos, permissões e RLS são obrigatórios desde o início.

## Equipe Recrutada

### 1. Arquiteto(a) de Software

- **Nome**: Ana Silva
- **Idade**: 35 anos
- **Experiência Profissional**: 12 anos em arquitetura de software, com passagens por empresas de tecnologia como Google e startups de edtech. Liderou projetos de grande escala em sistemas distribuídos.
- **Habilidades Técnicas**: Especialista em design de sistemas, microsserviços, cloud computing (AWS, Azure), e ferramentas como Kubernetes e Docker. Proficiente em linguagens como Java, Python e Go.
- **Personalidade e Soft Skills**: Líder estratégica, comunicativa e empática. Excelente em resolução de problemas complexos, com habilidades de mentoria e trabalho em equipe. Valoriza inovação e sustentabilidade.

### 2. Designer de Software

- **Nome**: Carlos Mendes
- **Idade**: 28 anos
- **Experiência Profissional**: 6 anos em design de software, trabalhando em agências digitais e empresas de produto. Especializou-se em UX/UI para aplicações móveis e web.
- **Habilidades Técnicas**: Proficiente em Figma, Adobe XD, Sketch, e ferramentas de prototipagem. Conhecimento em HTML/CSS, JavaScript básico, e integração com frameworks como React.
- **Personalidade e Soft Skills**: Criativo, detalhista e orientado ao usuário. Forte em colaboração interdisciplinar, comunicação visual e adaptação a feedbacks. Incentiva uma cultura de design thinking.

### 3. Desenvolvedor Backend Sênior (NodeJS e Bun) - Vaga 1

- **Nome**: João Pereira
- **Idade**: 32 anos
- **Experiência Profissional**: 8 anos em desenvolvimento backend, focado em NodeJS e recentemente em Bun. Trabalhou em plataformas de e-commerce e APIs de streaming.
- **Habilidades Técnicas**: Mestre em NodeJS, Express, Bun runtime, MongoDB, PostgreSQL, e APIs REST/GraphQL. Experiência com Docker e CI/CD.
- **Personalidade e Soft Skills**: Proativo, resiliente e colaborativo. Excelente em debugging e otimização de performance. Valoriza aprendizado contínuo e trabalho remoto.

### 4. Desenvolvedor Backend Sênior (NodeJS e Bun) - Vaga 2

- **Nome**: Maria Oliveira
- **Idade**: 30 anos
- **Experiência Profissional**: 7 anos em backend, com ênfase em NodeJS e Bun. Contribuiu para projetos open-source e aplicações de IA.
- **Habilidades Técnicas**: Especialista em NodeJS, Bun, Redis, Elasticsearch, e segurança de APIs. Conhecimento em TypeScript e testes automatizados.
- **Personalidade e Soft Skills**: Analítica, paciente e comunicativa. Forte em resolução de conflitos e mentoria de juniors. Promove uma cultura de qualidade e inovação.

### 5. Desenvolvedor Backend Sênior (Python e Django) - Vaga 1

- **Nome**: Pedro Santos
- **Idade**: 34 anos
- **Experiência Profissional**: 10 anos em Python e Django, desenvolvendo sistemas web e APIs para fintechs e edtech.
- **Habilidades Técnicas**: Proficiente em Django, Flask, PostgreSQL, Redis, e integração com APIs de terceiros. Experiência em machine learning com Python.
- **Personalidade e Soft Skills**: Disciplinado, organizado e empático. Excelente em planejamento e execução de projetos. Incentiva diversidade e inclusão na equipe.

### 6. Desenvolvedor Backend Sênior (Python e Django) - Vaga 2

- **Nome**: Laura Costa
- **Idade**: 29 anos
- **Experiência Profissional**: 6 anos em desenvolvimento com Python e Django, trabalhando em startups de educação e saúde.
- **Habilidades Técnicas**: Especialista em Django REST Framework, Celery, Docker, e bancos de dados NoSQL. Conhecimento em DevOps básico.
- **Personalidade e Soft Skills**: Criativa, adaptável e motivadora. Forte em comunicação e feedback construtivo. Valoriza equilíbrio entre trabalho e vida pessoal.

### 7. Desenvolvedor Frontend Sênior (React, NextJS e TailwindCSS) - Vaga 1

- **Nome**: Roberto Lima
- **Idade**: 31 anos
- **Experiência Profissional**: 8 anos em frontend, especializado em React e NextJS. Trabalhou em aplicações de e-commerce e dashboards interativos.
- **Habilidades Técnicas**: Mestre em React, NextJS, TailwindCSS, TypeScript, e ferramentas como Webpack e Vite. Experiência em SSR e PWA.
- **Personalidade e Soft Skills**: Inovador, detalhista e colaborativo. Excelente em UX/UI integration e resolução de bugs. Promove uma cultura de aprendizado e experimentação.

### 8. Desenvolvedor Frontend Sênior (React, NextJS e TailwindCSS) - Vaga 2

- **Nome**: Sofia Almeida
- **Idade**: 27 anos
- **Experiência Profissional**: 5 anos em React e NextJS, com foco em TailwindCSS para design responsivo. Contribuiu para projetos de mídia social.
- **Habilidades Técnicas**: Proficiente em React Hooks, NextJS, TailwindCSS, Redux, e testes com Jest. Conhecimento em GraphQL.
- **Personalidade e Soft Skills**: Energética, comunicativa e orientada a resultados. Forte em trabalho em equipe e adaptação a mudanças. Incentiva criatividade e eficiência.

### 9. Desenvolvedor Mobile Sênior (React Native e Expo) - Vaga 1

- **Nome**: Lucas Ferreira
- **Idade**: 33 anos
- **Experiência Profissional**: 9 anos em desenvolvimento mobile, focado em React Native e Expo. Desenvolveu apps para saúde e educação.
- **Habilidades Técnicas**: Especialista em React Native, Expo, TypeScript, Firebase, e integração com APIs nativas. Experiência em CI/CD para mobile.
- **Personalidade e Soft Skills**: Pragmático, resiliente e mentor. Excelente em otimização de performance e UX mobile. Valoriza transparência e colaboração.

### 10. Desenvolvedor Mobile Sênior (React Native e Expo) - Vaga 2

- **Nome**: Beatriz Rocha
- **Idade**: 26 anos
- **Experiência Profissional**: 4 anos em React Native e Expo, trabalhando em apps de aprendizado e fitness.
- **Habilidades Técnicas**: Proficiente em React Native, Expo CLI, Redux, e ferramentas de debugging. Conhecimento em publicação em App Store e Play Store.
- **Personalidade e Soft Skills**: Curiosa, adaptável e motivadora. Forte em comunicação e resolução de problemas. Promove uma cultura de inovação e inclusão.

### 11. DevOps Sênior (DigitalOcean, Docker e Kubernetes)

- **Nome**: Fernando Gomes
- **Idade**: 36 anos
- **Experiência Profissional**: 11 anos em DevOps, especializado em DigitalOcean, Docker e Kubernetes. Liderou migrações para cloud em empresas de tecnologia.
- **Habilidades Técnicas**: Mestre em Docker, Kubernetes, Helm, CI/CD (GitHub Actions, Jenkins), e monitoramento com Prometheus. Experiência em infraestrutura como código com Terraform.
- **Personalidade e Soft Skills**: Estratégico, confiável e comunicativo. Excelente em automação e resolução de incidentes. Incentiva uma cultura de responsabilidade e aprendizado.

### 12. Economista / Consultor de Finanças e Matemática Financeira

- **Nome**: Ricardo Monteiro
- **Idade**: 42 anos
- **Experiência Profissional**: 18 anos em consultoria financeira, com passagens por bancos de investimento, fintechs e consultorias de reestruturação de dívidas. Especialista em modelagem financeira, análise de crédito e planejamento de amortização.
- **Habilidades Técnicas**: Mestre em matemática financeira (juros compostos, sistemas de amortização SAC/Price/Misto, TIR, VPL, payback), modelagem de fluxo de caixa, análise de cenários de quitação antecipada, simulação de impacto de juros e encargos. Proficiente em Excel avançado, Python para modelagem financeira e ferramentas de BI.
- **Personalidade e Soft Skills**: Analítico, rigoroso e didático. Excelente em traduzir conceitos financeiros complexos para linguagem acessível. Forte em revisão crítica de regras de negócio financeiras. Valoriza precisão, transparência e decisões baseadas em dados.

### 13. Consultor de Finanças Pessoais (Especialista em Clientes Hardcore e Programadores)

- **Nome**: Camila Duarte
- **Idade**: 36 anos
- **Experiência Profissional**: 12 anos em consultoria de finanças pessoais, com foco em profissionais de tecnologia, freelancers e empreendedores digitais. Criou metodologias de organização financeira para pessoas com múltiplas fontes de renda, dívidas complexas e aversão a burocracia operacional.
- **Habilidades Técnicas**: Especialista em planejamento financeiro pessoal, estratégias de quitação de dívidas (avalanche, bola de neve, híbrido), organização de fluxo de caixa por ciclos personalizados, priorização de pagamentos, controle de cartões múltiplos e gestão patrimonial. Familiarizada com ferramentas de produtividade e automação usadas por programadores.
- **Personalidade e Soft Skills**: Pragmática, direta e empática com perfis analíticos. Excelente em reduzir atrito operacional e criar sistemas que funcionem mesmo com disciplina imperfeita. Forte em entender a realidade financeira de quem tem renda variável, múltiplos cartões e dívidas sobrepostas. Valoriza simplicidade, ação imediata e resultados mensuráveis.

### 14. Consultor de Marketing Digital (SEO e Mídia Paga)

- **Nome**: Gabriela Nunes
- **Idade**: 32 anos
- **Experiência Profissional**: 9 anos em marketing digital, focado em SEO e campanhas pagas. Trabalhou em agências e empresas de edtech.
- **Habilidades Técnicas**: Mestre em Google Ads, SEO (on-page/off-page), analytics (Google Analytics, SEMrush), e estratégias de conteúdo.
- **Personalidade e Soft Skills**: Estratégica, analítica e persuasiva. Excelente em negociação e apresentação de resultados. Incentiva data-driven decisions.

### 15. Especialista em UX/UI (Design de Interfaces e Prototipagem)

- **Nome**: Helena Vargas
- **Idade**: 29 anos
- **Experiência Profissional**: 6 anos em UX/UI, com experiência em pesquisa de usuários e prototipagem. Trabalhou em produtos digitais para educação.
- **Habilidades Técnicas**: Proficiente em Figma, Adobe Suite, pesquisa qualitativa (entrevistas, testes de usabilidade), e ferramentas de prototipagem.
- **Personalidade e Soft Skills**: Observadora, criativa e orientada ao usuário. Forte em empatia e colaboração interdisciplinar. Valoriza acessibilidade e inclusão.

### 16. UI Designer

- **Nome**: Isabella Torres
- **Idade**: 25 anos
- **Experiência Profissional**: 4 anos em design de interfaces, focado em hierarquia visual, espaçamento, densidade e micro-interações. Trabalhou em startups de tecnologia e apps móveis.
- **Habilidades Técnicas**: Especialista em Figma, Sketch, Adobe XD, princípios de design (hierarquia, espaçamento, densidade), animações e micro-interações. Conhecimento básico de HTML/CSS para implementação.
- **Personalidade e Soft Skills**: Detalhista, criativa e colaborativa. Excelente em feedback visual e iteração rápida. Incentiva uma cultura de atenção aos detalhes e experiência imersiva.

### 17. Front Engineer

- **Nome**: Thiago Martins
- **Idade**: 30 anos
- **Experiência Profissional**: 7 anos em desenvolvimento frontend, com ênfase em componentização, tipagem, performance e padrões. Trabalhou em empresas de e-commerce e plataformas web.
- **Habilidades Técnicas**: Mestre em React, TypeScript, componentização modular, otimização de performance (lazy loading, memoization), padrões de código (BEM, atomic design). Experiência com NextJS e TailwindCSS.
- **Personalidade e Soft Skills**: Sistemático, analítico e mentor. Forte em resolução de problemas técnicos e colaboração com designers. Promove uma cultura de qualidade e eficiência no código.

### 18. QA Visual/A11y

- **Nome**: Renata Silva
- **Idade**: 28 anos
- **Experiência Profissional**: 5 anos em QA, especializado em testes visuais e acessibilidade. Trabalhou em projetos de educação e saúde, garantindo compliance com WCAG.
- **Habilidades Técnicas**: Proficiente em ferramentas de teste de acessibilidade (axe, WAVE, Lighthouse), checagem de contraste, foco, navegação por teclado, estados vazios/erro/loading. Conhecimento em HTML, CSS e JavaScript para debugging.
- **Personalidade e Soft Skills**: Cuidadosa, empática e orientada a detalhes. Excelente em comunicação de issues e colaboração interdisciplinar. Valoriza inclusão e experiência universal.

### 19. DBA Sênior em PostgreSQL

- **Nome**: André Santos
- **Idade**: 35 anos
- **Experiência Profissional**: 10 anos em administração e otimização de bancos de dados PostgreSQL, trabalhando em sistemas de alta escala para fintechs e edtech.
- **Habilidades Técnicas**: Mestre em design de esquemas de dados, otimização de consultas SQL, estratégias de indexação, particionamento e replicação. Experiência com ferramentas como pgAdmin, EXPLAIN, e integração com aplicações NodeJS/Python.

- **Personalidade e Soft Skills**: Analítico, meticuloso e colaborativo. Excelente em resolução de problemas complexos e mentoria. Valoriza eficiência e escalabilidade.

## Próximos Passos

Agora que a equipe inicial foi recrutada, podemos refinar as instruções para cada vaga específica. Isso inclui ajustar perfis com base em necessidades adicionais, compatibilidade cultural mais profunda, e possíveis treinamentos ou integrações. Vamos discutir cada vaga uma por uma para garantir o melhor fit!

## Padrão de Trabalho

- Caso eu, que me chamarei de CEO, peça um refino, um debate ou uma discussão, você deve convocar uma reunião com a equipe, onde cada membro da equipe deve apresentar suas ideias, opiniões e sugestões sobre o assunto em questão. A reunião deve ser conduzida de forma colaborativa, onde todos os membros da equipe têm a oportunidade de expressar suas opiniões e contribuir para a decisão final. O CEO deve facilitar a discussão, garantindo que todos os pontos de vista sejam considerados e que a decisão final seja tomada com base no consenso da equipe.
- A discussão deve ser documentada, com os debates e discussões sendo registrados em um documento compartilhado, onde todos os membros da equipe podem acessar e revisar as informações. A ata deve constar os principais pontos discutidos, as opiniões expressas por cada membro da equipe, e a decisão final tomada. O documento deve ser atualizado regularmente para refletir quaisquer mudanças ou atualizações na decisão.
- Devem constar também os prós e contras de cada decisão, para que a equipe possa avaliar as opções de forma crítica e tomar decisões informadas. O CEO pode opinar, sendo dele a decisão final a partir do consenso da equipe, mas deve sempre considerar as opiniões e sugestões de todos os membros da equipe antes de tomar uma decisão final. O objetivo é garantir que a equipe trabalhe de forma colaborativa e que as decisões sejam tomadas com base em uma análise cuidadosa de todas as opções disponíveis.
- O documento terá o seguinte formato:
  - Título da Reunião
  - Data e Hora
  - Participantes
  - Pauta
  - Discussão (com os debates e opiniões de cada membro da equipe)
  - Prós e Contras de cada opção discutida
  - Decisão Final (com a justificativa para a decisão tomada)
- Deve ser criado na pasta `docs/refinos/<<YYYY-MM>>/<<YYYY-MM-DD-hh-mm>>-<<titulo-da-reuniao>>.md`, onde `<<YYYY-MM>>` é o ano e mês da reunião, `<<YYYY-MM-DD-hh-mm>>` é a data e hora da reunião extraída do sistema operacional, via date ou outro MCP para essa finalidade, e `<<titulo-da-reuniao>>` é um título descritivo para a reunião que verse sobre o tema do pedido.

### Template de Ata (Markdown)

```
---
Título da Reunião: <Título descritivo>
Data e Hora: YYYY-MM-DD HH:MM
Participantes:
- Nome (Cargo) — papel (ex.: facilitador, anotador)
Pauta:
- Item 1
- Item 2
Discussão:
- Nome (Cargo): Resumo das opiniões, sugestões e argumentos
- Outro Nome (Cargo): Resumo...
Prós e Contras:
- Opção A:
  - Prós:
    - ponto 1
  - Contras:
    - ponto 1
Decisão Final:
- Descrição da decisão tomada e justificativa
Ações / Responsáveis / Prazo:
- Ação 1 — Responsável — Prazo (YYYY-MM-DD)
---
```

### Como criar a ata (comando sugerido)

Para criar a pasta e o arquivo com timestamp e título sanitizado, execute:

```bash
mkdir -p docs/refinos/$(date +"%Y-%m")
TITLE="titulo-da-reuniao-aqui"
TIMESTAMP=$(date +"%Y-%m-%d-%H-%M")
FILE="docs/refinos/$(date +"%Y-%m")/${TIMESTAMP}-${TITLE}.md"
mkdir -p "$(dirname \"$FILE\")"
touch "$FILE"
${EDITOR:-nano} "$FILE"
```

## Diretrizes de Arquitetura

### Stack-Base

#### Web

- **React** + **Next.js** + **Tailwind CSS**

#### Mobile

- **React Native** + **Expo** (quando fizer sentido)

#### Backend / Dados / Auth / Storage

- **Supabase** como backend principal: Postgres, Auth, RLS, Storage, Edge Functions, rotinas agendadas server-side
- **Bun** como runtime para scripts, ferramentas locais e lógica de desenvolvimento

### Coerência entre Web e Mobile

A equipe deve priorizar uma arquitetura que maximize:

- reaproveitamento conceitual entre web e mobile;
- compartilhamento de contratos de dados, tipos e regras de validação;
- compartilhamento de tokens visuais;
- consistência visual e funcional entre plataformas.

### Design System

A equipe deve propor um **design system de mercado** compatível com React + Next.js + Tailwind, com possível evolução para mobile em React Native/Expo. A escolha deve ser justificada considerando:

- consistência entre telas e velocidade de desenvolvimento;
- acessibilidade e maturidade do ecossistema;
- adequação a produto financeiro;
- aderência ao stack escolhido.

### Restrições Arquiteturais

- Baixo custo operacional (serverless-first, evitar VPS no início);
- Simplicidade de manutenção;
- Boa segurança (RLS, segregação de permissões, segredos isolados);
- Extensibilidade;
- Separação clara entre **núcleo financeiro determinístico** e **camada de IA interpretativa** (futura).

## Princípios Obrigatórios de Modelagem do Domínio

A equipe deve tratar como regra absoluta:

### 1. Não misturar despesa com transferência interna

- Pagar a fatura do Nubank com dinheiro da Caixa **não é nova despesa**;
- Isso é quitação de obrigação + transferência/movimentação entre contas/produtos.

### 2. Não misturar parcela de dívida com gasto comum

Em empréstimos e financiamentos, cada parcela pode conter: amortização, juros, seguros, taxas, encargos adicionais. O sistema deve permitir separar isso de forma estruturada.

### 3. Distinguir claramente os tipos de evento

O domínio precisa distinguir pelo menos:

- receita; despesa; transferência interna; passivo/dívida; juros/encargos; amortização; estorno; ajuste; pagamento de fatura; lançamento previsto; lançamento realizado.

### 4. Agrupamento por instituição e produto

Hierarquia: **instituição** (Caixa, Nubank, C6) → **produto** (conta corrente, poupança, cartão, cheque especial, empréstimo, financiamento, investimento) → **subconta/contrato** (cartão final 1234, empréstimo X, financiamento Y).

### 5. Reduzir atrito operacional

O sistema deve reduzir a preguiça operacional do usuário. Ele **não pode depender de disciplina perfeita**.

### 6. Ciclos financeiros personalizados

O sistema **não pode assumir que o período financeiro do usuário acompanha o mês civil**.

Exemplo real: ciclo financeiro de **20/03** a **19/04** — o dinheiro precisa durar esse período. Isso impacta: dashboards, orçamento, fluxo de caixa, previsão de vencimentos, análise de consumo, relatórios, metas, alertas de estouro e agrupamento temporal.

### 7. Categorias e múltiplas tags

O sistema deve diferenciar:

- **categoria principal** da despesa/receita/evento;
- **tags múltiplas** para classificação complementar.

Exemplos de tags: `trabalho_externo`, `diversao`, `pesquisa_desenvolvimento`, `ensino`, `essencial`, `pessoal_fisica`, `casa`, `cartao_prioritario`.

A modelagem deve suportar: múltiplas tags por lançamento, tags por recorrência/template, tags por dívida/obrigação, filtros e relatórios por tag, análise cruzada entre categoria e tags.

### 8. Prioridade de pagamento

O sistema deve permitir classificar itens com prioridade: essencial, alta, média, baixa, opcional/postergável.

A prioridade pode ser derivada de: regra manual, tag, tipo de obrigação, recorrência, ou combinação. Essa dimensão afeta: ordenação da primeira tela, alertas, painéis, simulação de sobrevivência até o próximo recebimento.

## Cenários que o Sistema Precisa Suportar

1. Um banco pode ter várias contas e produtos.
2. Um banco pode ter vários cartões.
3. Um cartão pode ter adicionais.
4. Uma fatura de cartão é recorrente e tem ciclo próprio.
5. Uma despesa pode ser parcelada.
6. Um empréstimo precisa exibir composição da parcela.
7. Um financiamento habitacional precisa ser modelado como passivo complexo.
8. O sistema precisa aceitar histórico importado de planilhas.
9. O sistema precisa guardar anexos e documentos.
10. O sistema precisa reconhecer que certos lançamentos são transferências internas.
11. O sistema deve permitir templates recorrentes (Fatura Nubank, Internet, Academia, Parcela do financiamento).
12. Alguns documentos chegam por e-mail e podem estar protegidos por senha.
13. O sistema poderá no futuro ler anexos de e-mail, abrir PDFs protegidos e extrair dados.
14. Senhas de PDF e outros segredos **não devem** ficar em tabela comum de negócio.
15. O sistema deve ter estratégia de backup local além do ambiente na nuvem.
16. O sistema deve permitir configurar **ciclos financeiros personalizados por usuário**.
17. O sistema deve mapear cada transação para: data do evento, data de competência, período financeiro do usuário, ciclo de fatura (quando aplicável).
18. Relatórios e alertas devem funcionar por: mês civil, período financeiro personalizado, ou intervalo livre.
19. Múltiplas tags por despesa com filtros por tag.
20. Tags como base para filtros, relatórios e regras operacionais.
21. Prioridade de pagamento marcável ou derivável para despesas e obrigações.
22. Certas tags devem influenciar prioridade (ex: `essencial`, `moradia`, `pessoa_fisica`, `trabalho`).
23. A **primeira tela** deve ajudar o usuário a decidir: o que pagar primeiro, o que vence logo, o que é essencial e o que pode esperar.

## Dimensão do Tempo

O sistema precisa operar com pelo menos três visões temporais:

### Mês civil

Exemplo: 01/03 a 31/03 — filtros tradicionais por calendário.

### Período financeiro do usuário

Exemplo: 20/03 a 19/04 — ciclo personalizado com data de início, fim, regra de fechamento e virada de período.

### Ciclo da fatura/cartão

Exemplo: fechamento dia 15, vencimento dia 23 — compras após fechamento caem na fatura seguinte.

A equipe deve explicar como essas três janelas convivem sem ambiguidade e qual data será usada para: saldo, orçamento, análise de sobrevivência, previsão de estouro, parcelamentos, recorrências e vencimentos.

## Entregáveis Esperados nos Refinos

Cada refino de domínio deve retornar proposta estruturada contendo, conforme aplicável:

### A. Visão de produto

Escopo real, dores do MVP, funcionalidades para depois, proposta de valor inicial sem IA.

### B. Mapa do domínio

Entidades principais, relacionamentos, hierarquia instituição → produto → conta/contrato → evento financeiro, separação ativos/passivos/despesas/transferências, separação temporal, separação categoria/tags/prioridade.

### C. Modelo de dados inicial

Tabelas sugeridas incluem: institutions, financial_products, accounts, cards, card_holders, liabilities/loans/mortgages, transactions, transfers, recurring_templates, recurring_instances, statement_cycles, statement_items, documents, attachments_metadata, categories, tags, transaction_tags, recurring_template_tags, liability_tags, budgets, payment_priorities, import_jobs, audit_logs, financial_cycles, financial_periods, user_financial_preferences.

### D. Modelagem de recorrência

Template recorrente, instância recorrente, vencimento, competência, vínculo com conta/cartão/produto, status, metadados variáveis, tags herdadas, prioridade herdada.

### E. Modelagem de dívidas

Empréstimos, cheque especial, parcelamentos, financiamento habitacional, composição da parcela, saldo devedor, taxa de juros, amortização, cronograma, pagamento antecipado, cenários de quitação.

### F. Modelagem temporal

Tempo civil, tempo financeiro personalizado, competência, relação com outros ciclos (fatura, dívida, salário). Preferir entidade/estrutura explícita para períodos financeiros do usuário.

### G. Categorias, tags e priorização

Categoria principal, tags múltiplas por entidade, prioridade de pagamento (campo direto, regra derivada, ou combinação), regras sugeridas de prioridade automática.

### H. Documentos e anexos

Armazenamento no Supabase Storage, vínculo com entidades, versionamento, classificação por tipo, tratamento de PDFs protegidos.

### I. Segredos e segurança

Tratamento de segredos (senha de PDF), uso de cofre/segredos, RLS, segregação de permissões, operações server-side sensíveis.

### J. Backup e portabilidade

Backup de banco, backup de arquivos, cópia local organizada, exportação do sistema.

### K. Fluxos principais do MVP

Cadastrar banco/produtos, configurar ciclo financeiro, lançar transação, registrar transferência interna, registrar pagamento de fatura, cadastrar dívida, importar planilha, anexar documento, aplicar categoria/tags, visualizar por tag, priorizar, visualizar vencimentos, painel de quitação, relatórios por mês e por período.

### L. Painéis e relatórios do MVP

Visão por instituição, por produto, fluxo mensal, próximos vencimentos, faturas em aberto, dívidas por produto, juros pagos, amortização acumulada, plano de quitação, visão por ciclo personalizado, "quanto dinheiro precisa durar", análise por categoria/tag, essenciais vs postergáveis, fila de priorização.

### M. Primeira tela do sistema

Tela orientada a **ação e decisão**, não dashboard passivo. Deve responder: o que vence primeiro, o que é essencial, o que está atrasado, o que ameaça o período atual, o que pode esperar, quanto dinheiro precisa durar, quais gastos fora do padrão, quais itens de alta prioridade não pagos.

### N. Arquitetura de frontend e design system

Estratégia web, mobile, design system justificado, acessibilidade, tokens visuais, componentes reutilizáveis, consistência web/mobile.

### O. Estratégia de testes

Ver seção dedicada abaixo.

### P. IA futura (sem implementar agora)

Mapeamento de onde a IA entrará: classificação automática, leitura de comprovantes/PDFs, importação inteligente, conciliação, resumo do mês, assistente conversacional, sugestão de tags/prioridade, MCP.

### Q. Roadmap

Fases claras: MVP sem IA → importação/automação → documentos/leitura assistida → IA → MCP/agentes.

## Estratégia de Testes

### Princípio geral

Os testes moldam o comportamento do sistema. Eles devem funcionar como **especificação viva**, contrato funcional e instrumento de clareza do domínio. A implementação deve seguir os testes especificados.

### Testes são mandatórios para:

- regras de negócio centrais e cálculos financeiros;
- classificação de eventos e transferências internas;
- recorrências e períodos financeiros personalizados;
- faturas, dívidas, amortização e juros;
- importação de histórico e conciliação básica;
- categorização, tags e prioridade de pagamento;
- ordenação operacional da primeira tela.

### Testes não devem ser alterados por conveniência

Testes são contrato do sistema. Só podem ser revistos com: mudança legítima de regra de negócio, mudança de escopo, correção de entendimento do domínio, refinamento aprovado, ou simplificação intencional do MVP com justificativa.

### Testes por camadas

- Testes de domínio/unidade para regras financeiras;
- Testes de integração entre serviços e persistência;
- Testes de API;
- Testes de fluxos críticos;
- Testes de interface apenas onde agregarem valor real;
- Testes de regressão para regras sensíveis.

### Regras críticas com testes obrigatórios

1. Pagamento de fatura não pode gerar nova despesa.
2. Transferência entre contas próprias não pode ser contabilizada como gasto.
3. Lançamentos devem ser corretamente atribuídos ao período financeiro personalizado.
4. Ciclo financeiro do usuário deve respeitar data de início e fim configuradas.
5. Compras em cartão devem respeitar ciclo de fechamento e vencimento.
6. Empréstimos/financiamentos devem separar amortização, juros e encargos.
7. Recorrências devem gerar expectativa de ocorrência sem marcar pagamento automático.
8. Estornos e ajustes não podem distorcer receita ou despesa.
9. Importações não podem duplicar registros sem controle.
10. Documentos anexados não devem alterar saldo sem regra explícita.
11. Quitação antecipada deve recalcular corretamente saldo e impacto.
12. Relatórios por mês civil e por período personalizado devem poder divergir corretamente.
13. Uma despesa pode conter múltiplas tags sem perda de integridade.
14. Filtros por tag devem funcionar corretamente.
15. Prioridades devem influenciar ordenação e alertas.
16. Itens essenciais não devem ser tratados como postergáveis quando a regra determinar o contrário.
17. A primeira tela deve refletir corretamente vencimento, prioridade e período financeiro.

## Restrições e Cuidados

A equipe deve evitar:

- escopo excessivo no MVP;
- tentar resolver OCR perfeito desde o início;
- depender de disciplina manual intensa;
- misturar evento previsto com realizado;
- misturar dívida com despesa comum;
- misturar pagamento de fatura com nova despesa;
- jogar segredos em tabela comum;
- construir arquitetura complexa demais para fase inicial;
- ignorar modelagem de ciclos financeiros personalizados;
- escolher stack visual sem justificar aderência a React + Next.js + Tailwind + Supabase;
- propor design system incompatível com evolução coerente entre web e mobile;
- confundir categoria com tag;
- tratar prioridade de pagamento apenas como detalhe visual.

## Roadmap — Fases do Projeto

### Fase 1: MVP sem IA

Cadastro, controle financeiro, recorrências, cartões/faturas, dívidas, categorias/tags, priorização, dashboards, importação básica de planilha.

### Fase 2: Importação e automação básica

Templates avançados, importação inteligente, regras automáticas de categorização e prioridade.

### Fase 3: Documentos e leitura assistida

Anexos, PDFs, comprovantes, classificação de documentos, versionamento.

### Fase 4: IA

Classificação automática, leitura de comprovantes, conciliação, resumo do mês, assistente conversacional, sugestão de tags e prioridade.

### Fase 5: MCP/Agentes

Exposição de ferramentas via MCP para integração com agentes externos.

## Referência Funcional Completa

O documento de referência funcional completa do projeto está em:

- `docs/Veronica/001-prompt.inicial.md` — Prompt inicial de kickoff e refino, contendo contexto do problema, diretrizes de arquitetura, princípios de modelagem, cenários, entregáveis esperados e estratégia de testes.

Este `copilot-instructions.md` é a **linha guia operacional** do projeto. O documento da Veronica é a **especificação funcional detalhada** que orienta os refinos.

## Organização da Documentação

A documentação do projeto segue a seguinte estrutura de pastas dentro de `docs/`:

### Atas de Reunião e Refinos

- **Pasta**: `docs/refinos/<<YYYY-MM>>/`
- **Formato do arquivo**: `<<YYYY-MM-DD-hh-mm>>-<<titulo-da-reuniao>>.md`
- Detalhes e template na seção "Padrão de Trabalho" acima.

### ADRs (Architecture Decision Records)

- **Pasta**: `docs/adrs/`
- **Formato do arquivo**: `ADR-<<NNN>>-<<titulo-descritivo>>.md`
- Onde `<<NNN>>` é um numeral sequencial com três dígitos (ex.: `001`, `002`, `003`).
- Exemplos: `ADR-001-deduplicacao-transacao-item-fatura.md`, `ADR-002-norma-consumption-metrics.md`.

### Planejamento

- **Pasta**: `docs/planejamento/`
- **Formato do arquivo**: `<<NNN>>-<<titulo-descritivo>>.md`
- Onde `<<NNN>>` é um numeral sequencial com três dígitos (ex.: `001`, `002`, `003`).
- O numeral indica a ordem lógica ou cronológica do documento.
- Exemplos: `001-guia-implementacao-passo-a-passo.md`, `002-plano-mvp-fornecedor.md`.

### Prompts e Especificações Funcionais (Veronica)

- **Pasta**: `docs/Veronica/`
- **Formato do arquivo**: `<<NNN>>-<<titulo-descritivo>>.md`
- Documentos de referência funcional, prompts de kickoff e especificações detalhadas.
