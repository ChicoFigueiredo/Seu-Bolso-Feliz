-- Recibo Uber: a data do recibo quando vem por extenso ("12 de novembro de 2026"), que a
-- extração declarativa (dd/mm/aaaa) não lê. Só preenche quando a regra não achou a data.
function transformar(entrada, parametros, ajuda)
  local campos = entrada.campos or {}
  local texto = entrada.texto
  if campos.data ~= nil then
    return { corresponde = true, campos = { moeda = "brl" } }
  end
  local data = nil
  if texto ~= nil then
    data = achar_data(texto, ajuda)
  end
  if data == nil then
    return {
      corresponde = true,
      revisar = true,
      campos = { moeda = "brl" },
      motivos = { "não achei a data do recibo" },
    }
  end
  return { corresponde = true, campos = { data = data, moeda = "brl" }, evidencias = { "data-por-extenso" } }
end
