-- Roteiro escrito de exemplo: baixa o contracheque do mês no portal FICTÍCIO
-- (portal.exemplo.test). Quem manda aqui é o manifesto (contracheque-exemplo.json): ele diz
-- os hosts permitidos, o rótulo da senha no chaveiro e de quem vem o código por e-mail.
-- O roteiro nunca vê senha, CPF nem código: só pede ao motor que os digite.

-- 1. Abre a tela de entrada (a URL tem de estar nos `hosts` do manifesto).
navegador.abrir("https://portal.exemplo.test/entrar")
if not navegador.esperar("#cpf", 20) then
  error("a tela de entrada do portal não apareceu")
end

-- 2. Entra. O CPF vem do chaveiro; a senha também, pelo rótulo (o mesmo do manifesto).
navegador.preencher_cpf("#cpf")
navegador.preencher_senha("#senha", "senha:portal-exemplo")
navegador.clicar("#entrar")

-- 3. Segunda etapa: o portal manda um código por e-mail. O motor busca o código no Gmail
-- (remetente e formato vêm do manifesto) e o digita direto no campo.
if navegador.esperar("#codigo", 20) then
  navegador.preencher_codigo_do_email("#codigo", { janela_segundos = 120 })
  navegador.clicar("#confirmar")
elseif navegador.existe("#erro") then
  -- Sem segredo na mensagem: só o fato.
  error("o portal recusou a entrada (CPF ou senha)")
end
-- Se o portal pedisse o código do aplicativo autenticador (TOTP) ou CAPTCHA, o certo é parar e
-- passar a vez ao usuário: navegador.pausa("Resolva o CAPTCHA no navegador e continue").

-- 4. Espera a lista de contracheques.
if not navegador.esperar("#lista", 30) then
  error("a lista de contracheques não apareceu")
end

-- 5. Acha o mês corrente (fuso do usuário) e, se ele ainda não saiu, o anterior.
-- util.hoje() devolve "AAAA-MM-DD"; os sete primeiros caracteres são "AAAA-MM".
local hoje = util.hoje()
local mes_atual = string.sub(hoje, 1, 7)
local mes_anterior = string.sub(util.somar_meses(hoje, -1), 1, 7)

-- Cada linha da lista tem um link `a.baixar` com id "baixar-AAAA-MM".
local function link_do_mes(mes)
  return "#baixar-" .. mes
end

local mes = nil
if navegador.existe(link_do_mes(mes_atual)) then
  mes = mes_atual
elseif navegador.existe(link_do_mes(mes_anterior)) then
  mes = mes_anterior
end

-- Mês ainda não publicado: não é erro. Registra e sai; a agenda tenta de novo.
if mes == nil then
  navegador.registrar(
    "contracheque de " .. mes_atual .. " ainda não publicado (e o de " .. mes_anterior ..
    " também não); tento na próxima execução"
  )
  return
end

-- 6. Baixa. `baixar` já clica no link e espera o arquivo; devolve só o nome.
-- Não há clique em "Pagar boleto" nem em "Alterar cadastro": a lista de bloqueio recusaria.
local arquivo = navegador.baixar(link_do_mes(mes), "contracheque-" .. mes .. ".pdf")
navegador.registrar("contracheque de " .. mes .. " baixado: " .. arquivo)
