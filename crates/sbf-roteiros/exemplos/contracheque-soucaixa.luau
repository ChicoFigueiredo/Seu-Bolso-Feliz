-- Contracheque no SouCaixa (https://soucaixa.caixa.gov.br). Observado no portal real em
-- 2026-11-25, de forma assistida; os seletores abaixo são os dessa observação. O manifesto
-- (contracheque-soucaixa.json) diz os hosts, o rótulo da senha ("soucaixa", em Chaveiro >
-- Senhas de sites) e de quem vem o código por e-mail. O roteiro nunca vê senha, CPF nem código:
-- só pede ao motor que os digite no campo.
--
-- O que ele faz:
--  1. Entra só se preciso (a sessão do perfil dedicado pode ainda valer): CPF, escolha do
--     e-mail para receber o código, código (do Gmail, pelo motor) e senha.
--  2. Para cada mês de `parametros.de` a `parametros.ate` ("AAAA-MM", já resolvidos pelo app;
--     sem parâmetro, só o mês corrente), abre o contracheque, escolhe o mês no calendário e
--     baixa o PDF. A rota /contraCheque/MM/AAAA NÃO troca o mês: só o calendário troca.
--  3. Mês sem contracheque publicado (ou fora do período do portal) não é erro: registra e
--     segue. O dia 30 substitui o contracheque do dia 20 no mesmo lugar do portal; quem decide
--     se o arquivo já estava no inventário é o inventário (pelo conteúdo), não este roteiro.
--
-- Só clica no necessário: o portal tem Pedidos, Adiantamentos, Auxílios (alterar percentuais!),
-- Perfil, Sair, "Quero alterar meus dados", "Não recebi o código", "Voltar" e "Recuperar
-- Senha". Nada disso é tocado aqui, e a lista de bloqueio barra o que for destrutivo.

local URL_INICIAL = "https://soucaixa.caixa.gov.br/"
local ROTULO_DA_SENHA = "soucaixa"

-- Tela de entrada (login.caixa.gov.br, Keycloak).
local CAMPO_CPF = "#username"
local BOTAO_PROXIMO = "#button-submit"
-- O `id` do radio carrega o e-mail MASCARADO: por isso o seletor é pelo `name`.
local RADIO_EMAIL = 'input[name="mail"]'
local BOTAO_LOGIN = 'button[name="login"]'
local CAMPO_CODIGO = "#codigo"
local CAMPO_SENHA = "#password"
local BOTAO_ENTRAR = "#template-section > form:nth-of-type(1) > div:nth-of-type(1)"
  .. " > button:nth-of-type(1)"
local HOME = "app-home"

-- Contracheque.
local ALTERNADOR = "#matFormFieldDataTemplate mat-datepicker-toggle button"
local ANO_DO_CABECALHO = "mat-calendar-header button"
local ANO_ANTERIOR = 'mat-calendar-header button[aria-label="Previous year"]'
local ANO_SEGUINTE = 'mat-calendar-header button[aria-label="Next year"]'
local CALENDARIO = "mat-year-view button"
local BAIXAR_POR_ROTULO = 'button[aria-label*="baixar seu contracheque"],'
  .. ' button[title*="baixar seu contracheque"]'
local BAIXAR_POSICIONAL = "#interna > div:nth-of-type(1) > div:nth-of-type(3)"
  .. " > div:nth-of-type(8) > div:nth-of-type(2) > button:nth-of-type(1)"

local NOMES = {
  "janeiro", "fevereiro", "março", "abril", "maio", "junho",
  "julho", "agosto", "setembro", "outubro", "novembro", "dezembro",
}

-- ------------------------------------------------------------------ parâmetros

local P = parametros or {}
local hoje_aaaa_mm = string.sub(util.hoje(), 1, 7)
local de = P.de or hoje_aaaa_mm
local ate = P.ate or de

local function e_mes(texto)
  return type(texto) == "string" and string.match(texto, "^%d%d%d%d%-%d%d$") ~= nil
end

if not e_mes(de) or not e_mes(ate) then
  error("os parâmetros de e ate têm de ser meses no formato AAAA-MM")
end
if P.forcar == true then
  navegador.registrar("baixa forçada: o inventário aceita de novo o que já tem")
end

-- Espera de 1 s (não há verbo de espera pura: um seletor que nunca existe expira em 1 s).
local function esperar_um_segundo()
  navegador.esperar("#__espera-de-um-segundo__", 1)
end

-- ------------------------------------------------------------------ entrada

-- Entra só se preciso. Termina na home (`app-home`). CPF ou senha recusados: a entrada é
-- recusada (suspende a agenda do roteiro até você retomar). Tela inesperada: passa a vez.
local function entrar()
  navegador.abrir(URL_INICIAL)
  if not navegador.esperar(CAMPO_CPF .. ", " .. HOME, 30) then
    error("o SouCaixa não mostrou a tela de entrada nem a página inicial")
  end
  if navegador.existe(HOME) then
    navegador.registrar("a sessão do SouCaixa ainda valia: sem novo login")
    return
  end

  -- 1. CPF (o único do chaveiro).
  navegador.preencher_cpf(CAMPO_CPF)
  navegador.clicar(BOTAO_PROXIMO)

  -- 2. Validação: receber o código no e-mail (nunca no telefone).
  if not navegador.esperar(RADIO_EMAIL, 20) then
    if navegador.existe(CAMPO_CPF) then
      navegador.entrada_recusada("o SouCaixa não aceitou a entrada")
    end
    navegador.pausa(
      "O SouCaixa mostrou uma tela inesperada (CAPTCHA ou código do aplicativo?). "
        .. "Resolva no navegador, deixe na tela de validação e continue."
    )
    if not navegador.esperar(RADIO_EMAIL, 20) then
      error("o SouCaixa não mostrou a escolha do e-mail para o código")
    end
  end
  navegador.clicar(RADIO_EMAIL)
  navegador.clicar(BOTAO_LOGIN)

  -- 3. Código: o motor o busca no Gmail (remetente e formato vêm do manifesto) e o digita.
  if not navegador.esperar(CAMPO_CODIGO, 30) then
    error("o SouCaixa não pediu o código de validação")
  end
  navegador.preencher_codigo_do_email(CAMPO_CODIGO, { janela_segundos = 180 })
  navegador.clicar(BOTAO_LOGIN)

  -- 4. Senha (vem DEPOIS do código).
  if not navegador.esperar(CAMPO_SENHA, 30) then
    if navegador.existe(CAMPO_CODIGO) then
      -- Código recusado não é CPF nem senha: erro comum, sem suspender a agenda.
      error("o SouCaixa não aceitou o código de validação")
    end
    navegador.pausa("O SouCaixa mostrou uma tela inesperada depois do código. Resolva e continue.")
  end
  navegador.preencher_senha(CAMPO_SENHA, ROTULO_DA_SENHA)
  navegador.clicar(BOTAO_ENTRAR)

  -- 5. Home. Um aviso transitório "Ocorreu um erro inesperado" some sozinho: é ignorado.
  if not navegador.esperar(HOME, 20) then
    if navegador.existe(CAMPO_SENHA) then
      navegador.entrada_recusada("o SouCaixa não aceitou a entrada")
    end
    if not navegador.esperar(HOME, 40) then
      error("o SouCaixa não abriu a página inicial depois da senha")
    end
  end
end

-- ------------------------------------------------------------------ contracheque

-- Abre a rota do contracheque. Se a sessão caiu no meio (a tela de CPF aparece), entra de novo
-- uma vez. Devolve se o alternador do calendário apareceu.
local function abrir_contracheque(ano, n)
  local url = string.format("https://soucaixa.caixa.gov.br/contraCheque/%02d/%04d", n, ano)
  for _ = 1, 2 do
    navegador.abrir(url)
    if navegador.esperar(ALTERNADOR .. ", " .. CAMPO_CPF, 30) then
      if navegador.existe(ALTERNADOR) then
        return true
      end
      entrar()
    end
  end
  return false
end

-- O texto do contracheque mostrado agora (minúsculas), ou "" se nenhum aparece.
local function conteudo_mostrado()
  if not navegador.existe("#interna") then
    return ""
  end
  return string.lower(navegador.texto("#interna"))
end

-- Escolhe o mês no calendário e baixa o PDF. Devolve "baixado", "ausente" (mês sem contracheque
-- publicado ou fora do período do portal) ou "sem_pagina" (o portal não mostrou a página).
local function baixar_mes(mes)
  local ano = tonumber(string.sub(mes, 1, 4))
  local n = tonumber(string.sub(mes, 6, 7))
  local nome = NOMES[n]
  if not abrir_contracheque(ano, n) then
    navegador.registrar("o SouCaixa não mostrou a página do contracheque de " .. mes)
    return "sem_pagina"
  end

  -- O calendário: abre só se ainda está fechado (o alternador de um aberto o fecharia).
  if not navegador.existe(CALENDARIO) then
    navegador.clicar(ALTERNADOR)
  end
  if not navegador.esperar(CALENDARIO, 10) then
    navegador.registrar("o calendário do SouCaixa não abriu para " .. mes)
    return "sem_pagina"
  end

  -- Leva o cabeçalho ao ano certo (teto de 6 cliques).
  for _ = 1, 6 do
    local visto = tonumber(string.match(navegador.texto(ANO_DO_CABECALHO), "%d%d%d%d"))
    if visto == nil or visto == ano then
      break
    end
    if visto > ano then
      navegador.clicar(ANO_ANTERIOR)
    else
      navegador.clicar(ANO_SEGUINTE)
    end
  end

  -- A célula do mês, habilitada. Desabilitada ou ausente: não há contracheque a baixar, e o
  -- calendário fica como está (não se clica em lugar nenhum para fechá-lo).
  local celula = 'mat-year-view button[aria-label="' .. nome .. " de " .. ano .. '"]'
    .. ':not([disabled]):not([aria-disabled="true"])'
  if not navegador.existe(celula) then
    navegador.registrar(
      "contracheque de " .. mes .. " ainda não publicado ou fora do período do portal"
    )
    return "ausente"
  end
  navegador.clicar(celula)

  -- Confirma que o contracheque mostrado é o do mês desejado ANTES de baixar (o conteúdo de
  -- outro mês pode ainda estar na tela enquanto o portal carrega).
  local desejado = nome .. " " .. ano
  local confere = false
  for _ = 1, 15 do
    if string.find(conteudo_mostrado(), desejado, 1, true) then
      confere = true
      break
    end
    esperar_um_segundo()
  end
  if not confere then
    navegador.registrar(
      "contracheque de " .. mes .. " ainda não publicado ou fora do período do portal"
    )
    return "ausente"
  end

  -- O botão "Baixar PDF": pelo rótulo; o caminho posicional é a reserva.
  local botao = nil
  if navegador.esperar(BAIXAR_POR_ROTULO, 15) then
    botao = BAIXAR_POR_ROTULO
  elseif navegador.esperar(BAIXAR_POSICIONAL, 3) then
    botao = BAIXAR_POSICIONAL
  end
  if botao == nil then
    navegador.registrar(
      "contracheque de " .. mes .. " ainda não publicado ou fora do período do portal"
    )
    return "ausente"
  end

  local arquivo = navegador.baixar(botao, "contracheque-" .. mes .. ".pdf")
  navegador.registrar("contracheque de " .. mes .. " baixado: " .. arquivo)
  return "baixado"
end

-- ------------------------------------------------------------------ roteiro

-- util.meses devolve os meses de `de` a `ate` em ordem (erro se de > ate; no máximo 60).
local meses = util.meses(de, ate)

entrar()

local baixados = 0
local sem_pagina = 0
for _, alvo in ipairs(meses) do
  local resultado = baixar_mes(alvo)
  if resultado == "baixado" then
    baixados = baixados + 1
  elseif resultado == "sem_pagina" then
    sem_pagina = sem_pagina + 1
  end
end

navegador.registrar("contracheques baixados: " .. baixados .. " de " .. #meses)

-- Erro só se nenhuma etapa de infraestrutura funcionou (o portal não mostrou página nenhuma).
if sem_pagina == #meses then
  error("o SouCaixa não mostrou a página do contracheque em nenhum mês")
end
