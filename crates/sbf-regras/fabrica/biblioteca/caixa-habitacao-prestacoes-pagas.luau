-- CAIXA Habitação · Prestações pagas: a lista "Janeiro ... Prestação paga R$ ... dd/mm" do
-- app. Devolve um campo por mês pago (mes_01_centavos ... mes_12_centavos, mes_NN_pago_em
-- como "dd/mm"), o total e a quantidade de meses pagos. Mês sem "Prestação paga" não ganha
-- campo: ficou em aberto. Valores só em centavos inteiros.
local NOMES_DOS_MESES = {
  "janeiro", "fevereiro", "marco", "abril", "maio", "junho",
  "julho", "agosto", "setembro", "outubro", "novembro", "dezembro",
}

-- O número do mês se a linha normalizada começa por um nome de mês.
local function mes_da_linha(normalizada)
  for numero, nome in ipairs(NOMES_DOS_MESES) do
    if normalizada == nome or string.sub(normalizada, 1, #nome + 1) == nome .. " " then
      return numero
    end
  end
  return nil
end

function transformar(entrada, parametros, ajuda)
  local texto = entrada.texto
  if texto == nil then
    return { corresponde = false }
  end
  local lista = linhas(texto)
  local campos = {}
  local total, pagos = 0, 0
  local mes_atual = nil
  local ultimo_pago = nil
  for i, linha in ipairs(lista) do
    local normalizada = normalizar(ajuda, linha)
    local mes = mes_da_linha(normalizada)
    if mes ~= nil then
      mes_atual = mes
    end
    if mes_atual ~= nil and string.find(normalizada, "prestacao paga", 1, true) ~= nil then
      local trecho = linha
      local seguinte = lista[i + 1]
      if seguinte ~= nil and mes_da_linha(normalizar(ajuda, seguinte)) == nil then
        trecho = linha .. " " .. seguinte
      end
      local valor = achar_dinheiro(trecho, false)
      local chave = string.format("mes_%02d", mes_atual)
      if valor ~= nil and campos[chave .. "_centavos"] == nil then
        campos[chave .. "_centavos"] = valor
        local dia, mes_do_pagamento = string.match(trecho, "(%d%d)/(%d%d)")
        if dia ~= nil then
          campos[chave .. "_pago_em"] = dia .. "/" .. mes_do_pagamento
        end
        total = total + valor
        pagos = pagos + 1
        ultimo_pago = mes_atual
      end
    end
  end
  if pagos == 0 then
    return { corresponde = false }
  end
  campos.total_centavos = total
  campos.meses_pagos = pagos
  local ano = string.match(texto, "%f[%d](20%d%d)%f[%D]")
  if ano ~= nil then
    campos.ano = tonumber(ano)
  end
  local exibir = { string.format("%d prestações pagas · total %s", pagos, formatar_reais(total)) }
  local grupo = {}
  for numero = 1, 12 do
    local valor = campos[string.format("mes_%02d_centavos", numero)]
    if valor ~= nil then
      table.insert(grupo, ABREVIATURAS[numero] .. " " .. formatar_reais(valor))
    end
    if #grupo == 4 or (numero == 12 and #grupo > 0) then
      table.insert(exibir, table.concat(grupo, " · "))
      grupo = {}
    end
  end
  return {
    corresponde = true,
    campos = campos,
    exibir = exibir,
    evidencias = { "lista-de-prestacoes-pagas", "ultimo-mes-pago:" .. tostring(ultimo_pago) },
  }
end
