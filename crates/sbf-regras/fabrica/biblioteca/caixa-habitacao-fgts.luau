-- CAIXA Habitação · Uso do FGTS: resume o abatimento do saldo devedor com o FGTS. Só monta a
-- linha do cartão do Reconhecimento.
function transformar(entrada, parametros, ajuda)
  local campos = entrada.campos or {}
  local partes = { "FGTS" }
  local modalidade = campos.modalidade
  local valor = campos.valor_utilizado_centavos
  if valor ~= nil then
    local rotulo = "utilizado"
    if modalidade ~= nil then
      rotulo = modalidade
    end
    table.insert(partes, rotulo .. " " .. formatar_reais(valor))
  end
  if campos.data ~= nil then
    table.insert(partes, data_br(campos.data))
  end
  if campos.saldo_devedor_centavos ~= nil then
    table.insert(partes, "Saldo devedor " .. formatar_reais(campos.saldo_devedor_centavos))
  end
  return {
    corresponde = true,
    exibir = { table.concat(partes, " · ") },
    evidencias = { "uso-do-fgts" },
  }
end
