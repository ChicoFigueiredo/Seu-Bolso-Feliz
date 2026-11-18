-- Completa a `data` de um comprovante cuja data vem por extenso ("12 NOV 2026 - 14:32"),
-- que a extração declarativa (só dd/mm/aaaa) não lê. Só preenche quando a regra não achou a
-- data; nunca troca uma data já extraída.
function transformar(entrada, parametros, ajuda)
  local campos = entrada.campos or {}
  local texto = entrada.texto
  if campos.data ~= nil then
    return { corresponde = true }
  end
  if texto == nil then
    return { corresponde = true, revisar = true, motivos = { "sem a data do comprovante" } }
  end
  local data = achar_data(texto, ajuda)
  if data == nil then
    return { corresponde = true, revisar = true, motivos = { "não achei a data do comprovante" } }
  end
  return { corresponde = true, campos = { data = data }, evidencias = { "data-por-extenso" } }
end
