-- Invoice/receipt em inglês (Anthropic, OpenAI): valor em dólar no padrão americano
-- ("$1,234.56"), a data de pagamento por extenso ("November 12, 2026") e a moeda. Guarda o
-- valor em `valor_usd_centavos` (nunca em `valor_centavos`, que é reais) e a moeda em
-- `moeda`. Sem conversão para reais: a conciliação com a fatura do cartão é de quem usa.
function transformar(entrada, parametros, ajuda)
  local texto = entrada.texto
  if texto == nil then
    return { corresponde = false }
  end
  local lista = linhas(texto)
  local valor = nil
  for _, rotulo in ipairs({ "amount paid", "amount due", "total" }) do
    local trecho = trecho_apos(lista, rotulo, ajuda)
    if trecho ~= nil then
      valor = achar_dinheiro(trecho, true)
      if valor ~= nil then
        break
      end
    end
  end
  local data = nil
  for _, rotulo in ipairs({ "date paid", "date of issue", "paid on", "date" }) do
    local trecho = trecho_apos(lista, rotulo, ajuda)
    if trecho ~= nil then
      data = achar_data(trecho, ajuda)
      if data ~= nil then
        break
      end
    end
  end
  local campos = { moeda = "usd" }
  local motivos = {}
  if valor ~= nil then
    campos.valor_usd_centavos = valor
  else
    table.insert(motivos, "não achei o valor pago")
  end
  if data ~= nil then
    campos.data = data
  else
    table.insert(motivos, "não achei a data do pagamento")
  end
  local numero = string.match(texto, "[Rr]eceipt [Nn]umber%s*:?%s*([%w%-]+)")
    or string.match(texto, "[Ii]nvoice [Nn]umber%s*:?%s*([%w%-]+)")
  if numero ~= nil then
    campos.numero_do_documento = numero
  end
  return {
    corresponde = true,
    campos = campos,
    revisar = #motivos > 0,
    motivos = motivos,
    evidencias = { "valor-em-usd" },
  }
end
