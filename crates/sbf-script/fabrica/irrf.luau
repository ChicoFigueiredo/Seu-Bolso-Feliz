-- Conferência do IRRF do contracheque (spec 13, D8; AC-LUA-11).
--
-- Só confere: calcula o imposto da tabela oficial (parametros.irrf, versionada por
-- competência) sobre a base impressa no contracheque e compara com o que foi retido. Nunca
-- altera a rubrica. Tudo em centavos inteiros; o arredondamento é do host (mul_div_arred).
--
-- Entrada (entrada.campos):
--   base_irrf_centavos              base de cálculo impressa no contracheque
--   irrf_retido_centavos            soma das rubricas de IR do mês (sem PLR)
--   rendimento_tributavel_centavos  (opcional) rendimentos tributáveis sujeitos à incidência
--                                   mensal, para o redutor da Lei 15.270/2025
-- Parâmetros: parametros.irrf (faixas, redutor, versao).

local function imposto_da_tabela(base, irrf, ajuda)
  local faixa = nil
  for _, f in ipairs(irrf.faixas) do
    if f.teto_centavos == nil or base <= f.teto_centavos then
      faixa = f
      break
    end
  end
  if faixa == nil then
    return nil
  end
  local bruto = ajuda.mul_div_arred(base, faixa.aliquota_ppm, 1000000) - faixa.parcela_deduzir_centavos
  return math.max(0, bruto)
end

-- Devolve o redutor em centavos, ou nil quando não dá para saber (falta o rendimento).
local function redutor(imposto, base, rendimento, irrf, ajuda)
  local r = irrf.redutor
  if rendimento == nil then
    -- O rendimento tributável é sempre >= a base; acima do teto do redutor ele é zero.
    if base > r.ate_centavos then
      return 0
    end
    return nil
  end
  if rendimento <= r.zera_ate_centavos then
    return math.min(imposto, r.maximo_centavos)
  end
  if rendimento <= r.ate_centavos then
    local valor = r.fixo_centavos - ajuda.mul_div_arred(rendimento, r.fator_ppm, 1000000)
    return math.min(imposto, math.max(0, valor))
  end
  return 0
end

function transformar(entrada, parametros, ajuda)
  local campos = entrada.campos or {}
  local irrf = parametros.irrf
  local base = campos.base_irrf_centavos
  if base == nil or irrf == nil then
    return {
      corresponde = true,
      revisar = true,
      motivos = { base == nil and "falta a base do IRRF no contracheque" or "falta a tabela do IRRF" },
    }
  end
  local evidencias = { "rubrica_irrf", "tabela:irrf:" .. tostring(irrf.versao) }
  local imposto = imposto_da_tabela(base, irrf, ajuda)
  if imposto == nil then
    return { corresponde = true, revisar = true, motivos = { "a tabela do IRRF não tem faixa para a base" }, evidencias = evidencias }
  end
  local desconto = redutor(imposto, base, campos.rendimento_tributavel_centavos, irrf, ajuda)
  if desconto == nil then
    return {
      corresponde = true,
      revisar = true,
      motivos = { "falta o rendimento tributável para o redutor" },
      evidencias = evidencias,
    }
  end
  local estimado = imposto - desconto
  local retido = campos.irrf_retido_centavos
  if retido == nil then
    return {
      corresponde = true,
      revisar = true,
      campos = { irrf_estimado_centavos = estimado },
      motivos = { "falta o IRRF retido para conferir" },
      evidencias = evidencias,
    }
  end
  local diferenca = retido - estimado
  return {
    corresponde = true,
    campos = { irrf_estimado_centavos = estimado, irrf_diferenca_centavos = diferenca },
    revisar = diferenca ~= 0,
    motivos = diferenca ~= 0 and { "o IRRF retido difere do estimado pela tabela" } or {},
    evidencias = evidencias,
  }
end
