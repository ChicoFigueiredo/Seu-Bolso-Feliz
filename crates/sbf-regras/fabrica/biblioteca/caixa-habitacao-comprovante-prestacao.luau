-- CAIXA Habitação · Comprovante de pagamento de prestação: só monta a linha de resumo do
-- cartão do Reconhecimento. Não altera nem recalcula nenhum campo.
function transformar(entrada, parametros, ajuda)
  local campos = entrada.campos or {}
  local partes = {}
  local function juntar(rotulo, chave)
    if campos[chave] ~= nil then
      table.insert(partes, rotulo .. " " .. formatar_reais(campos[chave]))
    end
  end
  juntar("Amortização", "amortizacao_centavos")
  juntar("Juros", "juros_centavos")
  juntar("Taxas", "taxas_centavos")
  juntar("Saldo devedor", "saldo_devedor_centavos")
  local exibir = {}
  if #partes > 0 then
    exibir = { table.concat(partes, " · ") }
  end
  return { corresponde = true, exibir = exibir, evidencias = { "resumo-da-prestacao" } }
end
