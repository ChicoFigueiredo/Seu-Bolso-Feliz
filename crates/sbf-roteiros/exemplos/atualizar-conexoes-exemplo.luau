-- MODELO (só placeholders) de roteiro de "atualizar conexão": abre o painel de conexões e
-- clica em "Atualizar" em cada uma. O host, a URL e TODOS os seletores abaixo são inventados;
-- troque pelos do painel real antes de ativar. O manifesto declara `classe:
-- "atualizar_conexao"`, a única que libera clicar em Atualizar/Sincronizar. Pagar, transferir,
-- contratar e alterar cadastro continuam bloqueados.

navegador.abrir("https://portal-de-conexoes.exemplo.test/")

-- Se o painel pedir entrada, quem entra é o usuário (MFA manual): o roteiro pausa.
if navegador.esperar("#entrar-no-painel", 5) then
  navegador.pausa("Entre na sua conta no navegador e continue")
end

if not navegador.esperar("#conexoes", 30) then
  error("o painel de conexões não apareceu")
end

-- Uma linha por conexão: `.conexao:nth-child(N) .atualizar` (placeholder). O teto de 50 evita
-- laço sem fim se o painel mudar.
local atualizadas = 0
for i = 1, 50 do
  local botao = ".conexao:nth-child(" .. i .. ") .atualizar"
  if not navegador.existe(botao) then
    break
  end
  navegador.clicar(botao)
  atualizadas = atualizadas + 1

  -- O banco pode pedir consentimento: isso é sempre do usuário.
  if navegador.esperar("#consentimento", 5) then
    navegador.pausa("O banco pediu consentimento: autorize no navegador e continue")
  end

  -- Espera a conexão terminar de atualizar antes da próxima.
  navegador.esperar(".conexao:nth-child(" .. i .. ") .atualizada", 60)
end

navegador.registrar("conexões atualizadas: " .. tostring(atualizadas))
