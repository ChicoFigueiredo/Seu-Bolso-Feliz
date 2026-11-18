-- Completa o `vencimento` de uma fatura cuja data vem por extenso ("15 DEZ 2026") ou sem
-- ano ("15 DEZ"), que a extração declarativa (só dd/mm/aaaa) não lê. Só preenche quando a
-- regra não achou o vencimento. Sem ano, usa o primeiro ano de 4 dígitos (20xx) do texto.
function transformar(entrada, parametros, ajuda)
  local campos = entrada.campos or {}
  local texto = entrada.texto
  if campos.vencimento ~= nil then
    return { corresponde = true }
  end
  if texto == nil then
    return { corresponde = true, revisar = true, motivos = { "sem o vencimento da fatura" } }
  end
  local lista = linhas(texto)
  local trecho = trecho_apos(lista, "vencimento", ajuda)
  local vencimento = nil
  if trecho ~= nil then
    vencimento = achar_data(trecho, ajuda)
    if vencimento == nil then
      local ano = string.match(texto, "%f[%d](20%d%d)%f[%D]")
      local normalizado = normalizar(ajuda, trecho)
      for dia, palavra in string.gmatch(normalizado, "(%d%d?) +(%a+)") do
        local mes = mes_da_palavra(palavra)
        if mes ~= nil and ano ~= nil then
          vencimento = montar_data(dia, mes, ano)
          break
        end
      end
    end
  end
  if vencimento == nil then
    return { corresponde = true, revisar = true, motivos = { "não achei o vencimento da fatura" } }
  end
  return {
    corresponde = true,
    campos = { vencimento = vencimento },
    evidencias = { "vencimento-por-extenso" },
  }
end
