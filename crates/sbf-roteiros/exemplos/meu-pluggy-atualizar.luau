-- Atualiza as conexões do Meu Pluggy (https://meu.pluggy.ai), uma a uma.
--
-- Observado no site real em 2026-11-25 (assistido):
--  * /connections lista as conexões; cada item é um link "Banco Hoje Ver detalhes";
--  * a tela de detalhes (/connections/<id>) tem, lado a lado, "Atualizar" (1º botão) e
--    "Excluir" (2º botão). O roteiro confere o TEXTO do botão antes de clicar, e a lista de
--    bloqueio da classe `atualizar_conexao` só libera o texto exato "Atualizar";
--  * "Atualizar" abre, em menos de 1 s, o widget Pluggy Connect: um <iframe> com a classe
--    `zoid-pluggy-connect-widget-<hash>-visible` ("Atualizando", sem pedir consentimento nem
--    login do banco) que some sozinho em até ~50 s.
--
-- O login do Meu Pluggy é por link no e-mail (ou Google), nunca por senha: este roteiro não
-- digita segredo. Se a sessão do perfil dedicado expirou, ele PAUSA e passa a vez a você.
-- Entre antes com: sbf roteiros escrito entrar --url https://meu.pluggy.ai/login
-- Pedido de consentimento ou de login de um banco dentro do widget também é só seu.

local LISTA = "https://meu.pluggy.ai/connections"
local MAXIMO_DE_CONEXOES = 30

local function link(i)
  return "#__next > div:nth-of-type(1) > main:nth-of-type(1) > section:nth-of-type(1)"
    .. " > div:nth-of-type(2) > a:nth-of-type(" .. i .. ")"
end

-- Primeiro botão do bloco de ações: "Atualizar". O segundo é "Excluir" e nunca é tocado.
local ATUALIZAR = "#__next > div:nth-of-type(1) > main:nth-of-type(1) > div:nth-of-type(1)"
  .. " > div:nth-of-type(1) > div:nth-of-type(2) > button:nth-of-type(1)"

-- A primeira conta carregada da conexão: sinal de que a tela terminou de carregar.
local CONTAS_CARREGADAS = "#__next > div:nth-of-type(1) > main:nth-of-type(1)"
  .. " > section:nth-of-type(2) > div:nth-of-type(2) > div:nth-of-type(1) > button:nth-of-type(1)"

local WIDGET = 'iframe[class*="zoid-pluggy-connect-widget"]'

local function aparar(texto)
  return (string.match(texto, "^%s*(.-)%s*$"))
end

-- Abre a lista. Se a conexão não aparece, a sessão provavelmente expirou: pausa uma vez.
local function abrir_lista()
  navegador.abrir(LISTA)
  if navegador.esperar(link(1), 25) then
    return
  end
  navegador.pausa(
    "O Meu Pluggy não mostrou as conexões (sessão expirada?). Entre na sua conta no "
      .. "navegador (link no e-mail) e continue."
  )
  navegador.abrir(LISTA)
  if not navegador.esperar(link(1), 25) then
    error("o Meu Pluggy não mostrou a lista de conexões")
  end
end

local atualizadas = 0
local falhas = 0
local total = 0

abrir_lista()
-- Quantas conexões a lista tem (o teto evita laço sem fim se o site mudar).
for i = 1, MAXIMO_DE_CONEXOES do
  if not navegador.existe(link(i)) then
    break
  end
  total = i
end
navegador.registrar("conexões na lista: " .. tostring(total))

for i = 1, total do
  if i > 1 then
    abrir_lista()
  end
  local nome = aparar(navegador.texto(link(i)))
  navegador.clicar(link(i))

  if not navegador.esperar(ATUALIZAR, 30) then
    navegador.registrar("sem botão Atualizar em: " .. nome)
    falhas = falhas + 1
  else
    -- Deixa a tela terminar de carregar (melhor esforço: segue mesmo se não vier).
    navegador.esperar(CONTAS_CARREGADAS, 40)

    -- Atualizar e Excluir ficam lado a lado: só clica se o texto é exatamente "Atualizar".
    if aparar(navegador.texto(ATUALIZAR)) ~= "Atualizar" then
      error("o primeiro botão não é 'Atualizar': o site mudou, revise o roteiro")
    end
    navegador.clicar(ATUALIZAR)

    if not navegador.esperar(WIDGET, 10) then
      navegador.registrar("o widget de atualização não abriu em: " .. nome)
      falhas = falhas + 1
    elseif navegador.esperar_sumir(WIDGET, 120) then
      navegador.registrar("atualizada: " .. nome)
      atualizadas = atualizadas + 1
    else
      -- Ainda aberto: pode estar pedindo o banco (consentimento/login). É só seu.
      navegador.pausa(
        "A atualização de " .. nome .. " não terminou. Se o banco pediu consentimento ou "
          .. "login, resolva no navegador e continue."
      )
      if navegador.esperar_sumir(WIDGET, 120) then
        atualizadas = atualizadas + 1
      else
        navegador.registrar("a atualização não terminou em: " .. nome)
        falhas = falhas + 1
      end
    end
  end
end

navegador.registrar(
  "conexões atualizadas: " .. tostring(atualizadas) .. " de " .. tostring(total)
)
if falhas > 0 then
  error(tostring(falhas) .. " conexão(ões) não atualizaram; veja o histórico")
end
