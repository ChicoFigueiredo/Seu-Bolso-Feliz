-- Funções comuns às regras da biblioteca de fábrica. O sbf cola este arquivo antes do
-- script de cada regra, então tudo aqui é `local` e o script usa os nomes direto.
-- Só dados do documento entram; nada aqui lê arquivo, rede ou relógio.

local MESES = {
  jan = 1, fev = 2, feb = 2, mar = 3, abr = 4, apr = 4, mai = 5, may = 5, jun = 6, jul = 7,
  ago = 8, aug = 8, set = 9, sep = 9, out = 10, oct = 10, nov = 11, dez = 12, dec = 12,
}

local ABREVIATURAS = { "Jan", "Fev", "Mar", "Abr", "Mai", "Jun", "Jul", "Ago", "Set", "Out", "Nov", "Dez" }

-- 123456 -> "R$ 1.234,56" (negativo com sinal na frente).
local function formatar_reais(centavos)
  local sinal = ""
  if centavos < 0 then
    sinal = "-"
    centavos = -centavos
  end
  local inteiro = tostring(centavos // 100)
  local cent = centavos % 100
  local invertido = string.reverse(inteiro)
  local com_pontos = string.gsub(invertido, "(%d%d%d)", "%1.")
  local agrupado = string.reverse(com_pontos)
  if string.sub(agrupado, 1, 1) == "." then
    agrupado = string.sub(agrupado, 2)
  end
  return string.format("%sR$ %s,%02d", sinal, agrupado, cent)
end

-- "2026-11-09" -> "09/11/2026".
local function data_br(iso)
  local a, m, d = string.match(iso, "^(%d%d%d%d)%-(%d%d)%-(%d%d)$")
  if a == nil then
    return iso
  end
  return d .. "/" .. m .. "/" .. a
end

-- "1.234,56" ou "1,234.56" -> 123456. Só vale com exatamente duas casas decimais: tira
-- tudo o que não é dígito e os dois últimos são os centavos.
local function dinheiro(texto)
  local digitos = string.gsub(texto, "%D", "")
  if #digitos < 3 then
    return nil
  end
  return tonumber(digitos)
end

-- O primeiro valor no padrão brasileiro ("1.234,56") ou americano ("1,234.56") do texto.
local function achar_dinheiro(texto, americano)
  local padrao = "(%d[%d%.]*,%d%d)"
  if americano then
    padrao = "(%d[%d,]*%.%d%d)"
  end
  local achado = string.match(texto, padrao)
  if achado == nil then
    return nil
  end
  return dinheiro(achado)
end

-- Minúsculas, sem acento, só letras, dígitos e espaço simples ("vencimento:" vira
-- "vencimento"; "November 12, 2026" vira "november 12 2026"). A `ajuda.normalizar` do host
-- só tira acento e junta espaços, então a pontuação é tirada aqui.
local function normalizar(ajuda, texto)
  local n = ajuda.normalizar(texto)
  n = string.gsub(n, "[^%w ]", " ")
  n = string.gsub(n, " +", " ")
  return string.match(n, "^ *(.-) *$")
end

local function montar_data(dia, mes, ano)
  dia, mes, ano = tonumber(dia), tonumber(mes), tonumber(ano)
  if dia == nil or mes == nil or ano == nil or dia < 1 or dia > 31 or mes < 1 or mes > 12 then
    return nil
  end
  return string.format("%04d-%02d-%02d", ano, mes, dia)
end

local function mes_da_palavra(palavra)
  if #palavra < 3 then
    return nil
  end
  return MESES[string.sub(palavra, 1, 3)]
end

-- A primeira data do texto, em AAAA-MM-DD: "09/11/2026", "9 de novembro de 2026",
-- "09 NOV 2026" ou "November 9, 2026".
local function achar_data(texto, ajuda)
  local d, m, a = string.match(texto, "(%d%d)/(%d%d)/(%d%d%d%d)")
  if d ~= nil then
    local iso = montar_data(d, m, a)
    if iso ~= nil then
      return iso
    end
  end
  local n = normalizar(ajuda, texto)
  for dia, palavra, ano in string.gmatch(n, "(%d%d?) +de +(%a+) +de +(%d%d%d%d)") do
    local mes = mes_da_palavra(palavra)
    if mes ~= nil then
      return montar_data(dia, mes, ano)
    end
  end
  for dia, palavra, ano in string.gmatch(n, "(%d%d?) +(%a+) +(%d%d%d%d)") do
    local mes = mes_da_palavra(palavra)
    if mes ~= nil then
      return montar_data(dia, mes, ano)
    end
  end
  for palavra, dia, ano in string.gmatch(n, "(%a+) +(%d%d?) +(%d%d%d%d)") do
    local mes = mes_da_palavra(palavra)
    if mes ~= nil then
      return montar_data(dia, mes, ano)
    end
  end
  return nil
end

local function linhas(texto)
  local lista = {}
  for linha in string.gmatch(texto, "[^\r\n]+") do
    table.insert(lista, linha)
  end
  return lista
end

-- A primeira linha cujo texto normalizado contém `rotulo` (também normalizado) como palavra
-- inteira ("total" não casa "subtotal"), junto com a seguinte: o valor fica na linha do
-- rótulo ou logo abaixo.
local function trecho_apos(lista, rotulo, ajuda)
  for i, linha in ipairs(lista) do
    if string.find(" " .. normalizar(ajuda, linha) .. " ", " " .. rotulo .. " ", 1, true) ~= nil then
      return linha .. "\n" .. (lista[i + 1] or "")
    end
  end
  return nil
end

-- Dias desde 1970-01-01 (dia 0) de uma data do calendário gregoriano.
local function dias_desde_1970(ano, mes, dia)
  if mes <= 2 then
    ano = ano - 1
  end
  local era = ano // 400
  local ano_da_era = ano - era * 400
  local mes_ajustado = mes + (mes > 2 and -3 or 9)
  local dia_do_ano = (153 * mes_ajustado + 2) // 5 + dia - 1
  local dia_da_era = ano_da_era * 365 + ano_da_era // 4 - ano_da_era // 100 + dia_do_ano
  return era * 146097 + dia_da_era - 719468
end

local function data_do_dia(dias)
  dias = dias + 719468
  local era = dias // 146097
  local dia_da_era = dias - era * 146097
  local ano_da_era = (dia_da_era - dia_da_era // 1460 + dia_da_era // 36524 - dia_da_era // 146096) // 365
  local ano = ano_da_era + era * 400
  local dia_do_ano = dia_da_era - (365 * ano_da_era + ano_da_era // 4 - ano_da_era // 100)
  local mp = (5 * dia_do_ano + 2) // 153
  local dia = dia_do_ano - (153 * mp + 2) // 5 + 1
  local mes = mp + (mp < 10 and 3 or -9)
  if mes <= 2 then
    ano = ano + 1
  end
  return string.format("%04d-%02d-%02d", ano, mes, dia)
end
