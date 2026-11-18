-- Boleto: acha a linha digitável (47 dígitos de cobrança bancária ou 48 de convênio),
-- confere os dígitos verificadores e extrai o valor e o vencimento dela. Linha com dígito
-- verificador errado (comum em OCR) não é descartada: vira reconhecimento para revisar, sem
-- valor nem vencimento tirados dela.

local function soma_modulo_10(digitos)
  -- Pesos 2,1,2,1... da direita para a esquerda; produto maior que 9 soma os algarismos.
  local soma = 0
  local peso = 2
  for i = #digitos, 1, -1 do
    local produto = tonumber(string.sub(digitos, i, i)) * peso
    if produto > 9 then
      produto = produto - 9
    end
    soma = soma + produto
    peso = peso == 2 and 1 or 2
  end
  return soma
end

local function dv_modulo_10(digitos)
  return (10 - soma_modulo_10(digitos) % 10) % 10
end

-- Módulo 11 com pesos 2..9 da direita para a esquerda. `da_cobranca`: o DV geral do código
-- de barras (resto 0, 1 ou 10 vira 1); `convenio`: o DV de bloco de arrecadação (vira 0).
local function dv_modulo_11(digitos, da_cobranca)
  local soma = 0
  local peso = 2
  for i = #digitos, 1, -1 do
    soma = soma + tonumber(string.sub(digitos, i, i)) * peso
    peso = peso == 9 and 2 or peso + 1
  end
  local dv = 11 - soma % 11
  if da_cobranca then
    if dv == 0 or dv == 10 or dv == 11 then
      return 1
    end
    return dv
  end
  if dv >= 10 then
    return 0
  end
  return dv
end

-- Fator de vencimento -> data. O fator recomeçou em 1000 em 22/02/2025; fatores altos
-- (5000 ou mais) ainda são do ciclo anterior, que vai de 07/10/1997 a 21/02/2025.
local function vencimento_do_fator(fator)
  if fator == 0 then
    return nil
  end
  if fator >= 5000 then
    return data_do_dia(dias_desde_1970(1997, 10, 7) + fator)
  end
  return data_do_dia(dias_desde_1970(2025, 2, 22) + fator - 1000)
end

-- 47 dígitos. Devolve { valor, vencimento } ou nil e o motivo.
local function ler_bancaria(d)
  local campo1, dv1 = string.sub(d, 1, 9), string.sub(d, 10, 10)
  local campo2, dv2 = string.sub(d, 11, 20), string.sub(d, 21, 21)
  local campo3, dv3 = string.sub(d, 22, 31), string.sub(d, 32, 32)
  if dv_modulo_10(campo1) ~= tonumber(dv1) or dv_modulo_10(campo2) ~= tonumber(dv2)
      or dv_modulo_10(campo3) ~= tonumber(dv3) then
    return nil, "dígito verificador de um dos campos da linha digitável não confere"
  end
  local dv_geral = string.sub(d, 33, 33)
  -- Código de barras: banco+moeda (4), DV, fator+valor (14), campo livre (25).
  local barras = string.sub(d, 1, 4) .. string.sub(d, 34, 47)
    .. string.sub(d, 5, 9) .. campo2 .. campo3
  if dv_modulo_11(barras, true) ~= tonumber(dv_geral) then
    return nil, "dígito verificador geral do boleto não confere"
  end
  local fator = tonumber(string.sub(d, 34, 37))
  local valor = tonumber(string.sub(d, 38, 47))
  return { valor = valor, vencimento = vencimento_do_fator(fator) }
end

-- 48 dígitos: quatro blocos de 11 dígitos + DV. O identificador de valor (3o dígito) diz o
-- módulo: 6 e 7 usam o 10, 8 e 9 usam o 11.
local function ler_convenio(d)
  if string.sub(d, 1, 1) ~= "8" then
    return nil, "boleto de convênio deve começar por 8"
  end
  local modulo = tonumber(string.sub(d, 3, 3))
  local barras = ""
  for bloco = 0, 3 do
    local corpo = string.sub(d, bloco * 12 + 1, bloco * 12 + 11)
    local dv = tonumber(string.sub(d, bloco * 12 + 12, bloco * 12 + 12))
    local esperado
    if modulo == 6 or modulo == 7 then
      esperado = dv_modulo_10(corpo)
    elseif modulo == 8 or modulo == 9 then
      esperado = dv_modulo_11(corpo, false)
    else
      return nil, "identificador de valor desconhecido na linha de convênio"
    end
    if esperado ~= dv then
      return nil, "dígito verificador de um dos blocos da linha de convênio não confere"
    end
    barras = barras .. corpo
  end
  local valor = nil
  if modulo == 6 or modulo == 8 then
    valor = tonumber(string.sub(barras, 5, 15))
  end
  return { valor = valor, vencimento = nil }
end

local function candidatas(texto)
  local achadas = {}
  for linha in string.gmatch(texto, "[^\r\n]+") do
    local digitos = string.gsub(linha, "%D", "")
    if #digitos == 47 or #digitos == 48 then
      table.insert(achadas, digitos)
    end
  end
  return achadas
end

function transformar(entrada, parametros, ajuda)
  local texto = entrada.texto
  if texto == nil then
    return { corresponde = false }
  end
  local achadas = candidatas(texto)
  if #achadas == 0 then
    return { corresponde = false }
  end
  local motivos = {}
  for _, digitos in ipairs(achadas) do
    local lido, motivo
    if #digitos == 47 then
      lido, motivo = ler_bancaria(digitos)
    else
      lido, motivo = ler_convenio(digitos)
    end
    if lido ~= nil then
      local campos = { linha_digitavel = digitos }
      local evidencias = { "digito-verificador-conferido" }
      if lido.valor ~= nil and lido.valor > 0 then
        campos.valor_centavos = lido.valor
      end
      if lido.vencimento ~= nil then
        campos.vencimento = lido.vencimento
        campos.data = lido.vencimento
        table.insert(evidencias, "vencimento-do-fator")
      end
      -- O que a linha diz tem de bater com o que está impresso no boleto.
      local impresso = entrada.campos or {}
      local avisos = {}
      if impresso.vencimento ~= nil and lido.vencimento ~= nil and impresso.vencimento ~= lido.vencimento then
        table.insert(avisos, "o vencimento da linha digitável difere do impresso")
      end
      if impresso.valor_centavos ~= nil and campos.valor_centavos ~= nil
          and impresso.valor_centavos ~= campos.valor_centavos then
        table.insert(avisos, "o valor da linha digitável difere do impresso")
      end
      return { corresponde = true, campos = campos, revisar = #avisos > 0, motivos = avisos, evidencias = evidencias }
    end
    table.insert(motivos, motivo)
  end
  return {
    corresponde = true,
    revisar = true,
    campos = { linha_digitavel = achadas[1] },
    motivos = motivos,
    evidencias = { "digito-verificador-invalido" },
  }
end
